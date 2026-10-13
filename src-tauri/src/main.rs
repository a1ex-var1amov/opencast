// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hyperkey;
mod platform;
mod window_mgmt;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opencast_core::engine::{fill_selection, Builtin, Engine, LayoutOp, Plan};
use opencast_core::providers::{ChatEvent, ChatRequest, Message, ProviderInfo, Role};
use opencast_core::scripts::{self, OutputMode};
use opencast_core::{apps, Command, Config};
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::mpsc;

const MAIN: &str = "main";
/// The transparent full-monitor overlay for Click Text.
const SEEK: &str = "seek";
const SEARCH_LIMIT: usize = 60;

#[derive(Default)]
struct Hotkeys {
    /// The show/hide-launcher hotkey.
    toggle: Option<Shortcut>,
    /// `[[hotkeys]]` bindings: shortcut → command id.
    bindings: Vec<(Shortcut, String)>,
}

struct AppState {
    engine: Mutex<Arc<Engine>>,
    streams: Mutex<HashMap<String, JoinHandle<()>>>,
    hotkeys: Mutex<Hotkeys>,
}

impl AppState {
    fn engine(&self) -> Arc<Engine> {
        self.engine.lock().unwrap().clone()
    }
}

/// What the UI should do after running a command.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum ExecResult {
    Hide,
    Toast {
        text: String,
    },
    Output {
        title: String,
        text: String,
    },
    Chat {
        prompt: Option<String>,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        auto_send: bool,
    },
    Error {
        message: String,
    },
    /// Open the launcher on this command's argument form.
    Args {
        command: Command,
    },
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

/// Let a window sit over the menu bar (NSStatusWindowLevel), so the launcher
/// and the Click Text pill hang from the top edge of the screen like a
/// notch "island". Below that level macOS clamps windows under the menu bar.
#[cfg(target_os = "macos")]
fn raise_above_menu_bar(w: &WebviewWindow) {
    if let Ok(ns) = w.ns_window() {
        let ns = ns as *mut objc2::runtime::AnyObject;
        unsafe {
            let _: () = objc2::msg_send![ns, setLevel: 25isize];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn raise_above_menu_bar(_w: &WebviewWindow) {}

/// Island placement: centred at the very top of the monitor under the cursor
/// (primary monitor if the cursor can't be found).
fn place_on_cursor_monitor(w: &WebviewWindow) {
    let under_cursor = w
        .cursor_position()
        .map_err(|e| eprintln!("opencast: cursor position: {e}"))
        .ok()
        .and_then(|c| w.monitor_from_point(c.x, c.y).ok().flatten());
    let Some(mon) = under_cursor.or_else(|| w.primary_monitor().ok().flatten()) else {
        eprintln!("opencast: no monitor to place the launcher on");
        let _ = w.center();
        return;
    };
    let (mp, ms) = (mon.position(), mon.size());
    let width = w.outer_size().map(|s| s.width as i32).unwrap_or(0);
    let x = mp.x + (ms.width as i32 - width) / 2;
    if let Err(e) = w.set_position(tauri::PhysicalPosition::new(x, mp.y)) {
        eprintln!("opencast: place launcher: {e}");
    }
}

fn show_launcher(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(w) = main_window(app) {
        raise_above_menu_bar(&w);
        place_on_cursor_monitor(&w);
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit("launcher://shown", ());
    }
}

fn hide_launcher(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.hide();
    }
    // Hiding the app (not just the window) hands focus straight back to the
    // previous app, so window/selection commands see it as frontmost. Not
    // while the Seek overlay is up: that would take it down too.
    #[cfg(target_os = "macos")]
    if !seek_visible(app) {
        let _ = app.hide();
    }
}

fn seek_visible(app: &AppHandle) -> bool {
    app.get_webview_window(SEEK)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Cover the monitor the front window is on and hand the OCR lines to the page.
fn open_seek(app: &AppHandle, scan: &window_mgmt::SeekScan) {
    let Some(w) = app.get_webview_window(SEEK) else { return };
    let m = scan.monitor;
    raise_above_menu_bar(&w);
    let _ = w.set_position(tauri::LogicalPosition::new(m.x, m.y));
    let _ = w.set_size(tauri::LogicalSize::new(m.w, m.h));
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = w.show();
    let _ = w.set_focus();
    // Above the menu bar the window lands exactly at the monitor origin,
    // which is what the page subtracts from screen points.
    let _ = app.emit("seek://scan", scan);
}

fn close_seek(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SEEK) {
        let _ = w.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.hide();
}

fn toggle_launcher(app: &AppHandle) {
    match main_window(app) {
        Some(w) if w.is_visible().unwrap_or(false) => hide_launcher(app),
        Some(_) => show_launcher(app),
        None => {}
    }
}

/// (Re)register the toggle hotkey and every `[[hotkeys]]` binding.
/// Returns human-readable problems; never fails outright.
fn register_hotkeys(app: &AppHandle, engine: &Engine, state: &AppState) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut problems = Vec::new();

    let spec = engine.config.general.hotkey.as_str();
    let toggle = match spec.parse::<Shortcut>() {
        Ok(s) => Some(s),
        Err(e) => {
            problems.push(format!("hotkey `{spec}` is not valid ({e}); using Alt+Space"));
            "Alt+Space".parse::<Shortcut>().ok()
        }
    };
    if let Some(s) = toggle {
        if let Err(e) = gs.register(s) {
            problems.push(format!("could not register hotkey `{spec}`: {e}"));
        }
    }

    let (bindings, warns) = engine.hotkey_bindings();
    problems.extend(warns);
    let mut registered: Vec<(Shortcut, String)> = Vec::new();
    for (key, id) in bindings {
        // `Hyper+X` = all four modifiers, what the hyper key sends.
        let spec = key.replace("Hyper+", "Ctrl+Alt+Cmd+Shift+");
        let shortcut = match spec.parse::<Shortcut>() {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("hotkey `{key}` is not valid ({e})"));
                continue;
            }
        };
        if toggle == Some(shortcut) || registered.iter().any(|(s, _)| *s == shortcut) {
            problems.push(format!("hotkey `{key}` is bound twice; keeping the first binding"));
            continue;
        }
        match gs.register(shortcut) {
            Ok(()) => registered.push((shortcut, id)),
            Err(e) => problems.push(format!("could not register hotkey `{key}`: {e}")),
        }
    }
    *state.hotkeys.lock().unwrap() = Hotkeys {
        toggle,
        bindings: registered,
    };
    problems
}

// ---------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn search(state: State<'_, AppState>, query: String) -> Vec<Command> {
    state.engine().search(&query, SEARCH_LIMIT)
}

#[tauri::command]
fn warnings(state: State<'_, AppState>) -> Vec<String> {
    state.engine().warnings.clone()
}

#[tauri::command]
fn list_providers(state: State<'_, AppState>) -> Vec<ProviderInfo> {
    state.engine().router.list()
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    hide_launcher(&app);
}

/// The overlay picked a point: drop it, give focus back, click there.
#[tauri::command]
async fn seek_click(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    close_seek(&app);
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    window_mgmt::click_at(x, y).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn seek_close(app: AppHandle) {
    close_seek(&app);
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    platform::write_clipboard(&text)
}

/// Copy `text`, hide, and paste it into whatever app had focus.
#[tauri::command]
async fn paste_text(app: AppHandle, text: String) -> Result<(), String> {
    platform::write_clipboard(&text)?;
    hide_launcher(&app);
    platform::paste_into_frontmost().await
}

fn reload(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let engine = Arc::new(Engine::load(&Config::config_dir()));
    for p in register_hotkeys(app, &engine, &state) {
        eprintln!("opencast: {p}");
    }
    let n = engine.registry.len();
    let w = engine.warnings.len();
    *state.engine.lock().unwrap() = engine;
    if w == 0 {
        format!("Reloaded · {n} commands")
    } else {
        format!("Reloaded · {n} commands · {w} warning(s), see Open Config Folder")
    }
}

fn err(e: impl std::fmt::Display) -> ExecResult {
    let message = format!("{e}");
    eprintln!("opencast: {message}"); // `open OpenCast.app --stderr log` to read these
    ExecResult::Error { message }
}

/// Run a command to completion. Shared by the launcher UI (`execute`) and
/// global hotkeys.
async fn run_command(app: AppHandle, id: String, args: Vec<String>) -> ExecResult {
    let engine = app.state::<AppState>().engine();
    let plan = match engine.plan(&id, &args) {
        Ok(p) => p,
        Err(e) => return err(format!("{e:#}")),
    };
    let done = |r: anyhow::Result<()>| match r {
        Ok(()) => ExecResult::Hide,
        Err(e) => err(format!("{e:#}")),
    };
    match plan {
        Plan::LaunchApp(path) => done(apps::launch(&path)),
        Plan::OpenUrl(url) => done(apps::open_path(&url)),
        Plan::Copy(text) => match platform::write_clipboard(&text) {
            Ok(()) => ExecResult::Toast {
                text: format!("Copied {text}"),
            },
            Err(message) => ExecResult::Error { message },
        },
        Plan::Window(action) => {
            hide_launcher(&app);
            match window_mgmt::apply_window_action(&app, action).await {
                Ok(()) => ExecResult::Hide,
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::Seek => {
            hide_launcher(&app);
            match window_mgmt::seek_scan(&app).await {
                Ok(scan) => {
                    open_seek(&app, &scan);
                    ExecResult::Hide
                }
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::Layout(op) => match op {
            LayoutOp::Remember(name) => {
                hide_launcher(&app);
                match window_mgmt::remember_layout(&engine.config_dir, &name).await {
                    Ok(n) => {
                        reload(&app); // the new Restore command appears
                        ExecResult::Toast {
                            text: format!("Remembered “{name}” · {n} windows"),
                        }
                    }
                    Err(e) => err(format!("{e:#}")),
                }
            }
            LayoutOp::Restore(slug) => {
                hide_launcher(&app);
                match window_mgmt::restore_layout(&engine.config_dir, &slug).await {
                    Ok(summary) => ExecResult::Toast { text: summary },
                    Err(e) => err(format!("{e:#}")),
                }
            }
            LayoutOp::Forget(name) => match window_mgmt::forget_layout(&engine.config_dir, &name) {
                Ok(true) => {
                    reload(&app);
                    ExecResult::Toast {
                        text: format!("Forgot “{name}”"),
                    }
                }
                Ok(false) => err(format!("no layout called “{name}”")),
                Err(e) => err(format!("{e:#}")),
            },
        },
        Plan::RunScript { meta, args } => {
            if meta.mode == OutputMode::Silent {
                hide_launcher(&app);
            }
            match scripts::run(&meta, &args, engine.script_timeout()).await {
                Err(e) => err(format!("{e:#}")),
                Ok(out) if !out.success() => ExecResult::Output {
                    title: format!(
                        "{} failed (exit {})",
                        meta.title,
                        out.code.map_or("signal".into(), |c| c.to_string())
                    ),
                    text: [out.stdout.trim(), out.stderr.trim()]
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                },
                Ok(out) => match meta.mode {
                    OutputMode::Silent => ExecResult::Hide,
                    OutputMode::Compact => ExecResult::Toast {
                        text: out.last_line().unwrap_or("Done").to_string(),
                    },
                    OutputMode::FullOutput => ExecResult::Output {
                        title: meta.title.clone(),
                        text: out.stdout,
                    },
                },
            }
        }
        Plan::Chat {
            prompt,
            system,
            provider,
            model,
            needs_selection,
        } => {
            let prompt = match (prompt, needs_selection) {
                (Some(p), true) => {
                    let sel = platform::grab_selection(&app).await;
                    show_launcher(&app);
                    if sel.trim().is_empty() {
                        return err("No text selected. Select text in another app first \
                             (on macOS OpenCast needs Accessibility permission).");
                    }
                    Some(fill_selection(&p, &sel))
                }
                (p, _) => p,
            };
            let auto_send = prompt.is_some();
            ExecResult::Chat {
                prompt,
                system,
                provider,
                model,
                auto_send,
            }
        }
        Plan::Builtin(b) => match b {
            Builtin::Chat => ExecResult::Chat {
                prompt: None,
                system: None,
                provider: None,
                model: None,
                auto_send: false,
            },
            Builtin::Reload => ExecResult::Toast { text: reload(&app) },
            Builtin::OpenConfig => done(apps::open_path(&engine.config_dir.to_string_lossy())),
            Builtin::OpenScripts => {
                let dir = engine.scripts_dir();
                let _ = std::fs::create_dir_all(&dir);
                done(apps::open_path(&dir.to_string_lossy()))
            }
            Builtin::Quit => {
                if engine.config.general.hyperkey {
                    hyperkey::remap_caps_lock(false);
                }
                app.exit(0);
                ExecResult::Hide
            }
        },
    }
}

#[tauri::command]
async fn execute(app: AppHandle, id: String, args: Vec<String>) -> Result<ExecResult, String> {
    let r = run_command(app.clone(), id, args).await;
    // Window/layout commands hide the panel before they run; bring it back so
    // an error or toast is actually seen.
    if !matches!(r, ExecResult::Hide) {
        show_launcher(&app);
    }
    Ok(r)
}

/// A command fired from a global hotkey: run it, and only involve the panel
/// if there is something to show.
fn dispatch_hotkey(app: &AppHandle, id: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = app.state::<AppState>().engine();
        // A command that needs input opens the launcher on its argument form.
        let r = match engine.registry.get(&id) {
            Some(c) if c.arguments.iter().any(|a| !a.optional) => ExecResult::Args { command: c.clone() },
            _ => run_command(app.clone(), id, vec![]).await,
        };
        if !matches!(r, ExecResult::Hide) {
            show_launcher(&app);
            let _ = app.emit("launcher://result", &r);
        }
    });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamArgs {
    request_id: String,
    provider: Option<String>,
    model: Option<String>,
    system: Option<String>,
    messages: Vec<Message>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEvent {
    request_id: String,
    event: ChatEvent,
}

/// Start a streamed completion. Tokens arrive as `ai://event` events tagged
/// with `requestId`; the stream always ends with a `done` or `error` event.
#[tauri::command]
fn ai_stream(app: AppHandle, state: State<'_, AppState>, args: StreamArgs) -> Result<(), String> {
    let engine = state.engine();
    let provider = engine.router.get(args.provider.as_deref()).ok_or_else(|| {
        if engine.router.is_empty() {
            "No AI provider configured. Add a [[providers]] entry to config.toml (Open Config Folder), then Reload Commands.".to_string()
        } else {
            format!("Unknown provider `{}`", args.provider.unwrap_or_default())
        }
    })?;

    let mut messages = Vec::new();
    if let Some(s) = args.system.filter(|s| !s.trim().is_empty()) {
        messages.push(Message::new(Role::System, s));
    }
    messages.extend(args.messages);
    let req = ChatRequest {
        model: args.model.unwrap_or_default(),
        messages,
        ..Default::default()
    };

    let id = args.request_id;
    let app2 = app.clone();
    let id2 = id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<ChatEvent>(128);
        // Producer: talks to the provider; the channel closes when it returns.
        let producer = async move {
            if let Err(e) = provider.stream(req, tx.clone()).await {
                let _ = tx.send(ChatEvent::Error(format!("{e:#}"))).await;
            }
        };
        // Consumer: forwards every event to the webview, guaranteeing a final
        // done/error even if the provider stops early.
        let consumer = {
            let app = app2.clone();
            let id = id2.clone();
            async move {
                let mut finished = false;
                while let Some(ev) = rx.recv().await {
                    finished = !matches!(ev, ChatEvent::Delta(_));
                    let _ = app.emit(
                        "ai://event",
                        AiEvent {
                            request_id: id.clone(),
                            event: ev,
                        },
                    );
                    if finished {
                        break;
                    }
                }
                if !finished {
                    let _ = app.emit(
                        "ai://event",
                        AiEvent {
                            request_id: id,
                            event: ChatEvent::Done,
                        },
                    );
                }
            }
        };
        tokio::join!(producer, consumer);
        app2.state::<AppState>().streams.lock().unwrap().remove(&id2);
    });
    state.streams.lock().unwrap().insert(id, handle);
    Ok(())
}

#[tauri::command]
fn ai_cancel(state: State<'_, AppState>, request_id: String) {
    if let Some(h) = state.streams.lock().unwrap().remove(&request_id) {
        h.abort();
    }
}

// ---------------------------------------------------------------------------

fn main() {
    let engine = Arc::new(Engine::load(&Config::config_dir()));
    for w in &engine.warnings {
        eprintln!("opencast: {w}");
    }
    let keep_on_blur = std::env::var_os("OPENCAST_KEEP_ON_BLUR").is_some();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<AppState>();
                    let hk = state.hotkeys.lock().unwrap();
                    if hk.toggle.as_ref() == Some(shortcut) {
                        drop(hk);
                        toggle_launcher(app);
                        return;
                    }
                    if let Some((_, id)) = hk.bindings.iter().find(|(s, _)| s == shortcut) {
                        let id = id.clone();
                        drop(hk);
                        dispatch_hotkey(app, id);
                    }
                })
                .build(),
        )
        .manage(AppState {
            engine: Mutex::new(engine),
            streams: Mutex::new(HashMap::new()),
            hotkeys: Mutex::new(Hotkeys::default()),
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let state = app.state::<AppState>();
            let engine = state.engine();
            for p in register_hotkeys(app.handle(), &engine, &state) {
                eprintln!("opencast: {p}");
            }
            if engine.config.general.hyperkey {
                hyperkey::remap_caps_lock(true);
                let h = app.handle().clone();
                hyperkey::start(move || toggle_launcher(&h));
            }
            // Show once on launch so a first-time user sees it is running.
            show_launcher(app.handle());
            Ok(())
        })
        .on_window_event(move |window, event| {
            if let WindowEvent::Focused(false) = event {
                if !keep_on_blur {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            search,
            execute,
            warnings,
            list_providers,
            ai_stream,
            ai_cancel,
            hide_window,
            copy_text,
            paste_text,
            seek_click,
            seek_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenCast");
}
