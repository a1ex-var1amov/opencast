use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use opencast_core::engine::Engine;
use opencast_core::providers::{ChatRequest, Message, Role};
use opencast_core::Config;
use serde::Deserialize;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::dispatch;

pub type Callback = extern "C" fn(*const c_char);

static CALLBACK: Mutex<Option<Callback>> = Mutex::new(None);
static ENGINE: OnceLock<Mutex<std::sync::Arc<Engine>>> = OnceLock::new();
static STREAMS: LazyLock<Mutex<HashMap<String, JoinHandle<()>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static VISIBLE: AtomicBool = AtomicBool::new(false);

pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

pub fn emit(value: serde_json::Value) {
    match value.get("type").and_then(|t| t.as_str()) {
        Some("show") => VISIBLE.store(true, Ordering::SeqCst),
        Some("hide") => VISIBLE.store(false, Ordering::SeqCst),
        _ => {}
    }
    let Ok(text) = CString::new(value.to_string()) else {
        return;
    };
    // Drop the callback mutex before the call so the panel can re-enter.
    let cb = *CALLBACK.lock().unwrap();
    if let Some(cb) = cb {
        cb(text.as_ptr());
    }
}

pub(crate) fn toggle_panel() {
    if VISIBLE.load(Ordering::SeqCst) {
        emit(serde_json::json!({ "type": "hide" }));
    } else {
        emit(serde_json::json!({ "type": "show" }));
    }
}

pub fn with_engine<T>(f: impl FnOnce(&Engine) -> T) -> T {
    // Clone the Arc and release the lock at once, so concurrent searches (one per
    // keystroke) run in parallel instead of queueing behind each other.
    let engine = std::sync::Arc::clone(&ENGINE.get().expect("opencast_start was not called").lock().unwrap());
    f(&engine)
}

fn engine_arc() -> std::sync::Arc<Engine> {
    std::sync::Arc::clone(&ENGINE.get().expect("opencast_start was not called").lock().unwrap())
}

/// Re-register the global hotkeys; returns problems newline-joined (empty when fine).
///
/// Carbon's `InstallEventHandler` can fail while AppKit is still coming up, which
/// leaves a perfectly working launcher with dead keys. The panel calls this a beat
/// after the window appears.
#[no_mangle]
pub extern "C" fn opencast_hotkeys_fix() -> *mut std::ffi::c_char {
    let engine = engine_arc();
    let problems = crate::hotkeys::register(&engine);
    let text = problems.join("\n");
    match std::ffi::CString::new(text) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

pub fn replace_engine(engine: std::sync::Arc<Engine>) {
    *ENGINE.get().expect("opencast_start").lock().unwrap() = engine;
}

fn handle(request: &str) -> String {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let v: serde_json::Value = match serde_json::from_str(request) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
        };
        let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
        match op {
            // Swift hid on its own (Esc, blur, ExecResult). Do not emit: the
            // callback would hide again, and emit is what already clears this flag.
            "hidden" => {
                VISIBLE.store(false, Ordering::SeqCst);
                r#"{"ok":true}"#.into()
            }
            "shown" => {
                VISIBLE.store(true, Ordering::SeqCst);
                r#"{"ok":true}"#.into()
            }
            "execute" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let args = v
                    .get("args")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let ctx: crate::exec::ExecContext = serde_json::from_value(v.clone()).unwrap_or_default();
                let r = runtime().block_on(crate::exec::run_command(id, args, ctx));
                // `quiet`: the settings window runs commands without popping the launcher.
                let quiet = v.get("quiet").and_then(|x| x.as_bool()).unwrap_or(false);
                if r.wants_panel() && !quiet {
                    emit(serde_json::json!({ "type": "show" }));
                }
                serde_json::to_string(&r).unwrap_or_else(|_| r#"{"type":"error","message":"internal error"}"#.into())
            }
            "pin" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let pinned = with_engine(|engine| engine.toggle_pin(id));
                serde_json::json!({ "pinned": pinned }).to_string()
            }
            // Shell captured every window for "Remember Layout".
            "layoutSave" => {
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let displays = v.get("displays").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let apps: Vec<opencast_core::layouts::AppWindows> = v
                    .get("apps")
                    .cloned()
                    .and_then(|a| serde_json::from_value(a).ok())
                    .unwrap_or_default();
                match crate::exec::save_layout(&name, &displays, apps) {
                    Ok(text) => serde_json::json!({ "text": text }).to_string(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            // The app is terminating (AppKit hook): undo what outlives the process.
            "shutdown" => {
                crate::hyperkey::restore_caps_lock_if_ours();
                r#"{"ok":true}"#.into()
            }
            "configDir" => {
                // Must not need the engine: the clipboard history asks for this while
                // opencast_start is still loading config, apps and MCP tools.
                serde_json::json!({ "path": opencast_core::config::Config::config_dir().to_string_lossy() }).to_string()
            }
            "copy" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
                match crate::platform::write_clipboard(text) {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "paste" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let r = runtime().block_on(async {
                    crate::platform::write_clipboard(&text)?;
                    emit(serde_json::json!({ "type": "hide" }));
                    crate::platform::paste_into_frontmost().await
                });
                match r {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            // Paste whatever is already on the pasteboard (e.g. restored clip image).
            "pasteFront" => {
                let r = runtime().block_on(async {
                    emit(serde_json::json!({ "type": "hide" }));
                    crate::platform::paste_into_frontmost().await
                });
                match r {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "seekClick" => {
                let x = v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0);
                let y = v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0);
                let r = runtime().block_on(async {
                    emit(serde_json::json!({ "type": "hide" }));
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    crate::window_mgmt::click_at(x, y).map_err(|e| format!("{e:#}"))
                });
                match r {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "seekClose" => {
                emit(serde_json::json!({ "type": "hide" }));
                r#"{"ok":true}"#.into()
            }
            "reload" => serde_json::json!({ "text": crate::exec::reload() }).to_string(),
            "approveTool" => {
                let rid = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("");
                let cid = v.get("callId").and_then(|x| x.as_str()).unwrap_or("");
                let allow = v.get("allow").and_then(|x| x.as_bool()).unwrap_or(false);
                serde_json::json!({ "ok": crate::agent::answer_approval(rid, cid, allow) }).to_string()
            }
            "chatSave" => {
                let chat: Result<opencast_core::chats::SavedChat, _> =
                    serde_json::from_value(v.get("chat").cloned().unwrap_or_default());
                match chat {
                    Ok(c) => {
                        with_engine(|e| e.save_chat(c));
                        r#"{"ok":true}"#.into()
                    }
                    Err(e) => serde_json::json!({ "error": format!("bad chat: {e}") }).to_string(),
                }
            }
            "chatDelete" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                serde_json::json!({ "ok": with_engine(|e| e.delete_chat(id)) }).to_string()
            }
            "noteDelete" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                serde_json::json!({ "ok": with_engine(|e| e.delete_note(id)) }).to_string()
            }
            "profileAppend" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
                match with_engine(|e| e.profile_append(text)) {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            "setHotkey" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("");
                let dir = with_engine(|e| e.config_dir.clone());
                match opencast_core::config::set_command_hotkey(&dir, id, key) {
                    Ok(()) => {
                        crate::exec::reload();
                        r#"{"ok":true}"#.into()
                    }
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            // Notes editor window.
            "notesList" => serde_json::json!({ "notes": with_engine(|e| e.notes()) }).to_string(),
            "noteUpdate" => {
                let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let (id, title, body) = (s("id"), s("title"), s("body"));
                if id.is_empty() {
                    let note = with_engine(|e| e.add_note(&title, &body));
                    serde_json::json!({ "ok": true, "id": note.id }).to_string()
                } else {
                    serde_json::json!({ "ok": with_engine(|e| e.update_note(&id, &title, &body)), "id": id })
                        .to_string()
                }
            }
            "expandSnippet" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
                let (text, back) = crate::exec::expand_with_cursor(text);
                serde_json::json!({ "text": text, "back": back }).to_string()
            }
            // Settings window.
            "configGet" => with_engine(|e| {
                let g = &e.config.general;
                serde_json::json!({
                    "general": {
                        "hotkey": g.hotkey, "hyperkey": g.hyperkey, "presentation": g.presentation,
                        "theme": g.theme, "default_provider": g.default_provider, "integrations": g.integrations,
                        "snippet_expansion": g.snippet_expansion, "shortcuts": g.shortcuts,
                        "calendar": g.calendar, "check_updates": g.check_updates,
                        "update_repo": g.update_repo, "script_login_shell": g.script_login_shell,
                    },
                    "providers": e.config.providers.iter().map(|p| serde_json::json!({
                        "id": p.id, "kind": p.kind, "model": p.model, "baseUrl": p.base_url,
                        "hasKey": p.api_key_cmd.is_some() || p.api_key_env.is_some(), "local": p.is_local(),
                    })).collect::<Vec<_>>(),
                    "mcp": e.config.mcp_servers.iter().map(|m| serde_json::json!({
                        "id": m.id, "command": m.command, "url": m.url,
                    })).collect::<Vec<_>>(),
                    "configDir": e.config_dir.display().to_string(),
                    "version": env!("CARGO_PKG_VERSION"),
                    "synced": opencast_core::sync::is_enabled(&e.config_dir),
                })
                .to_string()
            }),
            "configSet" => {
                let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("");
                let value = match v.get("value") {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                let dir = with_engine(|e| e.config_dir.clone());
                match opencast_core::config::set_general_key(&dir, key, &value) {
                    Ok(()) => serde_json::json!({ "text": crate::exec::reload() }).to_string(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            "providerAdd" => {
                let p: Result<opencast_core::config::ProviderConfig, _> =
                    serde_json::from_value(v.get("provider").cloned().unwrap_or_default());
                let dir = with_engine(|e| e.config_dir.clone());
                match p
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|p| opencast_core::config::add_provider(&dir, &p))
                {
                    Ok(()) => serde_json::json!({ "text": crate::exec::reload() }).to_string(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            "providerRemove" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let dir = with_engine(|e| e.config_dir.clone());
                match opencast_core::config::remove_provider(&dir, id) {
                    Ok(_) => serde_json::json!({ "text": crate::exec::reload() }).to_string(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            // Background update check: reports, never opens anything.
            "updateCheck" => {
                let repo = with_engine(|e| e.config.general.update_repo.clone());
                match runtime().block_on(opencast_core::updates::check(&repo, env!("CARGO_PKG_VERSION"))) {
                    Ok(Some(r)) => serde_json::json!({ "newer": r.version, "url": r.url }).to_string(),
                    Ok(None) => r#"{"newer":null}"#.into(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            // Page text for "Ask About This Page" when the browser gives only the URL.
            "fetchPage" => {
                let url = v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
                match runtime().block_on(opencast_core::webtext::fetch(&url)) {
                    Ok(p) => serde_json::json!({ "title": p.title, "text": p.text }).to_string(),
                    Err(e) => serde_json::json!({ "error": format!("{e:#}") }).to_string(),
                }
            }
            "mcpRefresh" => {
                let (n, errors) = runtime().block_on(crate::agent::refresh_tools());
                serde_json::json!({ "tools": n, "errors": errors }).to_string()
            }
            "stream" => stream(&v),
            "cancel" => cancel(&v),
            _ => with_engine(|engine| dispatch::dispatch(engine, request)),
        }
    }));
    match result {
        Ok(s) => s,
        Err(_) => serde_json::json!({ "error": "internal error" }).to_string(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamArgs {
    request_id: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
    /// Offer MCP tools (default on; silent AI commands turn it off).
    #[serde(default = "yes")]
    tools: bool,
    /// Add `profile.md` to the system prompt (default on; silent commands off).
    #[serde(default = "yes")]
    profile: bool,
    /// `[[ai_presets]]` id: limits tools, may turn the profile off.
    #[serde(default)]
    preset: Option<String>,
    messages: Vec<Message>,
}

fn yes() -> bool {
    true
}

/// Start a streamed completion. Tokens arrive as `ai` callbacks tagged with
/// `requestId`; the stream always ends with a `done` or `error` event.
fn stream(v: &serde_json::Value) -> String {
    let args: StreamArgs = match serde_json::from_value(v.clone()) {
        Ok(a) => a,
        Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
    };
    let provider_id = args.provider.clone();
    let provider = with_engine(|engine| {
        engine.router.get(provider_id.as_deref()).ok_or_else(|| {
            if engine.router.is_empty() {
                "No AI provider configured. Add a [[providers]] entry to config.toml (Open Config Folder), then Reload Commands.".to_string()
            } else {
                format!("Unknown provider `{}`", provider_id.unwrap_or_default())
            }
        })
    });
    let provider = match provider {
        Ok(p) => p,
        Err(e) => return serde_json::json!({ "error": e }).to_string(),
    };

    let (profile, tool_globs) = with_engine(|e| e.chat_context(args.preset.as_deref(), args.profile));
    let mut messages = Vec::new();
    if let Some(s) = opencast_core::engine::system_with_profile(profile.as_deref(), args.system.as_deref()) {
        messages.push(Message::new(Role::System, s));
    }
    messages.extend(args.messages);
    let req = ChatRequest {
        model: args.model.unwrap_or_default(),
        messages,
        temperature: args.temperature,
        max_tokens: args.max_tokens,
        tools: Vec::new(),
    };
    let mut tools = if args.tools {
        with_engine(|e| e.chat_tools())
    } else {
        Vec::new()
    };
    if let Some(globs) = tool_globs {
        tools.retain(|t| globs.iter().any(|g| opencast_core::config::glob_match(g, &t.spec.name)));
    }

    let id = args.request_id;
    let id2 = id.clone();
    let handle = runtime().spawn(async move {
        crate::agent::converse(provider, req, tools, id2.clone()).await;
        STREAMS.lock().unwrap().remove(&id2);
    });
    STREAMS.lock().unwrap().insert(id, handle);
    r#"{"ok":true}"#.into()
}

fn cancel(v: &serde_json::Value) -> String {
    let request_id = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if let Some(h) = STREAMS.lock().unwrap().remove(&request_id) {
        h.abort();
    }
    r#"{"ok":true}"#.into()
}

pub fn install(cb: Callback) {
    *CALLBACK.lock().unwrap() = Some(cb);
    let config_dir = Config::config_dir();
    // Cached app index first so the panel opens at once; a fresh Spotlight
    // scan replaces it in the background.
    let apps = crate::apps_mac::discover_cached(&config_dir);
    let engine = std::sync::Arc::new(Engine::load_with_apps(&config_dir, apps));
    for w in &engine.warnings {
        eprintln!("opencast: {w}");
    }
    let hyperkey = engine.config.general.hyperkey;
    ENGINE.set(Mutex::new(std::sync::Arc::clone(&engine))).ok();
    let _ = runtime();
    crate::hotkeys::listen();
    install_signal_handlers();
    if hyperkey {
        crate::hyperkey::remap_caps_lock(true);
        // The tap callback must return before Swift `hide`. A slow tap is disabled.
        crate::hyperkey::start(|| {
            std::thread::spawn(toggle_panel);
        });
    } else {
        // A previous run that crashed may have left Caps Lock remapped.
        crate::hyperkey::restore_caps_lock_if_ours();
    }
    for p in crate::hotkeys::register(&engine) {
        eprintln!("opencast: {p}");
    }
    // The panel shows itself once its settings (presentation, theme) are applied.
    std::thread::spawn(move || {
        let apps_changed = crate::apps_mac::refresh_cache(&config_dir);
        let shortcuts_changed = crate::shortcuts_mac::refresh_cache(&config_dir);
        let bookmarks_changed = crate::refresh_mac::refresh_bookmarks(&config_dir);
        let currency = with_engine(|e| e.config.general.currency);
        let rates_changed = runtime().block_on(crate::refresh_mac::refresh_rates(&config_dir, currency));
        if apps_changed || shortcuts_changed || bookmarks_changed || rates_changed {
            crate::exec::reload();
        }
        crate::agent::refresh_in_background();
    });
}

/// `pkill`/`kill` skip AppKit's termination hooks; undo the Caps Lock remap
/// ourselves so a hyper-key user is not left with a dead key.
fn install_signal_handlers() {
    runtime().spawn(async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut int = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut hup = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(_) => return,
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
            _ = hup.recv() => {}
        }
        crate::hyperkey::restore_caps_lock_if_ours();
        std::process::exit(0);
    });
}

pub fn call_raw(json: *const c_char) -> *mut c_char {
    let request = unsafe { CStr::from_ptr(json) }.to_string_lossy();
    CString::new(handle(&request))
        .unwrap_or_else(|_| CString::new(r#"{"error":"internal error"}"#).unwrap())
        .into_raw()
}

pub fn free_raw(p: *mut c_char) {
    if !p.is_null() {
        unsafe { drop(CString::from_raw(p)) }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamArgs;

    #[test]
    fn hidden_op_clears_visible() {
        use std::sync::atomic::Ordering;
        super::VISIBLE.store(true, Ordering::SeqCst);
        assert_eq!(super::handle(r#"{"op":"hidden"}"#), r#"{"ok":true}"#);
        assert!(!super::VISIBLE.load(Ordering::SeqCst));
        assert_eq!(super::handle(r#"{"op":"shown"}"#), r#"{"ok":true}"#);
        assert!(super::VISIBLE.load(Ordering::SeqCst));
    }

    #[test]
    fn stream_args_omit_optional_fields() {
        let v = serde_json::json!({
            "op": "stream",
            "requestId": "abc",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let args: StreamArgs = serde_json::from_value(v).unwrap();
        assert_eq!(args.request_id, "abc");
        assert!(args.provider.is_none());
        assert!(args.model.is_none());
        assert!(args.system.is_none());
        assert_eq!(args.messages.len(), 1);
    }
}
