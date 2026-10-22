//! Extensions, Alfred import and Doctor: the parts that need macOS tools
//! (`plutil` for plists, `ditto` for zips) or the live engine.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use opencast_core::engine::ExtensionOp;
use opencast_core::scripts::{ItemAction, ListItem};
use opencast_core::{alfred, extensions};

use crate::exec::{reload, ExecContext, ExecResult};
use crate::host;

fn expand(p: &str) -> PathBuf {
    let p = p.trim().trim_matches('"').trim_matches('\'');
    match p.strip_prefix("~/") {
        Some(rest) => dirs_home().join(rest),
        None => PathBuf::from(p),
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `info.plist` → JSON via `plutil` (handles binary and XML plists).
fn plist_json(path: &Path) -> Result<serde_json::Value> {
    let out = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(path)
        .output()
        .context("running plutil")?;
    if !out.status.success() {
        bail!(
            "could not read {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).context("info.plist is not valid")
}

/// Generate commands for every workflow under `dir`. Returns how many commands.
fn generate_all(dir: &Path) -> (usize, Vec<String>) {
    let mut n = 0;
    let mut errors = Vec::new();
    for wf in alfred::find_workflows(dir) {
        match plist_json(&wf.join("info.plist")).and_then(|p| alfred::generate(&wf, &p)) {
            Ok(paths) => n += paths.len(),
            Err(e) => errors.push(format!("{}: {e:#}", wf.display())),
        }
    }
    (n, errors)
}

/// Import an `.alfredworkflow` file or a workflow folder into `extensions/`.
/// Import an `.alfredworkflow` (commands) or `.alfredsnippets` (snippets).
/// Returns the message to show.
pub fn import_alfred(config_dir: &Path, input: &str) -> Result<String> {
    let src = expand(input);
    if input.trim().is_empty() {
        bail!("give the path of an .alfredworkflow or .alfredsnippets file, or a workflow folder");
    }
    if !src.exists() {
        bail!("{} does not exist", src.display());
    }
    let root = extensions::root(config_dir);
    std::fs::create_dir_all(&root)?;
    let tmp = root.join(".importing-alfred");
    let _ = std::fs::remove_dir_all(&tmp);
    // ditto unpacks zips (`-x -k`) and copies folders with their permissions.
    let mut cmd = Command::new("/usr/bin/ditto");
    if src.is_file() {
        cmd.args(["-x", "-k"]);
    }
    let out = cmd.arg(&src).arg(&tmp).output().context("running ditto")?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("could not unpack: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let is_snippets = src
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("alfredsnippets"));
    if is_snippets {
        // Snippet files sit at the top of the zip, or in one folder inside it.
        let mut dirs = vec![tmp.clone()];
        if let Ok(rd) = std::fs::read_dir(&tmp) {
            dirs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        let snippets = dirs
            .iter()
            .map(|d| {
                let info = d.join("info.plist");
                let info = if info.is_file() { plist_json(&info).ok() } else { None };
                alfred::snippets_in(d, info.as_ref())
            })
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        let _ = std::fs::remove_dir_all(&tmp);
        if snippets.is_empty() {
            bail!("no snippets inside — is this an Alfred snippet collection?");
        }
        let items: Vec<(String, String, String)> = snippets.into_iter().map(|s| (s.keyword, s.name, s.text)).collect();
        let (added, skipped) = opencast_core::config::add_snippets(config_dir, &items)?;
        let name = src
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        return Ok(format!(
            "Imported {name} · {added} snippet{}{}",
            if added == 1 { "" } else { "s" },
            if skipped > 0 {
                format!(" ({skipped} skipped: keyword already used)")
            } else {
                String::new()
            }
        ));
    }
    let Some(wf) = alfred::find_workflows(&tmp).into_iter().next() else {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("no info.plist inside — is this an Alfred workflow?");
    };
    let plist = plist_json(&wf.join("info.plist"))?;
    let name = format!("alfred-{}", alfred::slug(&alfred::name_of(&plist)));
    let dest = root.join(&name);
    // Re-import replaces the old copy (keeps Alfred's data folder, which lives elsewhere).
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&wf, &dest).with_context(|| format!("moving into {}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&tmp);
    match alfred::generate(&dest, &plist) {
        Ok(paths) => {
            let n = paths.len();
            let hotkeys = paths
                .iter()
                .filter(|p| p.file_name().is_some_and(|f| f.to_string_lossy().contains("-hotkey-")))
                .count();
            Ok(format!(
                "Imported {} · {n} command{}{}",
                alfred::name_of(&plist),
                if n == 1 { "" } else { "s" },
                if hotkeys > 0 {
                    format!(", {hotkeys} with hotkeys")
                } else {
                    String::new()
                }
            ))
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            Err(e)
        }
    }
}

fn info_markdown(e: &extensions::ExtensionInfo) -> String {
    let mut md = format!("# {}\n\n", e.name);
    if let Some(d) = &e.description {
        md.push_str(&format!("{d}\n\n"));
    }
    md.push_str(&format!("**Commands:** {}\n\n", e.commands));
    if let Some(s) = &e.source {
        md.push_str(&format!("**Source:** {s}\n\n"));
    }
    if e.alfred {
        md.push_str("**Alfred workflow** (imported)\n\n");
    }
    md.push_str(&format!("`{}`", e.path.display()));
    md
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.expect("extension task panicked")
}

fn err(e: impl std::fmt::Display) -> ExecResult {
    ExecResult::Error { message: e.to_string() }
}

fn list(id: &str, items: Vec<ListItem>) -> ExecResult {
    match host::with_engine(|e| e.registry.get(id).cloned()) {
        Some(command) => ExecResult::List {
            command,
            items,
            filter: true,
            view: None,
            columns: None,
            rerun: None,
        },
        None => err("command not found — try Reload Commands"),
    }
}

async fn install(config_dir: PathBuf, source: String) -> ExecResult {
    let r = blocking(move || {
        let ext = extensions::install(&config_dir, &source)?;
        let (alfred_cmds, errors) = generate_all(&ext.path);
        anyhow::Ok((ext, alfred_cmds, errors))
    })
    .await;
    match r {
        Ok((ext, alfred_cmds, errors)) => {
            reload();
            for e in &errors {
                eprintln!("opencast: {e}");
            }
            let n = ext.commands.max(alfred_cmds);
            ExecResult::Toast {
                text: format!(
                    "Installed {} · {n} command{}{}",
                    ext.name,
                    if n == 1 { "" } else { "s" },
                    if errors.is_empty() {
                        ""
                    } else {
                        " (some workflow steps unsupported)"
                    }
                ),
            }
        }
        Err(e) => err(format!("{e:#}")),
    }
}

pub async fn run(op: ExtensionOp, config_dir: PathBuf, args: &[String], ctx: &ExecContext) -> ExecResult {
    let action = ctx.action.as_deref().unwrap_or("list");
    match op {
        ExtensionOp::Install(source) => {
            if source.trim().is_empty() {
                return err("type a GitHub repo (owner/repo) or a git URL");
            }
            install(config_dir, source).await
        }
        ExtensionOp::Update => {
            let includes = host::with_engine(|e| e.config.general.include.clone());
            let results = blocking(move || {
                let mut r = extensions::update_all(&config_dir);
                // Shared config pulled from git (`include = ["git:…"]`).
                for (url, res) in extensions::sync_git_includes(&config_dir, &includes) {
                    r.push((format!("include {url}"), res));
                }
                // Regenerate Alfred commands for extensions that changed.
                for (name, res) in &r {
                    if matches!(res, Ok(true)) {
                        generate_all(&extensions::root(&config_dir).join(name));
                    }
                }
                r
            })
            .await;
            if results.is_empty() {
                return ExecResult::Toast {
                    text: "No extensions installed — try Browse Extensions".into(),
                };
            }
            reload();
            let changed = results.iter().filter(|(_, r)| matches!(r, Ok(true))).count();
            let failed: Vec<String> = results
                .iter()
                .filter_map(|(n, r)| r.as_ref().err().map(|e| format!("{n}: {e}")))
                .collect();
            if failed.is_empty() {
                ExecResult::Toast {
                    text: format!("Updated {changed} of {} extension(s)", results.len()),
                }
            } else {
                ExecResult::Output {
                    title: "Update Extensions".into(),
                    text: format!(
                        "Updated {changed} of {}.\n\nFailed:\n{}",
                        results.len(),
                        failed.join("\n")
                    ),
                }
            }
        }
        ExtensionOp::Manage if action == "run" => {
            let arg = args.first().cloned().unwrap_or_default();
            if let Some(name) = arg.strip_prefix("remove:") {
                let name = name.to_string();
                let r = blocking(move || extensions::remove(&config_dir, &name).map(|_| name)).await;
                return match r {
                    Ok(name) => {
                        reload();
                        ExecResult::Toast {
                            text: format!("Removed {name}"),
                        }
                    }
                    Err(e) => err(format!("{e:#}")),
                };
            }
            if arg == "update" {
                return Box::pin(run(ExtensionOp::Update, config_dir, &[], &ExecContext::default())).await;
            }
            let name = arg.strip_prefix("open:").unwrap_or(&arg);
            let path = extensions::root(&config_dir).join(name);
            match opencast_core::apps::open_path(&path.to_string_lossy()) {
                Ok(()) => ExecResult::Hide,
                Err(e) => err(format!("{e:#}")),
            }
        }
        ExtensionOp::Manage => {
            let exts = blocking(move || extensions::list(&config_dir)).await;
            if exts.is_empty() {
                return ExecResult::Toast {
                    text: "No extensions yet — try Browse Extensions or Install Extension".into(),
                };
            }
            let items = exts
                .iter()
                .map(|e| {
                    let mut item = ListItem::new(e.name.clone());
                    item.subtitle = Some(format!(
                        "{} command{}{}",
                        e.commands,
                        if e.commands == 1 { "" } else { "s" },
                        e.description.as_deref().map(|d| format!(" · {d}")).unwrap_or_default()
                    ));
                    item.icon = Some(if e.alfred { "🎩" } else { "🧩" }.into());
                    item.arg = Some(format!("open:{}", e.name));
                    item.markdown = Some(info_markdown(e));
                    item.actions = vec![
                        ItemAction {
                            title: "Remove Extension".into(),
                            arg: format!("remove:{}", e.name),
                            key: Some("ctrl+x".into()),
                        },
                        ItemAction {
                            title: "Update All Extensions".into(),
                            arg: "update".into(),
                            key: Some("cmd+u".into()),
                        },
                    ];
                    item.copy = e.source.clone();
                    item
                })
                .collect();
            list("builtin:manage-extensions", items)
        }
        ExtensionOp::Browse if action == "run" => {
            let source = args.first().cloned().unwrap_or_default();
            install(config_dir, source).await
        }
        ExtensionOp::Browse => {
            let installed: Vec<String> = {
                let dir = config_dir.clone();
                blocking(move || extensions::list(&dir).into_iter().map(|e| e.name).collect()).await
            };
            match extensions::browse("").await {
                Ok(found) => {
                    let items = found
                        .into_iter()
                        .map(|l| {
                            let mut item = ListItem::new(l.title.clone());
                            let stars = l.stars.map(|s| format!("★ {s} · ")).unwrap_or_default();
                            item.subtitle = Some(format!(
                                "{stars}{} · {}",
                                l.description,
                                extensions::source_label(&l.source)
                            ));
                            let done = extensions::parse_source(&l.source)
                                .map(|s| installed.contains(&s.name))
                                .unwrap_or(false);
                            item.icon = Some(if done { "✅" } else { "🧩" }.into());
                            item.markdown = Some(format!(
                                "# {}\n\n{}\n\n**Install:** `{}`\n\nReturn installs it into `~/.config/opencast/extensions`. \
                                 Extensions run code on your Mac — install what you trust.",
                                l.title, l.description, l.source
                            ));
                            item.copy = Some(l.source.clone());
                            item.arg = Some(l.source);
                            item
                        })
                        .collect();
                    list("builtin:browse-extensions", items)
                }
                Err(e) => err(format!("{e:#}")),
            }
        }
    }
}

pub async fn import(config_dir: PathBuf, input: String) -> ExecResult {
    match blocking(move || import_alfred(&config_dir, &input)).await {
        Ok(text) => {
            reload();
            ExecResult::Toast { text }
        }
        Err(e) => err(format!("{e:#}")),
    }
}

pub fn doctor() -> ExecResult {
    let report = host::with_engine(|e| opencast_core::doctor::run(e, None));
    ExecResult::Output {
        title: "OpenCast Doctor".into(),
        text: report.to_text(),
    }
}
