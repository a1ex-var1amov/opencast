//! Carry out an [`opencast_core::Engine`] plan. The panel hides and shows
//! through [`crate::host::emit`]; there is no `AppHandle`.

use opencast_core::engine::{fill_selection, Builtin, LayoutOp, Plan};
use opencast_core::scripts::{self, OutputMode};
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

pub fn reload() -> String {
    let engine = std::sync::Arc::new(opencast_core::engine::Engine::load(&opencast_core::Config::config_dir()));
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

fn hide() {
    host::emit(serde_json::json!({ "type": "hide" }));
}

fn show() {
    host::emit(serde_json::json!({ "type": "show" }));
}

/// Run a command to completion.
pub async fn run_command(id: String, args: Vec<String>) -> ExecResult {
    // Drop the engine lock before any `.await`. `config_dir` and friends are
    // snapshotted because `std::sync::Mutex` cannot be held across await.
    let (plan, config_dir, scripts_dir, timeout, hyperkey) = host::with_engine(|engine| {
        (
            engine.plan(&id, &args),
            engine.config_dir.clone(),
            engine.scripts_dir(),
            engine.script_timeout(),
            engine.config.general.hyperkey,
        )
    });
    let plan = match plan {
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
        Plan::Copy(text) => match crate::platform::write_clipboard(&text) {
            Ok(()) => ExecResult::Toast {
                text: format!("Copied {text}"),
            },
            Err(message) => ExecResult::Error { message },
        },
        Plan::Window(action) => {
            hide();
            match crate::window_mgmt::apply_window_action(action).await {
                Ok(()) => ExecResult::Hide,
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::Seek => {
            hide();
            match crate::window_mgmt::seek_scan().await {
                Ok(scan) => {
                    host::emit(serde_json::json!({ "type": "seek", "scan": scan }));
                    ExecResult::Hide
                }
                Err(e) => err(format!("{e:#}")),
            }
        }
        Plan::Layout(op) => match op {
            LayoutOp::Remember(name) => {
                hide();
                match crate::window_mgmt::remember_layout(&config_dir, &name).await {
                    Ok(n) => {
                        reload(); // the new Restore command appears
                        ExecResult::Toast {
                            text: format!("Remembered “{name}” · {n} windows"),
                        }
                    }
                    Err(e) => err(format!("{e:#}")),
                }
            }
            LayoutOp::Restore(slug) => {
                hide();
                match crate::window_mgmt::restore_layout(&config_dir, &slug).await {
                    Ok(summary) => ExecResult::Toast { text: summary },
                    Err(e) => err(format!("{e:#}")),
                }
            }
            LayoutOp::Forget(name) => match crate::window_mgmt::forget_layout(&config_dir, &name) {
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
            if meta.mode == OutputMode::Silent {
                hide();
            }
            match scripts::run(&meta, &args, timeout).await {
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
                    let sel = crate::platform::grab_selection().await;
                    show();
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
            Builtin::Reload => ExecResult::Toast { text: reload() },
            Builtin::OpenConfig => done(apps::open_path(&config_dir.to_string_lossy())),
            Builtin::OpenScripts => {
                let _ = std::fs::create_dir_all(&scripts_dir);
                done(apps::open_path(&scripts_dir.to_string_lossy()))
            }
            Builtin::Quit => {
                if hyperkey {
                    crate::hyperkey::remap_caps_lock(false);
                }
                std::process::exit(0);
            }
        },
    }
}
