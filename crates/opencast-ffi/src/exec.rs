//! Carry out an [`opencast_core::Engine`] plan. The panel hides and shows
//! through [`crate::host::emit`]; there is no `AppHandle`.
//!
//! Anything that touches other apps' windows (snapping, layouts) is returned
//! as a result for the Swift shell, which talks to the Accessibility API
//! directly — no `osascript` on the hot path.

use opencast_core::config::{self, AiOutput};
use opencast_core::engine::{fill_placeholders, needs_clipboard, Builtin, LayoutOp, Plan, WindowAction};
use opencast_core::layouts::{self, AppWindows};
use opencast_core::scripts::{self, ListItem, OutputMode, RunOptions};
use opencast_core::{apps, Command};

use crate::host;

/// What the UI should do after running a command.
#[derive(Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ExecResult {
    Hide,
    Toast {
        text: String,
    },
    Output {
        title: String,
        text: String,
    },
    /// Like `Output`, rendered as Markdown.
    Markdown {
        title: String,
        text: String,
    },
    Chat {
        prompt: Option<String>,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        auto_send: bool,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        /// `chat`, `replace`, `paste` or `copy`.
        output: AiOutput,
        /// `[[ai_presets]]` id; the panel passes it back when streaming.
        #[serde(skip_serializing_if = "Option::is_none")]
        preset: Option<String>,
    },
    Error {
        message: String,
    },
    /// Open the launcher on this command's argument form.
    Args {
        command: Command,
    },
    Query {
        query: String,
    },
    Theme {
        theme: String,
    },
    Presentation {
        presentation: String,
    },
    StayAwake {
        mode: String,
    },
    RunSelected,
    DoEscape,
    Hint {
        text: String,
    },
    AskAi,
    /// Seek scan finished (or is about to show). Do not hide the panel.
    ClickMode,
    /// Snap the frontmost window (the shell does it through AX).
    /// `frac` is `(x, y, w, h)` of the work area, or null for the special
    /// actions (`center`, `next-display`, `previous-display`, `restore`).
    Window {
        action: String,
        frac: Option<[f64; 4]>,
    },
    /// Layouts: `capture` asks the shell to record every window and call the
    /// `layoutSave` op with `name`; `apply` hands it the windows to place.
    Layout {
        op: String,
        name: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        apps: Vec<AppWindows>,
    },
    /// Script Filter rows. `filter` = the panel filters locally; otherwise it
    /// re-runs the command with `action: "list"` and the query as the argument.
    List {
        command: Command,
        items: Vec<ListItem>,
        filter: bool,
        /// `list` or `grid`.
        #[serde(skip_serializing_if = "Option::is_none")]
        view: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<u8>,
        /// Seconds: re-run the list step while it is open.
        #[serde(skip_serializing_if = "Option::is_none")]
        rerun: Option<f64>,
    },
    /// Quit via the shell so AppKit's termination hooks run.
    Quit,
    /// A panel-side feature: `settings`, `menu-items`, `dictate`, `schedule`,
    /// `contacts`, `define`, `large-type`, `media-play`…, with its input.
    Ui {
        action: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    /// Reopen a saved chat.
    ResumeChat {
        chat: opencast_core::chats::SavedChat,
    },
}

impl ExecResult {
    /// Results that the panel should come to the front to show.
    pub fn wants_panel(&self) -> bool {
        !matches!(
            self,
            ExecResult::Hide | ExecResult::Window { .. } | ExecResult::Layout { .. } | ExecResult::Quit
        ) && !matches!(self, ExecResult::Ui { action, .. } if UI_WITHOUT_PANEL.contains(&action.as_str()))
    }
}

/// Panel-side actions that open their own window or act silently.
const UI_WITHOUT_PANEL: &[&str] = &[
    "hide-others",
    "settings",
    "media-play",
    "media-next",
    "media-previous",
    "spotify-play",
    "spotify-next",
    "spotify-previous",
    "large-type",
    "note-editor",
    "focus-stop",
    "focus-pause",
    "compare",
    "dictate-anywhere",
    "sticky-note",
];

/// Facts about the moment the command ran that only the shell knows.
#[derive(Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecContext {
    /// Selected text in the front app (AX), when the shell could read it.
    #[serde(default)]
    pub selection: Option<String>,
    /// Name of the frontmost app before the panel opened.
    #[serde(default)]
    pub front_app: Option<String>,
    /// Display signature (see `layouts::display_signature`).
    #[serde(default)]
    pub displays: Option<String>,
    /// Script Filter step: `list` (default) or `run`.
    #[serde(default)]
    pub action: Option<String>,
    /// What was typed in the search field when the command was chosen (ranking learns from it).
    #[serde(default)]
    pub query: Option<String>,
    /// AI commands: force where the answer goes ("Run in Chat" action).
    #[serde(default)]
    pub output: Option<AiOutput>,
    /// List rows: the chosen item's variables, passed to the `run` step as env.
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, String>,
}

pub fn reload() -> String {
    let apps = crate::apps_mac::discover();
    let engine = std::sync::Arc::new(opencast_core::engine::Engine::load_with_apps(
        &opencast_core::Config::config_dir(),
        apps,
    ));
    for p in crate::hotkeys::register(&engine) {
        eprintln!("opencast: {p}");
    }
    let n = engine.registry.len();
    let w = engine.warnings.len();
    crate::host::replace_engine(engine);
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

/// Web API search → panel list; empty query opens the arg field; failure falls back to deep link.
async fn spotify_search_list(args: Vec<String>) -> ExecResult {
    let query = args.first().map(|s| s.as_str()).unwrap_or("").trim().to_string();
    if query.is_empty() {
        return match host::with_engine(|e| e.registry.get("builtin:spotify-search").cloned()) {
            Some(command) => ExecResult::Args { command },
            None => err("Spotify Search is not registered"),
        };
    }
    let cfg = host::with_engine(|e| e.config.spotify.clone());
    match opencast_core::spotify::search(&cfg, &query).await {
        Ok(items) if items.is_empty() => ExecResult::Toast {
            text: format!("No Spotify results for “{query}”"),
        },
        Ok(items) => match host::with_engine(|e| e.registry.get("builtin:spotify-search").cloned()) {
            Some(command) => ExecResult::List {
                command,
                items,
                filter: true,
                view: None,
                columns: None,
                rerun: None,
            },
            None => err("Spotify Search is not registered"),
        },
        Err(e) => {
            eprintln!("opencast: spotify list search failed ({e:#}); opening Spotify search");
            hide();
            match apps::open_path(&opencast_core::spotify::search_deep_link(&query)) {
                Ok(()) => ExecResult::Hide,
                Err(e) => err(format!("{e:#}")),
            }
        }
    }
}

fn hide() {
    host::emit(serde_json::json!({ "type": "hide" }));
}

fn show() {
    host::emit(serde_json::json!({ "type": "show" }));
}

/// Run a command to completion.
pub async fn run_command(id: String, args: Vec<String>, ctx: ExecContext) -> ExecResult {
    // Drop the engine lock before any `.await`. `config_dir` and friends are
    // snapshotted because `std::sync::Mutex` cannot be held across await.
    let (plan, config_dir, scripts_dir, timeout, login_shell) = host::with_engine(|engine| {
        (
            engine.plan(&id, &args),
            engine.config_dir.clone(),
            engine.scripts_dir(),
            engine.script_timeout(),
            engine.config.general.script_login_shell,
        )
    });
    let plan = match plan {
        Ok(p) => p,
        Err(e) => return err(format!("{e:#}")),
    };
    // Count the run after the plan resolved (a typo is not a favourite).
    let query = ctx.query.clone().unwrap_or_default();
    host::with_engine(|engine| engine.bump_usage(&id, &query));
    let done = |r: anyhow::Result<()>| match r {
        Ok(()) => ExecResult::Hide,
        Err(e) => err(format!("{e:#}")),
    };
    match plan {
        Plan::LaunchApp(path) => done(apps::launch(&path)),
        Plan::OpenUrl(url) => done(apps::open_path(&url)),
        Plan::Copy(text) => match crate::platform::write_clipboard(&text) {
            Ok(()) => ExecResult::Toast {
                text: format!("Copied {text}"),
            },
            Err(message) => ExecResult::Error { message },
        },
        Plan::Window(action) => window_result(action),
        Plan::WindowKey(key) => ExecResult::Window {
            action: key,
            frac: None,
        },
        // OCR runs in-process in the panel (SeekOCR / Vision). ClickMode keeps
        // the island up; Swift starts the scan when it applies this result.
        Plan::Seek => ExecResult::ClickMode,
        Plan::Layout(op) => match op {
            LayoutOp::Remember(name) => ExecResult::Layout {
                op: "capture".into(),
                name,
                apps: vec![],
            },
            LayoutOp::Restore(slug) => {
                let displays = ctx.displays.clone().unwrap_or_default();
                match layouts::get_for(&config_dir, &slug, &displays) {
                    Some(layout) => ExecResult::Layout {
                        op: "apply".into(),
                        name: layout.name,
                        apps: layout.apps,
                    },
                    None => err(format!("layout `{slug}` is gone — try Reload Commands")),
                }
            }
            LayoutOp::Forget(name) => match layouts::delete(&config_dir, &name) {
                Ok(true) => {
                    reload();
                    ExecResult::Toast {
                        text: format!("Forgot “{name}”"),
                    }
                }
                Ok(false) => err(format!("no layout called “{name}”")),
                Err(e) => err(format!("{e:#}")),
            },
        },
        Plan::RunScript { meta, args } => {
            let mut opts = RunOptions::new(timeout);
            opts.login_shell = login_shell;
            if let Some(app) = &ctx.front_app {
                opts.env.push(("OPENCAST_FRONT_APP".into(), app.clone()));
            }
            if meta.mode == OutputMode::List {
                return run_list_script(&id, &meta, &args, opts, &ctx).await;
            }
            if meta.mode == OutputMode::Silent {
                hide();
            }
            match scripts::run_with(&meta, &args, &opts).await {
                Err(e) => err(format!("{e:#}")),
                Ok(out) if !out.success() => script_failure(&meta.title, &out),
                Ok(out) => script_success(meta.mode, &meta.title, out),
            }
        }
        Plan::Chat {
            prompt,
            system,
            provider,
            model,
            needs_selection,
            temperature,
            max_tokens,
            output,
            preset,
        } => {
            let prompt = match prompt {
                Some(p) => {
                    let selection = if needs_selection {
                        // Prefer what the shell read through AX; fall back to ⌘C.
                        let sel = match ctx.selection.as_deref().map(str::trim) {
                            Some(s) if !s.is_empty() => s.to_string(),
                            _ => {
                                let s = crate::platform::grab_selection().await;
                                show();
                                s
                            }
                        };
                        if sel.trim().is_empty() {
                            return err("No text selected. Select text in another app first \
                                 (on macOS OpenCast needs Accessibility permission).");
                        }
                        sel
                    } else {
                        String::new()
                    };
                    let clipboard = if needs_clipboard(&p) {
                        crate::platform::read_clipboard().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let app = ctx.front_app.clone().unwrap_or_default();
                    Some(fill_placeholders(&p, &selection, &clipboard, &app))
                }
                None => None,
            };
            let auto_send = prompt.is_some();
            let output = ctx.output.unwrap_or(output);
            ExecResult::Chat {
                prompt,
                system,
                provider,
                model,
                auto_send,
                temperature,
                max_tokens,
                output,
                preset,
            }
        }
        Plan::Builtin(b) => match b {
            Builtin::Chat => ExecResult::Chat {
                prompt: None,
                system: None,
                provider: None,
                model: None,
                auto_send: false,
                temperature: None,
                max_tokens: None,
                output: AiOutput::Chat,
                preset: None,
            },
            Builtin::Reload => ExecResult::Toast { text: reload() },
            Builtin::OpenConfig => done(apps::open_path(&config_dir.to_string_lossy())),
            Builtin::OpenScripts => {
                let _ = std::fs::create_dir_all(&scripts_dir);
                done(apps::open_path(&scripts_dir.to_string_lossy()))
            }
            Builtin::Quit => ExecResult::Quit,
            Builtin::SyncICloud => {
                // `sync_dir` (Dropbox, a git checkout…) when set, else iCloud Drive.
                let custom = host::with_engine(|e| e.config.general.sync_dir.clone())
                    .filter(|d| !d.trim().is_empty())
                    .map(|d| config::expand_tilde(std::path::Path::new(d.trim())));
                if let Some(target) = custom {
                    return match opencast_core::sync::enable(&config_dir, &target) {
                        Ok(_) => {
                            reload();
                            ExecResult::Toast {
                                text: format!("Config now syncs via {}", target.display()),
                            }
                        }
                        Err(e) => err(format!("{e:#}")),
                    };
                }
                let Some(target) = opencast_core::sync::icloud_dir() else {
                    return err("no home folder");
                };
                if !target.parent().is_some_and(|p| p.exists()) {
                    return err(
                        "iCloud Drive is not turned on (System Settings → Apple Account → iCloud → iCloud Drive)",
                    );
                }
                match opencast_core::sync::enable(&config_dir, &target) {
                    Ok(r) => {
                        reload();
                        ExecResult::Toast {
                            text: if r.adopted.is_empty() {
                                "Config now syncs via iCloud Drive › OpenCast".into()
                            } else {
                                "Using the config synced from your other Mac (local copy kept as *.local-backup)".into()
                            },
                        }
                    }
                    Err(e) => err(format!("{e:#}")),
                }
            }
            Builtin::CheckUpdates => {
                let repo = host::with_engine(|e| e.config.general.update_repo.clone());
                match opencast_core::updates::check(&repo, env!("CARGO_PKG_VERSION")).await {
                    Ok(Some(r)) => {
                        let _ = apps::open_path(r.download.as_deref().unwrap_or(&r.url));
                        ExecResult::Toast {
                            text: format!("OpenCast {} is available — opening the download", r.version),
                        }
                    }
                    Ok(None) => ExecResult::Toast {
                        text: format!("OpenCast {} is up to date", env!("CARGO_PKG_VERSION")),
                    },
                    Err(e) => err(format!("{e:#}")),
                }
            }
            Builtin::RefreshMcp => {
                let (n, errors) = crate::agent::refresh_tools().await;
                if errors.is_empty() {
                    ExecResult::Toast {
                        text: format!("MCP · {n} tools"),
                    }
                } else {
                    err(format!("MCP · {n} tools · {}", errors.join(" · ")))
                }
            }
            Builtin::Appearance
            | Builtin::PanelPosition
            | Builtin::StayAwake
            | Builtin::Settings
            | Builtin::Shortcuts
            | Builtin::ClipboardHistory
            | Builtin::SearchScreenshots
            | Builtin::PasteLatestScreenshot
            | Builtin::EmojiPicker
            | Builtin::Notes
            | Builtin::NewNote
            | Builtin::ToggleDarkMode
            | Builtin::VolumeUp
            | Builtin::VolumeDown
            | Builtin::VolumeMute
            | Builtin::BrightnessUp
            | Builtin::BrightnessDown
            | Builtin::EmptyTrash
            | Builtin::LockScreen
            | Builtin::Sleep
            | Builtin::EjectAll
            | Builtin::QuitFrontApp
            | Builtin::KillFrontApp
            | Builtin::MenuItems
            | Builtin::ChatHistory
            | Builtin::Dictate
            | Builtin::MySchedule
            | Builtin::InstallExtension
            | Builtin::UpdateExtensions
            | Builtin::ManageExtensions
            | Builtin::BrowseExtensions
            | Builtin::ImportAlfredWorkflow
            | Builtin::Doctor
            | Builtin::Contacts
            | Builtin::Reminders
            | Builtin::NewReminder
            | Builtin::DefineWord
            | Builtin::LargeType
            | Builtin::MediaPlayPause
            | Builtin::MediaNext
            | Builtin::MediaPrevious
            | Builtin::Spotify
            | Builtin::SpotifyPlay
            | Builtin::SpotifyNext
            | Builtin::SpotifyPrevious
            | Builtin::SpotifyNowPlaying
            | Builtin::SpotifySearch
            | Builtin::ActOnSelection
            | Builtin::StartFocus
            | Builtin::StopFocus
            | Builtin::PauseFocus
            | Builtin::SearchBookmarks
            | Builtin::SearchHistory
            | Builtin::EditNote
            | Builtin::Restart
            | Builtin::ShutDown
            | Builtin::LogOut
            | Builtin::ToggleHiddenFiles
            | Builtin::QuitAllApps
            | Builtin::HideOthers
            | Builtin::ShowDesktop
            | Builtin::Spell
            | Builtin::ArrangeWindow
            | Builtin::Translate
            | Builtin::AskPage
            | Builtin::SummarizePage
            | Builtin::CompareModels
            | Builtin::DictateAnywhere
            | Builtin::CheatSheet
            | Builtin::StickyNote
            | Builtin::SaveSnippet
            | Builtin::Stats
            | Builtin::EditProfile
            | Builtin::Welcome
            | Builtin::BrowseFiles => {
                // The engine plans these as Ui/UiArgs/Extensions/Doctor/FillQuery, never Plan::Builtin.
                err("this builtin should have resolved to a more specific plan")
            }
        },
        Plan::Pipeline {
            title,
            input,
            query,
            steps,
            output,
        } => run_pipeline(title, input, query, steps, output, &ctx, timeout, login_shell).await,
        Plan::PasteText(text) => {
            hide();
            let (text, back) = expand_with_cursor(&text);
            match crate::platform::write_clipboard(&text) {
                Ok(()) => match crate::platform::paste_into_frontmost().await {
                    Ok(()) => {
                        crate::platform::move_cursor_left(back);
                        ExecResult::Hide
                    }
                    Err(e) => err(e),
                },
                Err(e) => err(e),
            }
        }
        Plan::RunShortcut { name, input } => {
            hide();
            match crate::shortcuts_mac::run(&name, input.as_deref()).await {
                Ok(out) if out.is_empty() => ExecResult::Hide,
                Ok(out) if out.lines().count() <= 1 && out.chars().count() <= 120 => ExecResult::Toast { text: out },
                Ok(out) => ExecResult::Output { title: name, text: out },
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::McpCall {
            server,
            tool,
            arguments,
        } => match crate::agent::hub().call(&server, &tool, arguments).await {
            Ok(out) => ExecResult::Output {
                title: format!("{server} · {tool}"),
                text: out,
            },
            Err(e) => err(format!("{e:#}")),
        },
        // A resource is a document: show its text (Return copies it).
        Plan::McpReadResource { server, uri } => match crate::agent::hub().read_resource(&server, &uri).await {
            Ok(text) => ExecResult::Markdown {
                title: format!("{server} · {uri}"),
                text,
            },
            Err(e) => err(format!("{e:#}")),
        },
        // The server wrote the prompt; the panel puts it in a chat, unsent, so
        // you can edit it (or attach something) before pressing Return.
        Plan::McpGetPrompt {
            server,
            name,
            arguments,
        } => match crate::agent::hub().get_prompt(&server, &name, arguments).await {
            Ok(text) => ExecResult::Ui {
                action: "mcp-prompt".to_string(),
                args: vec![text],
            },
            Err(e) => err(format!("{e:#}")),
        },
        Plan::ResumeChat(chat) => ExecResult::ResumeChat { chat: *chat },
        Plan::Ui(action) => ExecResult::Ui {
            action: action.to_string(),
            args: vec![],
        },
        Plan::UiArgs {
            action: "spotify-search-list",
            args,
        } => spotify_search_list(args).await,
        Plan::UiArgs { action, args } => ExecResult::Ui {
            action: action.to_string(),
            args,
        },
        Plan::Extensions(op) => crate::extensions_mac::run(op, config_dir, &args, &ctx).await,
        Plan::ImportAlfred(path) => crate::extensions_mac::import(config_dir, path).await,
        Plan::Doctor => crate::extensions_mac::doctor(),
        Plan::SystemShell(cmd) => {
            hide();
            match tokio::process::Command::new("sh").arg("-c").arg(cmd).status().await {
                Ok(s) if s.success() => ExecResult::Hide,
                Ok(_) => err("system command failed"),
                Err(e) => err(e),
            }
        }
        Plan::QuitApp(name) => {
            hide();
            let script = format!(
                "osascript -e 'tell application \"{}\" to quit'",
                name.replace('\\', "\\\\").replace('"', "\\\"")
            );
            match tokio::process::Command::new("sh").arg("-c").arg(script).status().await {
                Ok(s) if s.success() => ExecResult::Hide,
                Ok(_) => err(format!("could not quit {name}")),
                Err(e) => err(e),
            }
        }
        Plan::ShowText { title, text } => ExecResult::Markdown { title, text },
        Plan::AddSnippet { keyword, text } => match config::add_snippet(&config_dir, &keyword, &text, None) {
            Ok(()) => {
                reload();
                ExecResult::Toast {
                    text: format!("Saved snippet {}", keyword.trim()),
                }
            }
            Err(e) => err(format!("{e:#}")),
        },
        Plan::SaveNote { title, body } => {
            if body.trim().is_empty() {
                return err("note body is empty");
            }
            let note = host::with_engine(|engine| engine.add_note(&title, &body));
            let label = if note.title.is_empty() {
                "untitled"
            } else {
                note.title.as_str()
            };
            ExecResult::Toast {
                text: format!("Saved note · {label}"),
            }
        }
        Plan::SetTheme(theme) => match config::set_general_key(&config_dir, "theme", theme) {
            Ok(()) => ExecResult::Theme {
                theme: theme.to_string(),
            },
            Err(e) => err(format!("{e:#}")),
        },
        Plan::SetPresentation(presentation) => {
            match config::set_general_key(&config_dir, "presentation", presentation) {
                Ok(()) => ExecResult::Presentation {
                    presentation: presentation.to_string(),
                },
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::StayAwake(mode) => ExecResult::StayAwake { mode: mode.to_string() },
        Plan::FillQuery(query) => ExecResult::Query { query },
        Plan::RunSelected => ExecResult::RunSelected,
        Plan::DoEscape => ExecResult::DoEscape,
        Plan::Hint(text) => ExecResult::Hint { text },
        Plan::AskAi => ExecResult::AskAi,
    }
}

fn window_result(action: WindowAction) -> ExecResult {
    if action == WindowAction::SpaceSwitcher {
        // The space list is live state the engine never sees; the panel reads it.
        return ExecResult::Ui {
            action: "spaces".to_string(),
            args: Vec::new(),
        };
    }
    ExecResult::Window {
        action: action.key().to_string(),
        frac: action.frac().map(|(x, y, w, h)| [x, y, w, h]),
    }
}

fn script_failure(title: &str, out: &scripts::ScriptOutput) -> ExecResult {
    ExecResult::Output {
        title: format!(
            "{} failed (exit {})",
            title,
            out.code.map_or("signal".into(), |c| c.to_string())
        ),
        text: [out.stdout.trim(), out.stderr.trim()]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

/// Run a `[[pipelines]]` entry: each step's output is the next step's input.
#[allow(clippy::too_many_arguments)]
async fn run_pipeline(
    title: String,
    input: opencast_core::pipeline::Input,
    query: Option<String>,
    steps: Vec<opencast_core::pipeline::Step>,
    output: opencast_core::pipeline::Output,
    ctx: &ExecContext,
    timeout: std::time::Duration,
    login_shell: bool,
) -> ExecResult {
    use opencast_core::pipeline::{fill_prompt, Input, Output, Step};
    use opencast_core::providers::{ChatRequest, Message, Role};
    let mut text = match input {
        Input::Selection => match ctx.selection.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                let s = crate::platform::grab_selection().await;
                if s.trim().is_empty() {
                    return err("No text selected. Select text in another app first.");
                }
                s
            }
        },
        Input::Clipboard => crate::platform::read_clipboard().unwrap_or_default(),
        Input::Query => query.unwrap_or_default(),
        Input::None => String::new(),
    };
    let total = steps.len();
    for (i, step) in steps.into_iter().enumerate() {
        let label = step.label();
        if total > 1 {
            host::emit(serde_json::json!({
                "type": "progress",
                "text": format!("{title}: {}/{total} · {label}…", i + 1),
            }));
        }
        let fail = |e: String| err(format!("{title}, step {} ({label}): {e}", i + 1));
        text = match step {
            Step::Transform(t) => match t.apply(&text) {
                Ok(t) => t,
                Err(e) => return fail(e),
            },
            Step::Ai {
                prompt,
                system,
                provider,
                model,
                temperature,
                max_tokens,
                ..
            } => {
                let p = host::with_engine(|e| e.router.get(provider.as_deref()));
                let Some(p) = p else {
                    return fail("no AI provider configured".into());
                };
                let mut messages = Vec::new();
                if let Some(sys) = system.filter(|s| !s.trim().is_empty()) {
                    messages.push(Message::new(Role::System, sys));
                }
                messages.push(Message::new(Role::User, fill_prompt(&prompt, &text)));
                let req = ChatRequest {
                    model: model.unwrap_or_default(),
                    messages,
                    temperature,
                    max_tokens,
                    tools: Vec::new(),
                };
                match opencast_core::providers::complete(p.as_ref(), req).await {
                    Ok(t) => t.trim().to_string(),
                    Err(e) => return fail(format!("{e:#}")),
                }
            }
            Step::Script(meta) => {
                let mut opts = RunOptions::new(timeout);
                opts.login_shell = login_shell;
                match scripts::run_with(&meta, &[text.clone()], &opts).await {
                    Ok(out) if out.success() => out.stdout.trim_end().to_string(),
                    Ok(out) => return fail(out.stderr.trim().to_string()),
                    Err(e) => return fail(format!("{e:#}")),
                }
            }
            Step::Shell(cmd) => {
                let input = text.clone();
                let run = tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    let mut child = std::process::Command::new("/bin/sh")
                        .arg("-c")
                        .arg(&cmd)
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()?;
                    if let Some(mut stdin) = child.stdin.take() {
                        // A writer thread, so a command that prints before reading cannot deadlock.
                        std::thread::spawn(move || {
                            let _ = stdin.write_all(input.as_bytes());
                        });
                    }
                    child.wait_with_output()
                });
                match tokio::time::timeout(timeout, run).await {
                    Err(_) => return fail("timed out".into()),
                    Ok(Err(e)) => return fail(e.to_string()),
                    Ok(Ok(Err(e))) => return fail(e.to_string()),
                    Ok(Ok(Ok(o))) if !o.status.success() => {
                        return fail(String::from_utf8_lossy(&o.stderr).trim().to_string())
                    }
                    Ok(Ok(Ok(o))) => String::from_utf8_lossy(&o.stdout).trim_end().to_string(),
                }
            }
        };
    }
    match output {
        Output::Show => ExecResult::Output { title, text },
        Output::Copy => match crate::platform::write_clipboard(&text) {
            Ok(()) => ExecResult::Toast {
                text: format!("{title}: copied"),
            },
            Err(e) => err(e),
        },
        Output::Paste | Output::Replace => {
            hide();
            match crate::platform::write_clipboard(&text) {
                Ok(()) => match crate::platform::paste_into_frontmost().await {
                    Ok(()) => ExecResult::Hide,
                    Err(e) => err(e),
                },
                Err(e) => err(e),
            }
        }
    }
}

fn script_success(mode: OutputMode, title: &str, out: scripts::ScriptOutput) -> ExecResult {
    match mode {
        OutputMode::Silent => ExecResult::Hide,
        OutputMode::Compact | OutputMode::List => ExecResult::Toast {
            text: out.last_line().unwrap_or("Done").to_string(),
        },
        OutputMode::Markdown => ExecResult::Markdown {
            title: title.to_string(),
            text: out.stdout,
        },
        OutputMode::FullOutput => ExecResult::Output {
            title: title.to_string(),
            text: out.stdout,
        },
    }
}

/// Script Filter: `list` prints rows, `run` acts on the chosen one.
async fn run_list_script(
    id: &str,
    meta: &scripts::ScriptMeta,
    args: &[String],
    mut opts: RunOptions,
    ctx: &ExecContext,
) -> ExecResult {
    let action = ctx.action.as_deref().unwrap_or("list");
    opts.env.push(("OPENCAST_ACTION".into(), action.to_string()));
    for (k, v) in &ctx.variables {
        // Never let a list row override the basics.
        if !matches!(k.as_str(), "PATH" | "HOME" | "SHELL" | "OPENCAST_ACTION") {
            opts.env.push((k.clone(), v.clone()));
        }
    }
    if action == "run" {
        if meta.action_mode == OutputMode::Silent {
            hide();
        }
        return match scripts::run_with(meta, args, &opts).await {
            Err(e) => err(format!("{e:#}")),
            Ok(out) if !out.success() => script_failure(&meta.title, &out),
            Ok(out) => script_success(meta.action_mode, &meta.title, out),
        };
    }
    match scripts::run_with(meta, args, &opts).await {
        Err(e) => err(format!("{e:#}")),
        Ok(out) if !out.success() => script_failure(&meta.title, &out),
        Ok(out) => match scripts::parse_list(&out.stdout) {
            Ok(list) => {
                let command = host::with_engine(|engine| engine.registry.get(id).cloned());
                let base = meta.path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
                let items = list
                    .items
                    .into_iter()
                    .map(|mut item| {
                        item.resolve_icon(&base);
                        // Top-level variables apply to every row; the row's own win.
                        for (k, v) in &list.variables {
                            item.variables.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                        item
                    })
                    .collect();
                match command {
                    Some(command) => ExecResult::List {
                        command,
                        items,
                        filter: meta.filter,
                        view: list.view.or_else(|| meta.grid.then(|| "grid".to_string())),
                        columns: list.columns,
                        rerun: list.rerun,
                    },
                    None => err("command not found — try Reload Commands"),
                }
            }
            Err(e) => ExecResult::Output {
                title: format!("{} printed something that is not a list", meta.title),
                text: format!("{e:#}\n\n{}", out.stdout.chars().take(2000).collect::<String>()),
            },
        },
    }
}

/// Expand snippet placeholders (`{date}`, `{time}`, `{datetime}`,
/// `{clipboard}`); `{cursor}` is removed and the number of characters
/// after it is returned, so the caller can move the caret back there.
pub fn expand_with_cursor(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let date = |fmt: &str| {
        std::process::Command::new("/bin/date")
            .arg(fmt)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    if out.contains("{datetime}") {
        out = out.replace("{datetime}", &date("+%Y-%m-%d %H:%M"));
    }
    if out.contains("{date}") {
        out = out.replace("{date}", &date("+%Y-%m-%d"));
    }
    if out.contains("{time}") {
        out = out.replace("{time}", &date("+%H:%M"));
    }
    if out.contains("{clipboard}") {
        out = out.replace("{clipboard}", &crate::platform::read_clipboard().unwrap_or_default());
    }
    match out.find("{cursor}") {
        Some(i) => {
            let after = out[i + "{cursor}".len()..].chars().count();
            (out.replacen("{cursor}", "", 1), after)
        }
        None => (out, 0),
    }
}

pub fn save_layout(name: &str, displays: &str, apps: Vec<AppWindows>) -> Result<String, String> {
    let count: usize = apps.iter().map(|a| a.windows.len()).sum();
    if count == 0 {
        return Err("no windows to remember".into());
    }
    let dir = host::with_engine(|engine| engine.config_dir.clone());
    layouts::save(&dir, name, displays, apps).map_err(|e| format!("{e:#}"))?;
    reload(); // the new Restore command appears
    Ok(format!("Remembered “{}” · {count} windows", name.trim()))
}
