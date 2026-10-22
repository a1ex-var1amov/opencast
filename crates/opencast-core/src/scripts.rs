//! Script commands: executable files with `@opencast.*` (or `@raycast.*`)
//! header comments, compatible with Raycast's script-command format.
//!
//! `@opencast.mode list` turns a script into an Alfred-style Script Filter:
//! it prints a JSON array of items, the panel lists them, and Return re-runs
//! the script with `OPENCAST_ACTION=run` and the chosen `arg` as `$1`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::commands::Argument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    /// Run, close the launcher, show nothing.
    Silent,
    /// Show the last line of output as a short toast.
    Compact,
    /// Show all output in a scrollable view.
    FullOutput,
    /// Script Filter: stdout is a JSON list of items to pick from.
    List,
    /// Show stdout rendered as Markdown (Alfred's Text View).
    Markdown,
}

impl OutputMode {
    fn parse(s: &str) -> Self {
        match s {
            "silent" => Self::Silent,
            "fullOutput" | "fulloutput" | "full" => Self::FullOutput,
            "list" | "listItems" | "listitems" | "filter" | "grid" => Self::List,
            "markdown" | "detail" | "text" => Self::Markdown,
            // Raycast's "inline" refreshes in the list; compact is the closest.
            _ => Self::Compact,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptMeta {
    pub path: PathBuf,
    pub title: String,
    pub mode: OutputMode,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub package: Option<String>,
    pub arguments: Vec<Argument>,
    /// List mode: `true` = OpenCast filters the items as you type (script runs
    /// once); `false` = the script is re-run with the query as `$1`.
    pub filter: bool,
    /// List mode: how to treat output of the `run` step (default compact).
    pub action_mode: OutputMode,
    /// Extra search words (`@opencast.keywords a, b`), e.g. an Alfred keyword.
    pub keywords: Vec<String>,
    /// `@opencast.mode grid`: list rows shown as a grid.
    pub grid: bool,
    /// `@opencast.accepts text, url, file`: offered by Act on Selection / ⌘K for that input.
    pub accepts: Vec<String>,
    /// `@opencast.hotkey Ctrl+Alt+K`: a global hotkey for this command
    /// (a `[[hotkeys]]` entry for the same command wins).
    pub hotkey: Option<String>,
}

/// An extra action on a list row (⌘K, or its own shortcut). Choosing it runs
/// the script's `run` step with `arg`, like choosing the row itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemAction {
    pub title: String,
    pub arg: String,
    /// `cmd+return`, `cmd+shift+d`… (same format as `[keys]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// One row printed by a list-mode script. Accepts OpenCast's format and
/// Alfred's Script Filter JSON (`icon.path`, `mods`, `text`, `valid`,
/// `quicklookurl`, `variables`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Emoji, SF Symbol name, or an image path.
    #[serde(default)]
    pub icon: Option<String>,
    /// Passed back as `$1` when the item is chosen. Defaults to the title.
    #[serde(default)]
    pub arg: Option<String>,
    /// Text to put in the search field on Tab (Alfred-style). Optional.
    #[serde(default)]
    pub autocomplete: Option<String>,
    /// Markdown for the detail pane beside the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ItemAction>,
    /// File or URL for Quick Look (⌘Y).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quicklook: Option<String>,
    /// Text for Copy (⌘⇧C); defaults to the arg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
    /// `false`: Return autocompletes instead of running (Alfred `valid`).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub valid: bool,
    /// Passed as environment variables to the `run` step (Alfred `variables`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub variables: std::collections::BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl ListItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            icon: None,
            arg: None,
            autocomplete: None,
            markdown: None,
            actions: Vec::new(),
            quicklook: None,
            copy: None,
            valid: true,
            variables: Default::default(),
        }
    }

    pub fn arg(&self) -> &str {
        self.arg.as_deref().unwrap_or(&self.title)
    }

    /// Make relative image icons absolute (scripts print paths relative to their folder).
    pub fn resolve_icon(&mut self, base: &Path) {
        if let Some(icon) = &self.icon {
            if looks_like_image_path(icon) && !icon.starts_with('/') && !icon.starts_with('~') {
                self.icon = Some(base.join(icon).to_string_lossy().into_owned());
            }
        }
    }
}

fn looks_like_image_path(s: &str) -> bool {
    let l = s.to_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".icns", ".pdf", ".svg", ".tiff", ".heic", ".webp",
    ]
    .iter()
    .any(|e| l.ends_with(e))
}

/// A whole Script Filter response.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ListOutput {
    pub items: Vec<ListItem>,
    /// `list` (default) or `grid`.
    pub view: Option<String>,
    /// Grid columns (default 5).
    pub columns: Option<u8>,
    /// Re-run the script after this many seconds while the list is open (Alfred `rerun`).
    pub rerun: Option<f64>,
    /// Variables for every item (Alfred top-level `variables`).
    pub variables: std::collections::BTreeMap<String, String>,
}

fn string_map(v: Option<&serde_json::Value>) -> std::collections::BTreeMap<String, String> {
    v.and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn text_of(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        // Alfred allows an array arg (several files); join with newlines.
        serde_json::Value::Array(a) => Some(
            a.iter()
                .map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

const MOD_KEYS: &[(&str, &str, &str)] = &[
    ("cmd", "cmd+return", "⌘"),
    ("alt", "alt+return", "⌥"),
    ("ctrl", "ctrl+return", "⌃"),
    ("shift", "shift+return", "⇧"),
    ("cmd+shift", "cmd+shift+return", "⌘⇧"),
    ("cmd+alt", "cmd+alt+return", "⌘⌥"),
];

fn item_from(v: &serde_json::Value) -> Result<Option<ListItem>> {
    let o = match v {
        serde_json::Value::String(s) => return Ok(Some(ListItem::new(s.clone()))),
        serde_json::Value::Object(o) => o,
        _ => bail!("bad list item: {v}"),
    };
    let title = text_of(o.get("title")).unwrap_or_default();
    if title.trim().is_empty() {
        return Ok(None);
    }
    let mut item = ListItem::new(title);
    item.subtitle = text_of(o.get("subtitle")).filter(|s| !s.is_empty());
    item.icon = match o.get("icon") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        // Alfred: {"path": "icon.png"} or {"type": "fileicon", "path": "/Applications/Safari.app"}.
        Some(serde_json::Value::Object(i)) => i.get("path").and_then(|p| p.as_str()).map(|p| {
            if i.get("type").and_then(|t| t.as_str()) == Some("fileicon") {
                format!("fileicon:{p}")
            } else {
                p.to_string()
            }
        }),
        _ => None,
    };
    item.arg = text_of(o.get("arg"));
    item.autocomplete = text_of(o.get("autocomplete"));
    item.markdown = text_of(o.get("markdown")).or_else(|| text_of(o.get("detail")));
    item.quicklook = text_of(o.get("quicklook")).or_else(|| text_of(o.get("quicklookurl")));
    item.copy = text_of(o.get("copy")).or_else(|| o.get("text").and_then(|t| text_of(t.get("copy"))));
    if item.markdown.is_none() {
        // Alfred's Large Type text makes a decent detail pane.
        item.markdown = o.get("text").and_then(|t| text_of(t.get("largetype")));
    }
    item.valid = o.get("valid").and_then(|v| v.as_bool()).unwrap_or(true);
    item.variables = string_map(o.get("variables"));
    if let Some(actions) = o.get("actions").and_then(|a| a.as_array()) {
        for a in actions {
            if let Ok(action) = serde_json::from_value::<ItemAction>(a.clone()) {
                item.actions.push(action);
            }
        }
    }
    if let Some(mods) = o.get("mods").and_then(|m| m.as_object()) {
        for (name, key, glyph) in MOD_KEYS {
            let Some(m) = mods.get(*name) else { continue };
            if m.get("valid").and_then(|v| v.as_bool()) == Some(false) {
                continue;
            }
            let arg = text_of(m.get("arg")).unwrap_or_else(|| item.arg().to_string());
            let title = text_of(m.get("subtitle"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("Run with {glyph}"));
            item.actions.push(ItemAction {
                title,
                arg,
                key: Some(key.to_string()),
            });
        }
    }
    Ok(Some(item))
}

/// Parse a list-mode script's stdout: a JSON array, or `{"items": [...]}`
/// with optional `view`, `columns`, `rerun` and `variables`.
pub fn parse_list(stdout: &str) -> Result<ListOutput> {
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(ListOutput::default());
    }
    let v: serde_json::Value = serde_json::from_str(text).context("list script did not print JSON")?;
    let mut out = ListOutput::default();
    let arr = match &v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(o) => {
            out.view = o.get("view").and_then(|x| x.as_str()).map(str::to_string);
            out.columns = o.get("columns").and_then(|x| x.as_u64()).map(|n| n.clamp(2, 10) as u8);
            out.rerun = o.get("rerun").and_then(|x| x.as_f64()).map(|s| s.clamp(0.1, 5.0));
            out.variables = string_map(o.get("variables"));
            o.get("items")
                .and_then(|i| i.as_array())
                .ok_or_else(|| anyhow!("list script JSON needs an array or an `items` array"))?
        }
        _ => bail!("list script JSON needs an array or an `items` array"),
    };
    for item in arr {
        if let Some(i) = item_from(item)? {
            out.items.push(i);
        }
    }
    Ok(out)
}

/// Just the rows of [`parse_list`].
pub fn parse_items(stdout: &str) -> Result<Vec<ListItem>> {
    parse_list(stdout).map(|o| o.items)
}

#[derive(Debug, Deserialize)]
struct RawArgument {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    optional: bool,
}

/// Parse the header of a script. Returns `None` if it has no title (i.e. it
/// is not a script command).
pub fn parse_header(path: &Path, src: &str) -> Option<ScriptMeta> {
    let mut title = None;
    let mut mode = OutputMode::Compact;
    let mut icon = None;
    let mut description = None;
    let mut package = None;
    let mut filter = true;
    let mut action_mode = OutputMode::Compact;
    let mut keywords = Vec::new();
    let mut grid = false;
    let mut accepts = Vec::new();
    let mut hotkey = None;
    let mut args: Vec<(u8, Argument)> = Vec::new();

    for line in src.lines().take(80) {
        let Some(rest) = line
            .find("@opencast.")
            .map(|i| &line[i + "@opencast.".len()..])
            .or_else(|| line.find("@raycast.").map(|i| &line[i + "@raycast.".len()..]))
        else {
            continue;
        };
        let (key, value) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest.trim(), ""),
        };
        match key {
            "title" if !value.is_empty() => title = Some(value.to_string()),
            "mode" => {
                let m = value.split_whitespace().next().unwrap_or("");
                grid = m == "grid";
                mode = OutputMode::parse(m);
            }
            "icon" if !value.is_empty() => icon = Some(value.to_string()),
            "hotkey" if !value.is_empty() => hotkey = Some(value.to_string()),
            "description" if !value.is_empty() => description = Some(value.to_string()),
            "packageName" if !value.is_empty() => package = Some(value.to_string()),
            "accepts" => accepts.extend(
                value
                    .split([',', ' '])
                    .map(|k| k.trim().to_lowercase().trim_end_matches('s').to_string())
                    .filter(|k| matches!(k.as_str(), "text" | "url" | "file")),
            ),
            "keywords" | "keyword" => {
                keywords.extend(value.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))
            }
            "filter" => filter = !matches!(value.split_whitespace().next(), Some("false" | "no" | "0")),
            "action_mode" | "actionMode" => {
                action_mode = OutputMode::parse(value.split_whitespace().next().unwrap_or(""));
                if action_mode == OutputMode::List {
                    action_mode = OutputMode::Compact;
                }
            }
            k if k.starts_with("argument") => {
                let Ok(n) = k["argument".len()..].parse::<u8>() else {
                    continue;
                };
                if let Ok(raw) = serde_json::from_str::<RawArgument>(value) {
                    args.push((
                        n,
                        Argument {
                            placeholder: raw.placeholder.unwrap_or_else(|| format!("Argument {n}")),
                            optional: raw.optional,
                            secure: raw.kind.as_deref() == Some("password"),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    args.sort_by_key(|(n, _)| *n);
    Some(ScriptMeta {
        path: path.to_path_buf(),
        title: title?,
        mode,
        icon,
        description,
        package,
        arguments: args.into_iter().map(|(_, a)| a).take(3).collect(),
        filter,
        action_mode,
        keywords,
        grid,
        accepts,
        hotkey,
    })
}

/// Find script commands in `dir` (one level deep). Returns the scripts and
/// human-readable warnings for files that were skipped.
pub fn discover(dir: &Path) -> (Vec<ScriptMeta>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (found, warnings);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !path.is_file() {
            continue;
        }
        let Ok(bytes) = read_prefix(&path, 64 * 1024) else {
            continue;
        };
        let src = String::from_utf8_lossy(&bytes);
        let Some(meta) = parse_header(&path, &src) else {
            continue;
        };
        if !is_executable(&path) {
            warnings.push(format!(
                "script `{name}` is not executable — run: chmod +x \"{}\"",
                path.display()
            ));
            continue;
        }
        found.push(meta);
    }
    found.sort_by_key(|a| a.title.to_lowercase());
    (found, warnings)
}

fn read_prefix(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(4096);
    std::fs::File::open(path)?.take(max as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl ScriptOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// Last non-empty stdout line, used by compact mode.
    pub fn last_line(&self) -> Option<&str> {
        self.stdout.lines().rev().map(str::trim).find(|l| !l.is_empty())
    }
}

/// How to run a script.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub timeout: Duration,
    /// Run through `$SHELL -lc` so the user's PATH (Homebrew, nvm, pyenv...)
    /// applies. GUI apps on macOS otherwise get a minimal environment.
    pub login_shell: bool,
    /// Extra environment, e.g. `OPENCAST_ACTION=run`.
    pub env: Vec<(String, String)>,
}

impl RunOptions {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            login_shell: false,
            env: Vec::new(),
        }
    }
}

/// Directories prepended to PATH when running scripts, so the usual tools are
/// found even without a login shell.
pub const EXTRA_PATH: &[&str] = &["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"];

pub fn path_with_extras() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = EXTRA_PATH.iter().map(|s| s.to_string()).collect();
    for p in current.split(':').filter(|p| !p.is_empty()) {
        if !parts.iter().any(|x| x == p) {
            parts.push(p.to_string());
        }
    }
    parts.join(":")
}

/// Run a script with arguments, out of process, killed after `timeout`.
pub async fn run(meta: &ScriptMeta, args: &[String], timeout: Duration) -> Result<ScriptOutput> {
    run_with(meta, args, &RunOptions::new(timeout)).await
}

/// Per-script data and cache folders, and Alfred's variable names for them,
/// so Alfred Script Filters and Raycast scripts that expect them run as is.
pub fn standard_env(meta: &ScriptMeta) -> Vec<(String, String)> {
    let stem: String = meta
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script".into())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let home = dirs::home_dir().unwrap_or_default();
    let data = home
        .join("Library/Application Support/OpenCast/script-data")
        .join(&stem);
    let cache = home.join("Library/Caches/OpenCast/script-cache").join(&stem);
    let bundle = format!("opencast.script.{stem}");
    let s = |p: &Path| p.to_string_lossy().into_owned();
    vec![
        ("OPENCAST_DATA".into(), s(&data)),
        ("OPENCAST_CACHE".into(), s(&cache)),
        ("alfred_workflow_data".into(), s(&data)),
        ("alfred_workflow_cache".into(), s(&cache)),
        ("alfred_workflow_bundleid".into(), bundle),
        ("alfred_workflow_name".into(), meta.title.clone()),
        ("alfred_version".into(), "5.5".into()),
        ("alfred_debug".into(), "0".into()),
    ]
}

pub async fn run_with(meta: &ScriptMeta, args: &[String], opts: &RunOptions) -> Result<ScriptOutput> {
    let mut cmd = if opts.login_shell {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/zsh".to_string());
        let mut c = tokio::process::Command::new(shell);
        // `-l` sources the login profile (Homebrew's shellenv lives there);
        // `exec "$0" "$@"` runs the script itself with the real arguments.
        c.arg("-lc").arg("exec \"$0\" \"$@\"").arg(&meta.path).args(args);
        c
    } else {
        let mut c = tokio::process::Command::new(&meta.path);
        c.args(args);
        c
    };
    cmd.kill_on_drop(true).stdin(std::process::Stdio::null());
    cmd.env("PATH", path_with_extras());
    cmd.env("OPENCAST", "1");
    // Imported Alfred workflows set their own (real bundle id) in their wrapper;
    // `opts.env` comes after and wins.
    for (k, v) in standard_env(meta) {
        if std::env::var_os(&k).is_none() {
            cmd.env(k, v);
        }
    }
    for (k, v) in &opts.env {
        cmd.env(k, v);
    }
    if let Some(parent) = meta.path.parent() {
        cmd.current_dir(parent);
    }
    let timeout = opts.timeout;
    let out = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| anyhow!("`{}` timed out after {}s", meta.title, timeout.as_secs()))?
        .with_context(|| format!("running {}", meta.path.display()))?;
    Ok(ScriptOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opencast_header() {
        let src = include_str!("../../../examples/scripts/uuid.sh");
        let m = parse_header(Path::new("/x/uuid.sh"), src).unwrap();
        assert_eq!(m.title, "Generate UUID");
        assert_eq!(m.mode, OutputMode::Compact);
        assert_eq!(m.icon.as_deref(), Some("🆔"));
        assert_eq!(m.arguments.len(), 1);
        assert!(m.arguments[0].optional);
        assert_eq!(m.arguments[0].placeholder, "lowercase? (y/n)");
    }

    #[test]
    fn parses_raycast_header_with_trailing_comment_and_args_in_order() {
        let src = "#!/usr/bin/env python3\n\
            # Required parameters:\n\
            # @raycast.schemaVersion 1\n\
            # @raycast.title Open Ticket\n\
            # @raycast.mode fullOutput   # show everything\n\
            # @raycast.argument2 { \"type\": \"password\", \"placeholder\": \"token\" }\n\
            # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"id\" }\n";
        let m = parse_header(Path::new("t.py"), src).unwrap();
        assert_eq!(m.title, "Open Ticket");
        assert_eq!(m.mode, OutputMode::FullOutput);
        assert_eq!(m.arguments[0].placeholder, "id");
        assert!(m.arguments[1].secure);
    }

    #[test]
    fn parses_list_mode_header() {
        let g = parse_header(Path::new("g.sh"), "# @opencast.title Colors\n# @opencast.mode grid\n").unwrap();
        assert!(g.grid);
        assert_eq!(g.mode, OutputMode::List);
        let md = parse_header(
            Path::new("m.sh"),
            "# @opencast.title Readme\n# @opencast.mode markdown\n",
        )
        .unwrap();
        assert_eq!(md.mode, OutputMode::Markdown);
        let env = standard_env(&md);
        assert!(env
            .iter()
            .any(|(k, v)| k == "alfred_workflow_bundleid" && v == "opencast.script.m"));
        let src = "#!/bin/sh\n# @opencast.title Branches\n# @opencast.mode list\n# @opencast.filter false\n# @opencast.action_mode silent\n";
        let m = parse_header(Path::new("b.sh"), src).unwrap();
        assert_eq!(m.mode, OutputMode::List);
        assert!(!m.filter);
        assert_eq!(m.action_mode, OutputMode::Silent);
        let d = parse_header(Path::new("c.sh"), "# @opencast.title X\n# @opencast.mode list\n").unwrap();
        assert!(d.filter);
        assert_eq!(d.action_mode, OutputMode::Compact);
    }

    #[test]
    fn parses_list_items() {
        let items =
            parse_items(r#"[{"title":"main","subtitle":"checkout","arg":"main"},"dev",{"title":"","arg":"x"}]"#)
                .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].arg(), "main");
        assert_eq!(items[1].title, "dev");
        assert_eq!(items[1].arg(), "dev");
        let wrapped = parse_items(r#"{"items":[{"title":"a"}]}"#).unwrap();
        assert_eq!(wrapped[0].title, "a");
        assert!(parse_items("").unwrap().is_empty());
        assert!(parse_items("not json").is_err());
        assert!(parse_items("42").is_err());
    }

    #[test]
    fn parses_alfred_script_filter_json() {
        let out = parse_list(
            r#"{"rerun": 1, "variables": {"mode": "x"}, "items": [
                {"uid": "a", "title": "Safari", "subtitle": "browser", "arg": ["/a", "/b"],
                 "icon": {"type": "fileicon", "path": "/Applications/Safari.app"},
                 "valid": false, "autocomplete": "saf",
                 "quicklookurl": "https://apple.com",
                 "text": {"copy": "copied", "largetype": "BIG"},
                 "mods": {"cmd": {"arg": "cmd-arg", "subtitle": "Reveal"}, "alt": {"valid": false}},
                 "variables": {"k": "v", "n": 2}},
                {"title": "Icon", "icon": {"path": "icons/x.png"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(out.rerun, Some(1.0));
        assert_eq!(out.variables["mode"], "x");
        let a = &out.items[0];
        assert_eq!(a.arg(), "/a\n/b");
        assert_eq!(a.icon.as_deref(), Some("fileicon:/Applications/Safari.app"));
        assert!(!a.valid);
        assert_eq!(a.quicklook.as_deref(), Some("https://apple.com"));
        assert_eq!(a.copy.as_deref(), Some("copied"));
        assert_eq!(a.markdown.as_deref(), Some("BIG"));
        assert_eq!(a.actions.len(), 1);
        assert_eq!(a.actions[0].title, "Reveal");
        assert_eq!(a.actions[0].key.as_deref(), Some("cmd+return"));
        assert_eq!(a.variables["n"], "2");
        let mut b = out.items[1].clone();
        b.resolve_icon(Path::new("/wf"));
        assert_eq!(b.icon.as_deref(), Some("/wf/icons/x.png"));
    }

    #[test]
    fn parses_grid_and_actions() {
        let out = parse_list(
            r##"{"view": "grid", "columns": 4, "items": [
                {"title": "Red", "icon": "🟥", "markdown": "# Red", "actions": [{"title": "Copy Hex", "arg": "#f00", "key": "cmd+shift+c"}]}
            ]}"##,
        )
        .unwrap();
        assert_eq!(out.view.as_deref(), Some("grid"));
        assert_eq!(out.columns, Some(4));
        assert_eq!(out.items[0].markdown.as_deref(), Some("# Red"));
        assert_eq!(out.items[0].actions[0].arg, "#f00");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_through_login_shell_with_env() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("env.sh");
        std::fs::write(
            &p,
            "#!/bin/sh\n# @opencast.title Env\necho \"$OPENCAST_ACTION:$1:$OPENCAST\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let meta = parse_header(&p, &std::fs::read_to_string(&p).unwrap()).unwrap();
        let mut opts = RunOptions::new(Duration::from_secs(5));
        opts.login_shell = true;
        opts.env.push(("OPENCAST_ACTION".into(), "run".into()));
        std::env::set_var("SHELL", "/bin/sh");
        let out = run_with(&meta, &["main".into()], &opts).await.unwrap();
        assert!(out.success(), "{out:?}");
        assert_eq!(out.last_line(), Some("run:main:1"));
    }

    #[test]
    fn no_title_means_not_a_command() {
        assert!(parse_header(Path::new("x"), "#!/bin/sh\necho hi\n").is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn discover_and_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("hello.sh");
        std::fs::write(
            &ok,
            "#!/bin/sh\n# @opencast.title Hello\necho \"hi $1\"\necho oops >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&ok, std::fs::Permissions::from_mode(0o755)).unwrap();
        let noexec = dir.path().join("noexec.sh");
        std::fs::write(&noexec, "#!/bin/sh\n# @opencast.title Nope\n").unwrap();

        let (found, warnings) = discover(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(warnings.len(), 1);

        let out = run(&found[0], &["there".into()], Duration::from_secs(5)).await.unwrap();
        assert!(out.success());
        assert_eq!(out.last_line(), Some("hi there"));
        assert_eq!(out.stderr.trim(), "oops");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_times_out() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("slow.sh");
        std::fs::write(&p, "#!/bin/sh\n# @opencast.title Slow\nsleep 5\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let meta = parse_header(&p, &std::fs::read_to_string(&p).unwrap()).unwrap();
        let err = run(&meta, &[], Duration::from_millis(200)).await.unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
