//! `config.toml` model, loading, validation and first-run setup.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Written to `<config dir>/config.toml` on first run.
/// Largest config.toml OpenCast will read (5 MB).
pub const MAX_CONFIG_BYTES: u64 = 5_000_000;

pub const DEFAULT_CONFIG: &str = include_str!("../../../examples/config.toml");

/// Example scripts written to `<config dir>/scripts/` on first run.
pub const EXAMPLE_SCRIPTS: &[(&str, &str)] = &[
    (
        "toggle-dark-mode.sh",
        include_str!("../../../examples/scripts/toggle-dark-mode.sh"),
    ),
    ("ip-address.sh", include_str!("../../../examples/scripts/ip-address.sh")),
    ("uuid.sh", include_str!("../../../examples/scripts/uuid.sh")),
    (
        "capslock-launcher-key.sh",
        include_str!("../../../examples/scripts/capslock-launcher-key.sh"),
    ),
    (
        "recent-branches.sh",
        include_str!("../../../examples/scripts/recent-branches.sh"),
    ),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub keys: Keys,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub ai_commands: Vec<AiCommandConfig>,
    #[serde(default)]
    pub quicklinks: Vec<QuicklinkConfig>,
    #[serde(default)]
    pub snippets: Vec<SnippetConfig>,
    #[serde(default)]
    pub hotkeys: Vec<HotkeyBinding>,
    /// Per-command tweaks: aliases, boost, icon, title, hidden.
    #[serde(default)]
    pub commands: Vec<CommandOverride>,
    /// Model Context Protocol servers: their tools become launcher commands
    /// and AI chat tools.
    #[serde(default)]
    pub mcp_servers: Vec<crate::mcp::McpServerConfig>,
    /// Focus sessions: apps hidden while one runs.
    #[serde(default)]
    pub focus: FocusConfig,
    /// Saved chat setups shown as commands ("Ask Code Reviewer").
    #[serde(default)]
    pub ai_presets: Vec<AiPreset>,
    #[serde(default)]
    pub window_commands: Vec<WindowCommandConfig>,
    /// `[[pipelines]]`: text through several commands in a row.
    #[serde(default)]
    pub pipelines: Vec<crate::pipeline::PipelineConfig>,
    /// Problems with `include` files (missing, invalid); shown as warnings.
    #[serde(skip)]
    pub include_warnings: Vec<String>,
    /// Keys and section names in `config.toml` that nothing reads — typos serde
    /// would otherwise swallow. Shown as warnings.
    #[serde(skip)]
    pub parse_warnings: Vec<String>,
}

/// Section and key names the parser accepts, taken from the structs by
/// serializing a default `Config` — so this list cannot drift out of date.
/// serde ignores unknown fields, so without this a typo in `config.toml` is
/// silent: the setting just does nothing and nothing says so.
///
/// Checked: section names (`[general]`, `[[ai_commands]]`, …) and the keys of
/// the single-instance tables (`[general]`, `[keys]`, `[focus]`). The keys of
/// `[[providers]]`-style entries are left to serde, which reports the ones it
/// cares about as missing fields.
pub fn unknown_keys(src: &str) -> Vec<String> {
    let Ok(file) = src.parse::<toml::Value>() else {
        return Vec::new(); // a file that does not parse is reported as an error
    };
    let Ok(shape) = serde_json::to_value(Config::default()) else {
        return Vec::new();
    };
    let Some(table) = file.as_table() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, val) in table {
        let Some(spec) = shape.get(name) else {
            let what = if section_like(val) { "section" } else { "key" };
            out.push(format!(
                "unknown {what} `{name}` is not read by anything{}",
                did_you_mean(name, shape.as_object().into_iter().flat_map(|o| o.keys()))
            ));
            continue;
        };
        let (Some(rows), Some(want)) = (val.as_table(), spec.as_object()) else {
            continue;
        };
        for (k, v) in rows {
            if section_like(v) {
                continue;
            }
            if !want.contains_key(k.as_str()) {
                out.push(format!(
                    "unknown key `[{name}] {k}` is not read by anything{}",
                    did_you_mean(k, want.keys())
                ));
            }
        }
    }
    out.sort();
    out
}

/// `[table]` or `[[array.of.tables]]` as parsed from TOML.
fn section_like(v: &toml::Value) -> bool {
    v.is_table() || v.as_array().is_some_and(|a| a.first().is_some_and(|t| t.is_table()))
}

/// “ (did you mean `x`?)” when a known name is within two edits.
fn did_you_mean<'a, I: Iterator<Item = &'a String>>(needle: &str, names: I) -> String {
    let mut best: Option<(usize, &str)> = None;
    for n in names {
        let d = edit_distance(needle, n);
        if d <= 2 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, n.as_str()));
        }
    }
    match best {
        Some((_, n)) => format!(" (did you mean `{n}`?)"),
        None => String::new(),
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=bc.len()).collect();
    let mut cur = vec![0usize; bc.len() + 1];
    for i in 1..=ac.len() {
        cur[0] = i;
        for j in 1..=bc.len() {
            let cost = usize::from(ac[i - 1] != bc[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[bc.len()]
}

/// `[[ai_presets]]`: a chat with its own provider, model, prompt and tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiPreset {
    pub id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub icon: Option<String>,
    /// MCP tools the chat may use: globs over `server__tool` (`github__*`).
    /// Absent = every enabled tool; empty list = none.
    pub tools: Option<Vec<String>>,
    /// Prepend `profile.md` (default: the `ai_profile` setting).
    pub profile: Option<bool>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// `*` and `prefix*` / `*suffix` / `a*b` globs (case-insensitive).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.to_lowercase();
    let t = text.to_lowercase();
    match p.split_once('*') {
        None => p == t,
        Some((head, tail)) => {
            if !t.starts_with(head) {
                return false;
            }
            let rest = &t[head.len()..];
            if tail.is_empty() {
                return true;
            }
            if !tail.contains('*') {
                return rest.ends_with(tail);
            }
            (0..=rest.len())
                .filter(|i| rest.is_char_boundary(*i))
                .any(|i| glob_match(tail, &rest[i..]))
        }
    }
}

/// `[focus]`: Start Focus hides these apps (and keeps hiding them when they
/// come back) until the session ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusConfig {
    #[serde(default = "d_focus_apps")]
    pub apps: Vec<String>,
    #[serde(default = "d_focus_minutes")]
    pub minutes: u32,
    /// Sites closed when they come up in the front browser tab during a session
    /// (`reddit.com` also matches `old.reddit.com`).
    #[serde(default = "d_focus_domains")]
    pub domains: Vec<String>,
    /// Show the countdown in the menu bar while a session runs.
    #[serde(default = "default_true")]
    pub menu_bar: bool,
}

fn d_focus_domains() -> Vec<String> {
    [
        "x.com",
        "twitter.com",
        "reddit.com",
        "facebook.com",
        "instagram.com",
        "tiktok.com",
        "news.ycombinator.com",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
#[test]
fn focus_domains_match_subdomains() {
    let d = vec!["reddit.com".to_string(), "*.example.org".into()];
    assert!(domain_blocked("www.reddit.com", &d));
    assert!(domain_blocked("old.reddit.com", &d));
    assert!(domain_blocked("a.example.org", &d));
    assert!(!domain_blocked("notreddit.com", &d));
    assert!(!domain_blocked("reddit.com.evil.net", &d));
}

/// Does `host` fall under one of `domains` (exact or a subdomain)?
pub fn domain_blocked(host: &str, domains: &[String]) -> bool {
    let host = host.trim().trim_end_matches('.').to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    domains.iter().any(|d| {
        let d = d
            .trim()
            .trim_start_matches("*.")
            .trim_start_matches("www.")
            .to_lowercase();
        !d.is_empty() && (host == d || host.ends_with(&format!(".{d}")))
    })
}

fn d_focus_apps() -> Vec<String> {
    [
        "Slack", "Messages", "Mail", "Discord", "Telegram", "WhatsApp", "Twitter", "X",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn d_focus_minutes() -> u32 {
    25
}

impl Default for FocusConfig {
    fn default() -> Self {
        Self {
            apps: d_focus_apps(),
            minutes: d_focus_minutes(),
            domains: d_focus_domains(),
            menu_bar: true,
        }
    }
}

/// Text expansion: run the command to copy (and paste when possible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetConfig {
    pub keyword: String,
    pub name: Option<String>,
    /// Placeholders: `{date}`, `{time}`, `{datetime}`, `{clipboard}`, `{cursor}`.
    pub text: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Expand while typing in other apps. Default: on when the keyword starts
    /// with a symbol (`;sig`), off for plain words.
    pub auto_expand: Option<bool>,
}

impl SnippetConfig {
    pub fn expands_while_typing(&self) -> bool {
        let k = self.keyword.trim();
        self.auto_expand
            .unwrap_or_else(|| k.chars().next().is_some_and(|c| !c.is_alphanumeric()) && k.chars().count() >= 2)
    }
}

/// A global hotkey bound to any command: an app, a window action, an AI
/// command, a script... `command` is a command id or a (case-insensitive)
/// command title, e.g. "Slack", "Left Half", "ai:fix-grammar".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyBinding {
    pub key: String,
    pub command: String,
}

/// `[[commands]]`: tweak any command, including apps and builtins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOverride {
    /// Command id or case-insensitive title.
    #[serde(rename = "match")]
    pub match_: String,
    /// Short alias; an exact match ranks first.
    pub alias: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Emoji, or an SF Symbol name (the panel resolves it).
    pub icon: Option<String>,
    pub title: Option<String>,
    /// Added to the search score (negative demotes).
    #[serde(default)]
    pub boost: i64,
    #[serde(default)]
    pub hidden: bool,
    /// Global hotkey for this command (`Hyper+V`, `Ctrl+Alt+C`).
    #[serde(default)]
    pub hotkey: Option<String>,
}

impl CommandOverride {
    pub fn all_aliases(&self) -> Vec<String> {
        self.alias
            .iter()
            .chain(self.aliases.iter())
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty())
            .collect()
    }
}

/// In-panel keys. Specs are `Mod+Key`: mods `Cmd`, `Shift`, `Alt`/`Option`,
/// `Ctrl`; keys are single characters or `Tab`, `Return`, `Escape`, `Space`,
/// `Up`, `Down`, `Left`, `Right`, `Backspace`, `Delete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keys {
    #[serde(default = "d_ask_ai")]
    pub ask_ai: String,
    #[serde(default = "d_click_text")]
    pub click_text: String,
    #[serde(default = "d_actions")]
    pub actions: String,
    #[serde(default = "d_pin")]
    pub pin: String,
    #[serde(default = "d_paste_answer")]
    pub paste_answer: String,
    #[serde(default = "d_copy_answer")]
    pub copy_answer: String,
    #[serde(default = "d_new_chat")]
    pub new_chat: String,
    #[serde(default = "d_next")]
    pub next: String,
    #[serde(default = "d_previous")]
    pub previous: String,
    #[serde(default = "d_dictate")]
    pub dictate: String,
    #[serde(default = "d_quick_look")]
    pub quick_look: String,
    /// Show / hide the preview pane.
    #[serde(default = "d_preview")]
    pub preview: String,
    /// Show the search text (or selected row) in Large Type.
    #[serde(default = "d_large_type")]
    pub large_type: String,
}

fn d_ask_ai() -> String {
    "Tab".into()
}
fn d_click_text() -> String {
    "`".into()
}
fn d_actions() -> String {
    "Cmd+K".into()
}
fn d_pin() -> String {
    "Cmd+Shift+P".into()
}
fn d_paste_answer() -> String {
    "Cmd+Return".into()
}
fn d_copy_answer() -> String {
    "Cmd+Shift+C".into()
}
fn d_new_chat() -> String {
    "Cmd+N".into()
}
fn d_next() -> String {
    "Ctrl+N".into()
}
fn d_previous() -> String {
    "Ctrl+P".into()
}
fn d_dictate() -> String {
    "Cmd+D".into()
}
fn d_quick_look() -> String {
    "Cmd+Y".into()
}
fn d_preview() -> String {
    "Cmd+I".into()
}
fn d_large_type() -> String {
    "Cmd+L".into()
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            ask_ai: d_ask_ai(),
            click_text: d_click_text(),
            actions: d_actions(),
            pin: d_pin(),
            paste_answer: d_paste_answer(),
            copy_answer: d_copy_answer(),
            new_chat: d_new_chat(),
            next: d_next(),
            previous: d_previous(),
            dictate: d_dictate(),
            quick_look: d_quick_look(),
            preview: d_preview(),
            large_type: d_large_type(),
        }
    }
}

impl Keys {
    /// `(name, spec)` for every binding.
    pub fn entries(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("ask_ai", &self.ask_ai),
            ("click_text", &self.click_text),
            ("actions", &self.actions),
            ("pin", &self.pin),
            ("paste_answer", &self.paste_answer),
            ("copy_answer", &self.copy_answer),
            ("new_chat", &self.new_chat),
            ("next", &self.next),
            ("previous", &self.previous),
            ("dictate", &self.dictate),
            ("quick_look", &self.quick_look),
            ("preview", &self.preview),
            ("large_type", &self.large_type),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    pub default_provider: Option<String>,
    pub scripts_dir: Option<PathBuf>,
    #[serde(default = "default_timeout")]
    pub script_timeout_secs: u64,
    /// Run scripts through `$SHELL -lc` so Homebrew / nvm / pyenv are on PATH.
    #[serde(default = "default_true")]
    pub script_login_shell: bool,
    /// Caps Lock becomes ⌃⌥⌘⇧ while held and opens the launcher when tapped.
    #[serde(default)]
    pub hyperkey: bool,
    /// `system`, `light`, or `dark`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Optional theme TOML (colors, radius, sizes); see `ThemeFile`.
    pub theme_file: Option<PathBuf>,
    /// `island` (top bar) or `spotlight` (centered).
    #[serde(default = "default_presentation")]
    pub presentation: String,
    /// Rows appended when a query has fewer than `fallback_threshold` strong
    /// hits. Entries are command ids/titles, or the tokens `quicklinks`
    /// (every `{query}` quicklink) and `files` (Spotlight expand).
    #[serde(default = "default_fallbacks")]
    pub fallbacks: Vec<String>,
    #[serde(default = "default_fallback_threshold")]
    pub fallback_threshold: usize,
    /// List the user's Shortcuts.app shortcuts as commands.
    #[serde(default = "default_true")]
    pub shortcuts: bool,
    /// Expand snippet keywords as you type in any app (needs Accessibility).
    /// Only keywords that start with a symbol (`;sig`, `::date`) expand, so
    /// ordinary words never get replaced.
    #[serde(default = "default_true")]
    pub snippet_expansion: bool,
    /// Ask GitHub for a newer release once a week (off by default: no traffic
    /// you did not configure).
    #[serde(default)]
    pub check_updates: bool,
    /// Currency conversion in the search field: fetch exchange rates once a
    /// day (open.er-api.com). Off: no traffic you did not configure.
    #[serde(default)]
    pub currency: bool,
    /// Show the next calendar event at the top of the empty launcher.
    #[serde(default = "default_true")]
    pub calendar: bool,
    /// Where `Check for Updates` looks: `owner/repo` on GitHub (empty = not set up).
    #[serde(default = "default_update_repo")]
    pub update_repo: String,
    /// Ask before Restart, Shut Down, Log Out and Quit All Apps.
    #[serde(default = "default_true")]
    pub confirm_destructive: bool,
    /// A lone Shift tap opens Quick Look on the selected row.
    #[serde(default)]
    pub shift_quick_look: bool,
    /// Skin tone for emoji that have one: `light`, `medium-light`, `medium`,
    /// `medium-dark`, `dark`, or empty for the default yellow.
    #[serde(default)]
    pub emoji_skin_tone: String,
    /// Folder that "Sync Config" links the shared files into (Dropbox, a git
    /// checkout…). Empty: iCloud Drive.
    #[serde(default)]
    pub sync_dir: Option<String>,
    /// Extra config files merged in (snippets, quicklinks, commands, AI
    /// commands and presets, hotkeys); this file wins on id clashes.
    /// `git:<url>` entries are cloned by "Update Extensions".
    #[serde(default)]
    pub include: Vec<String>,
    /// Prepend `profile.md` (in the config folder) to every AI chat's system prompt.
    #[serde(default = "default_true")]
    pub ai_profile: bool,
    /// Translate: the language text is translated into…
    #[serde(default = "default_translate_to")]
    pub translate_to: String,
    /// …and the language used when the text is already in `translate_to`.
    #[serde(default)]
    pub translate_alt: Option<String>,
    /// Global hotkey for Act on Selection (Universal Actions). Empty = none.
    #[serde(default = "default_ua_hotkey")]
    pub universal_actions_hotkey: String,
    /// Points between tiled windows.
    #[serde(default)]
    pub window_gap: f64,
    /// Points between windows and the screen edges.
    #[serde(default)]
    pub screen_margin: f64,
    /// Repeating a half command: `sizes` (½ → ⅔ → ⅓), `displays` (on to the
    /// next screen), or `off`.
    #[serde(default = "default_window_cycle")]
    pub window_cycle: String,
    /// `auto`: on a portrait (rotated) screen, thirds, two thirds, center half
    /// and sixths split top-to-bottom. `off`: always side by side.
    #[serde(default = "default_window_portrait")]
    pub window_portrait: String,
    /// Almost Maximize size, as a fraction of the screen.
    #[serde(default = "default_almost")]
    pub almost_maximize: f64,
    /// Show a small HUD when a hotkey moves a window.
    #[serde(default = "default_true")]
    pub window_hud: bool,
    /// Drag a window to a screen edge or corner to snap it (off when macOS
    /// tiling is on).
    #[serde(default)]
    pub drag_snap: bool,
}

fn default_translate_to() -> String {
    "English".into()
}

fn default_window_portrait() -> String {
    "auto".into()
}

fn default_window_cycle() -> String {
    "sizes".into()
}

fn default_almost() -> f64 {
    0.9
}

/// A size in `[[window_commands]]`: `0.6` (fraction) or `"800px"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WindowDim {
    Fraction(f64),
    Points(String),
}

/// `[[window_commands]]`: your own window position, shown as a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowCommandConfig {
    pub id: String,
    pub title: String,
    pub x: WindowDim,
    pub y: WindowDim,
    pub w: WindowDim,
    pub h: WindowDim,
    /// `current` (default), `main`, `next`, `previous`, or a display number.
    pub display: Option<String>,
    pub hotkey: Option<String>,
    pub icon: Option<String>,
}

fn default_ua_hotkey() -> String {
    "Hyper+Space".into()
}

impl Default for General {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            default_provider: None,
            scripts_dir: None,
            script_timeout_secs: default_timeout(),
            script_login_shell: true,
            hyperkey: false,
            theme: default_theme(),
            theme_file: None,
            presentation: default_presentation(),
            fallbacks: default_fallbacks(),
            fallback_threshold: default_fallback_threshold(),
            shortcuts: true,
            snippet_expansion: true,
            check_updates: false,
            currency: false,
            calendar: true,
            update_repo: default_update_repo(),
            confirm_destructive: true,
            shift_quick_look: false,
            emoji_skin_tone: String::new(),
            sync_dir: None,
            include: Vec::new(),
            ai_profile: true,
            translate_to: default_translate_to(),
            translate_alt: None,
            universal_actions_hotkey: default_ua_hotkey(),
            window_gap: 0.0,
            screen_margin: 0.0,
            window_cycle: default_window_cycle(),
            window_portrait: default_window_portrait(),
            almost_maximize: default_almost(),
            window_hud: true,
            drag_snap: false,
        }
    }
}

fn default_hotkey() -> String {
    "Alt+Space".into()
}

fn default_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".into()
}

fn default_presentation() -> String {
    "island".into()
}

fn default_fallbacks() -> Vec<String> {
    vec!["quicklinks".into(), "files".into()]
}

fn default_fallback_threshold() -> usize {
    3
}

fn default_update_repo() -> String {
    String::new()
}

/// `theme_file` contents. Every key optional; the panel keeps its default
/// for anything missing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThemeFile {
    /// `#rrggbb`; default follows the Mac's accent colour.
    pub accent: Option<String>,
    pub panel_light: Option<String>,
    pub panel_dark: Option<String>,
    pub text_light: Option<String>,
    pub text_dark: Option<String>,
    pub radius: Option<f64>,
    pub row_height: Option<f64>,
    pub width: Option<f64>,
    pub font_size: Option<f64>,
    pub title_font_size: Option<f64>,
    /// `false` = opaque panel (same as Reduce Transparency).
    pub glass: Option<bool>,
    /// Font family name, e.g. "JetBrains Mono".
    pub font: Option<String>,
}

impl ThemeFile {
    pub fn parse(src: &str) -> Result<Self> {
        let t: ThemeFile = toml::from_str(src).context("parsing theme file")?;
        for (k, v) in [
            ("accent", &t.accent),
            ("panel_light", &t.panel_light),
            ("panel_dark", &t.panel_dark),
            ("text_light", &t.text_light),
            ("text_dark", &t.text_dark),
        ] {
            if let Some(v) = v {
                anyhow::ensure!(is_hex_color(v), "theme `{k}` must be #rrggbb, got `{v}`");
            }
        }
        Ok(t)
    }
}

fn is_hex_color(s: &str) -> bool {
    let h = s.strip_prefix('#').unwrap_or("");
    (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// Replace or insert `key = "value"` under `[general]` in `config.toml`,
/// keeping comments and formatting (toml_edit).
pub fn set_general_key(dir: &Path, key: &str, value: &str) -> Result<()> {
    anyhow::ensure!(
        key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "bad config key"
    );
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let updated = set_general_key_in(&src, key, value)?;
    // One key changed: the file can only grow by a line. Never write anything else.
    anyhow::ensure!(
        updated.len() <= src.len() + key.len() + 2 * value.len() + 64,
        "refusing to rewrite config.toml (unexpected growth)"
    );
    std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn set_general_key_in(src: &str, key: &str, value: &str) -> Result<String> {
    use toml_edit::{Document, Item, Table};
    let mut doc: Document = src.parse().context("config.toml is not valid TOML")?;
    if !doc.contains_key("general") {
        let mut t = Table::new();
        t.set_implicit(false);
        doc.insert("general", Item::Table(t));
    }
    let general = doc["general"]
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[general] is not a table"))?;
    general[key] = Item::Value(value_for(value));
    Ok(doc.to_string())
}

/// Set (or with an empty `key`, clear) the global hotkey of command `id`:
/// `hotkey` on its `[[commands]]` entry, created if needed. Any `[[hotkeys]]`
/// entry for the same command is removed so there is one binding. Refuses a
/// key another command already uses (returns that command).
pub fn set_command_hotkey(dir: &Path, id: &str, key: &str) -> Result<()> {
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let cfg = Config::parse(&src)?;
    let key = key.trim();
    if !key.is_empty() {
        let want = crate::keymap::normalize_hotkey(key).ok_or_else(|| anyhow::anyhow!("`{key}` is not a hotkey"))?;
        anyhow::ensure!(
            crate::keymap::normalize_hotkey(&cfg.general.hotkey).as_deref() != Some(want.as_str()),
            "{key} opens OpenCast itself"
        );
        for h in cfg.all_hotkeys() {
            if crate::keymap::normalize_hotkey(&h.key).as_deref() == Some(want.as_str()) && h.command != id {
                anyhow::bail!("{key} is already bound to `{}`", h.command);
            }
        }
    }
    let updated = set_command_hotkey_in(&src, id, key)?;
    Config::parse(&updated).context("the new hotkey makes config.toml invalid")?;
    std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))
}

fn set_command_hotkey_in(src: &str, id: &str, key: &str) -> Result<String> {
    use toml_edit::{value, ArrayOfTables, Document, Item, Table};
    let mut doc: Document = src.parse().context("config.toml is not valid TOML")?;
    // Drop [[hotkeys]] entries for this command.
    if let Some(hk) = doc.get_mut("hotkeys").and_then(Item::as_array_of_tables_mut) {
        hk.retain(|t| t.get("command").and_then(|v| v.as_str()) != Some(id));
    }
    if !doc.contains_key("commands") {
        doc.insert("commands", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let commands = doc["commands"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("`commands` is not an array of tables"))?;
    let existing = commands
        .iter_mut()
        .find(|t| t.get("match").and_then(|v| v.as_str()) == Some(id));
    match (existing, key.is_empty()) {
        (Some(t), true) => {
            t.remove("hotkey");
        }
        (Some(t), false) => {
            t["hotkey"] = value(key);
        }
        (None, true) => {}
        (None, false) => {
            let mut t = Table::new();
            t["match"] = value(id);
            t["hotkey"] = value(key);
            commands.push(t);
        }
    }
    Ok(doc.to_string())
}

/// Append a `[[snippets]]` entry (Save as Snippet). Refuses a keyword in use.
pub fn add_snippet(dir: &Path, keyword: &str, text: &str, name: Option<&str>) -> Result<()> {
    let keyword = keyword.trim();
    anyhow::ensure!(!keyword.is_empty(), "a snippet needs a keyword");
    anyhow::ensure!(!text.is_empty(), "the snippet is empty");
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).unwrap_or_default();
    let cfg = Config::parse(&src)?;
    anyhow::ensure!(
        !cfg.snippets.iter().any(|s| s.keyword == keyword),
        "a snippet with keyword `{keyword}` already exists"
    );
    use toml_edit::{value, ArrayOfTables, Document, Item, Table};
    let mut doc: Document = src.parse().context("config.toml is not valid TOML")?;
    if !doc.contains_key("snippets") {
        doc.insert("snippets", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let mut t = Table::new();
    t["keyword"] = value(keyword);
    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
        t["name"] = value(n.trim());
    }
    t["text"] = value(text);
    doc["snippets"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("`snippets` is not an array of tables"))?
        .push(t);
    let updated = doc.to_string();
    Config::parse(&updated).context("the snippet makes config.toml invalid")?;
    std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))
}

/// Add many `[[snippets]]` in one write (an imported collection). Keywords
/// that already exist are skipped. Returns (added, skipped).
pub fn add_snippets(dir: &Path, items: &[(String, String, String)]) -> Result<(usize, usize)> {
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).unwrap_or_default();
    let cfg = Config::parse(&src)?;
    use toml_edit::{value, ArrayOfTables, Document, Item, Table};
    let mut doc: Document = src.parse().context("config.toml is not valid TOML")?;
    if !doc.contains_key("snippets") {
        doc.insert("snippets", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let arr = doc["snippets"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("`snippets` is not an array of tables"))?;
    let (mut added, mut skipped) = (0, 0);
    let mut seen: std::collections::HashSet<String> = cfg.snippets.iter().map(|s| s.keyword.clone()).collect();
    for (keyword, name, text) in items {
        let keyword = keyword.trim();
        if keyword.is_empty() || text.is_empty() || !seen.insert(keyword.to_string()) {
            skipped += 1;
            continue;
        }
        let mut t = Table::new();
        t["keyword"] = value(keyword);
        if !name.trim().is_empty() {
            t["name"] = value(name.trim());
        }
        t["text"] = value(text.as_str());
        arr.push(t);
        added += 1;
    }
    if added > 0 {
        let updated = doc.to_string();
        Config::parse(&updated).context("the snippets make config.toml invalid")?;
        std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok((added, skipped))
}

/// Append a `[[providers]]` entry (comments elsewhere are kept). Replaces an
/// existing provider with the same id.
pub fn add_provider(dir: &Path, p: &ProviderConfig) -> Result<()> {
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = add_provider_in(&src, p)?;
    // Validate before writing so a bad entry never breaks the config.
    Config::parse(&updated).context("the new provider makes config.toml invalid")?;
    std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))
}

fn add_provider_in(src: &str, p: &ProviderConfig) -> Result<String> {
    use toml_edit::{value, Array, ArrayOfTables, Document, Item, Table};
    let without = remove_provider_in(src, &p.id)?;
    let mut doc: Document = without.parse().context("config.toml is not valid TOML")?;
    let mut t = Table::new();
    t["id"] = value(p.id.clone());
    t["kind"] = value(match p.kind {
        ProviderKind::Openai => "openai",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Gemini => "gemini",
    });
    if let Some(u) = &p.base_url {
        t["base_url"] = value(u.clone());
    }
    if let Some(e) = &p.api_key_env {
        t["api_key_env"] = value(e.clone());
    }
    if let Some(c) = &p.api_key_cmd {
        t["api_key_cmd"] = value(c.clone());
    }
    t["model"] = value(p.model.clone());
    if !p.models.is_empty() {
        let mut a = Array::new();
        for m in &p.models {
            a.push(m.as_str());
        }
        t["models"] = value(a);
    }
    if !doc.contains_key("providers") {
        doc.insert("providers", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc["providers"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("`providers` is not an array of tables"))?
        .push(t);
    Ok(doc.to_string())
}

/// Remove the `[[providers]]` entry with `id` (no-op when absent).
pub fn remove_provider(dir: &Path, id: &str) -> Result<bool> {
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let updated = remove_provider_in(&src, id)?;
    if updated == src {
        return Ok(false);
    }
    // Drop default_provider if it pointed at the removed one.
    let mut cfg_src = updated.clone();
    if let Ok(cfg) = toml::from_str::<Config>(&updated) {
        if cfg.general.default_provider.as_deref() == Some(id) {
            let mut doc: toml_edit::Document = updated.parse()?;
            if let Some(g) = doc.get_mut("general").and_then(|g| g.as_table_mut()) {
                g.remove("default_provider");
            }
            cfg_src = doc.to_string();
        }
    }
    std::fs::write(&path, cfg_src).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

fn remove_provider_in(src: &str, id: &str) -> Result<String> {
    let mut doc: toml_edit::Document = src.parse().context("config.toml is not valid TOML")?;
    if let Some(arr) = doc.get_mut("providers").and_then(|p| p.as_array_of_tables_mut()) {
        let keep: Vec<toml_edit::Table> = arr
            .iter()
            .filter(|t| t.get("id").and_then(|v| v.as_str()) != Some(id))
            .cloned()
            .collect();
        if keep.len() != arr.len() {
            arr.clear();
            for t in keep {
                arr.push(t);
            }
        }
    }
    Ok(doc.to_string())
}

/// Keep booleans and integers typed when the caller passes them as text.
fn value_for(v: &str) -> toml_edit::Value {
    if v == "true" || v == "false" {
        return toml_edit::Value::from(v == "true");
    }
    if let Ok(n) = v.parse::<i64>() {
        return toml_edit::Value::from(n);
    }
    toml_edit::Value::from(v)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Openai,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub api_key_cmd: Option<String>,
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// Take image attachments (screenshots, files) as pixels instead of OCR text.
    /// Unset = guess from the model name (`gpt-4o`, `claude…`, `…-vl` = yes).
    pub vision: Option<bool>,
}

impl ProviderConfig {
    /// Resolve the API key: env var first, then the key command.
    /// `Ok(None)` means no key configured (fine for local servers).
    pub fn resolve_api_key(&self) -> Result<Option<String>> {
        if let Some(var) = &self.api_key_env {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    return Ok(Some(v));
                }
            }
        }
        if let Some(cmd) = &self.api_key_cmd {
            let out = Process::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .with_context(|| format!("provider `{}`: running api_key_cmd", self.id))?;
            if !out.status.success() {
                bail!(
                    "provider `{}`: api_key_cmd exited with {}: {}",
                    self.id,
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !key.is_empty() {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    /// Default model first, then any extras, without duplicates.
    pub fn all_models(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        std::iter::once(&self.model)
            .chain(self.models.iter())
            .filter(|m| seen.insert(m.as_str()))
            .cloned()
            .collect()
    }

    /// True when the endpoint is on this machine, so a missing key is expected.
    pub fn is_local(&self) -> bool {
        self.base_url
            .as_deref()
            .map(|u| u.contains("://localhost") || u.contains("://127.0.0.1") || u.contains("://[::1]"))
            .unwrap_or(false)
    }
}

/// Where an AI command's answer goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiOutput {
    /// Open the chat panel and stream there.
    #[default]
    Chat,
    /// Stream silently, then paste over the selection in the front app.
    Replace,
    /// Same as replace (paste into the front app); reads better when the
    /// prompt uses `{input}` or `{clipboard}` rather than the selection.
    Paste,
    /// Stream silently, put the answer on the clipboard, toast.
    Copy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCommandConfig {
    pub id: String,
    pub title: String,
    /// Placeholders: `{selection}`, `{input}`, `{clipboard}`, `{app}`.
    pub prompt: String,
    pub system: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub output: AiOutput,
    /// Inputs Act on Selection offers this command for (`text`, `url`).
    /// Default: `["text"]` when the prompt uses `{selection}`.
    #[serde(default)]
    pub accepts: Option<Vec<String>>,
}

impl AiCommandConfig {
    pub fn accepts(&self, kind: &str) -> bool {
        match &self.accepts {
            Some(list) => list.iter().any(|k| k.eq_ignore_ascii_case(kind)),
            None => kind == "text" && self.prompt.contains("{selection}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicklinkConfig {
    pub id: Option<String>,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

impl QuicklinkConfig {
    pub fn key(&self) -> String {
        self.id.clone().unwrap_or_else(|| slug(&self.title))
    }
}

/// Lowercase, alphanumeric-and-dashes identifier derived from a title.
pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Expand a leading `~/` to the home directory.
pub fn expand_tilde(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| p.to_path_buf()),
        Err(_) => p.to_path_buf(),
    }
}

impl Config {
    /// `$OPENCAST_CONFIG_DIR`, else `~/.config/opencast` on every platform
    /// (dotfile-friendly, unlike `~/Library/Application Support`).
    pub fn config_dir() -> PathBuf {
        if let Ok(d) = std::env::var("OPENCAST_CONFIG_DIR") {
            if !d.is_empty() {
                return expand_tilde(Path::new(&d));
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("opencast")
    }

    pub fn parse(src: &str) -> Result<Self> {
        let mut cfg: Config = toml::from_str(src).context("parsing config.toml")?;
        cfg.parse_warnings = unknown_keys(src);
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        // A config is a few KB. Refuse runaway files instead of freezing the panel.
        if let Ok(meta) = std::fs::metadata(path) {
            anyhow::ensure!(
                meta.len() <= MAX_CONFIG_BYTES,
                "{} is {} MB — too large to be a real config; check it for repeated text",
                path.display(),
                meta.len() / 1_000_000
            );
        }
        let src = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut cfg = Self::parse(&src).with_context(|| format!("in {}", path.display()))?;
        if let Some(dir) = path.parent() {
            cfg.apply_includes(dir);
        }
        Ok(cfg)
    }

    /// Where a `git:<url>` include is cloned.
    pub fn include_clone_dir(config_dir: &Path, url: &str) -> PathBuf {
        let name: String = url
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or("include")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        config_dir.join("includes").join(name)
    }

    /// The file an include entry points at.
    pub fn include_path(config_dir: &Path, entry: &str) -> PathBuf {
        if let Some(url) = entry.strip_prefix("git:") {
            let dir = Self::include_clone_dir(config_dir, url);
            for name in ["opencast.toml", "config.toml"] {
                if dir.join(name).is_file() {
                    return dir.join(name);
                }
            }
            return dir.join("opencast.toml");
        }
        let p = expand_tilde(Path::new(entry));
        if p.is_relative() {
            config_dir.join(p)
        } else {
            p
        }
    }

    /// Merge `include` files. Local entries win on id clashes; problems
    /// become `include_warnings` so a broken team file never blocks startup.
    pub fn apply_includes(&mut self, config_dir: &Path) {
        for entry in self.general.include.clone() {
            let path = Self::include_path(config_dir, &entry);
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) if entry.starts_with("git:") => {
                    self.include_warnings.push(format!(
                        "include `{entry}` is not downloaded yet — run Update Extensions"
                    ));
                    continue;
                }
                Err(e) => {
                    self.include_warnings.push(format!("include `{entry}`: {e}"));
                    continue;
                }
            };
            match toml::from_str::<Config>(&text) {
                Ok(other) => {
                    for w in unknown_keys(&text) {
                        self.include_warnings.push(format!("include `{entry}`: {w}"));
                    }
                    self.merge_from(other);
                }
                Err(e) => self.include_warnings.push(format!("include `{entry}`: {e}")),
            }
        }
    }

    fn merge_from(&mut self, other: Config) {
        fn add<T: Clone>(mine: &mut Vec<T>, theirs: Vec<T>, key: impl Fn(&T) -> String) {
            for t in theirs {
                if !mine.iter().any(|m| key(m) == key(&t)) {
                    mine.push(t);
                }
            }
        }
        add(&mut self.snippets, other.snippets, |s| s.keyword.clone());
        add(&mut self.quicklinks, other.quicklinks, |q| q.key());
        add(&mut self.ai_commands, other.ai_commands, |a| a.id.clone());
        add(&mut self.ai_presets, other.ai_presets, |a| a.id.clone());
        add(&mut self.window_commands, other.window_commands, |w| w.id.clone());
        add(&mut self.pipelines, other.pipelines, |p| p.id.clone());
        add(&mut self.commands, other.commands, |c| c.match_.to_lowercase());
        add(&mut self.hotkeys, other.hotkeys, |h| h.key.to_lowercase());
        add(&mut self.providers, other.providers, |p| p.id.clone());
        add(&mut self.mcp_servers, other.mcp_servers, |m| m.id.clone());
    }

    /// `[[hotkeys]]` plus `hotkey` on `[[commands]]` entries.
    pub fn all_hotkeys(&self) -> Vec<HotkeyBinding> {
        let mut out = self.hotkeys.clone();
        let ua = self.general.universal_actions_hotkey.trim();
        let norm = crate::keymap::normalize_hotkey;
        let ua_taken = norm(ua).is_some_and(|k| {
            norm(&self.general.hotkey).as_ref() == Some(&k)
                || self.hotkeys.iter().any(|h| norm(&h.key).as_ref() == Some(&k))
                || self
                    .commands
                    .iter()
                    .any(|c| c.hotkey.as_deref().and_then(norm).as_ref() == Some(&k))
                || self
                    .commands
                    .iter()
                    .any(|c| c.match_ == "builtin:selection" && c.hotkey.is_some())
        });
        // A user binding on the same keys (or for the same command) wins over this default.
        if !ua.is_empty() && !ua_taken {
            out.push(HotkeyBinding {
                key: ua.to_string(),
                command: "builtin:selection".into(),
            });
        }
        for w in &self.window_commands {
            if let Some(k) = w.hotkey.as_ref().filter(|k| !k.trim().is_empty()) {
                out.push(HotkeyBinding {
                    key: k.clone(),
                    command: format!("window:custom:{}", w.id),
                });
            }
        }
        for p in &self.pipelines {
            if let Some(k) = p.hotkey.as_ref().filter(|k| !k.trim().is_empty()) {
                out.push(HotkeyBinding {
                    key: k.clone(),
                    command: format!("pipe:{}", p.id),
                });
            }
        }
        for c in &self.commands {
            if let Some(k) = c.hotkey.as_ref().filter(|k| !k.trim().is_empty()) {
                out.push(HotkeyBinding {
                    key: k.clone(),
                    command: c.match_.clone(),
                });
            }
        }
        out
    }

    /// Load `<dir>/config.toml`, creating it and the example scripts first if
    /// the directory has never been set up.
    pub fn load_or_init(dir: &Path) -> Result<Self> {
        let path = dir.join("config.toml");
        if !path.exists() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            std::fs::write(&path, DEFAULT_CONFIG)?;
            let scripts = dir.join("scripts");
            if !scripts.exists() {
                std::fs::create_dir_all(&scripts)?;
                for (name, body) in EXAMPLE_SCRIPTS {
                    let p = scripts.join(name);
                    std::fs::write(&p, body)?;
                    make_executable(&p)?;
                }
            }
        }
        Self::load_from(&path)
    }

    pub fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for p in &self.providers {
            if p.id.trim().is_empty() {
                bail!("a provider has an empty id");
            }
            if !ids.insert(p.id.as_str()) {
                bail!("provider id `{}` is used twice", p.id);
            }
            if p.model.trim().is_empty() {
                bail!("provider `{}` needs a `model`", p.id);
            }
        }
        if let Some(d) = &self.general.default_provider {
            if !ids.contains(d.as_str()) {
                bail!("default_provider `{d}` is not one of the [[providers]]");
            }
        }
        let mut cmd_ids = HashSet::new();
        for c in &self.ai_commands {
            if !cmd_ids.insert(c.id.as_str()) {
                bail!("ai_command id `{}` is used twice", c.id);
            }
            if let Some(p) = &c.provider {
                if !ids.contains(p.as_str()) {
                    bail!("ai_command `{}` uses unknown provider `{p}`", c.id);
                }
            }
        }
        for h in &self.hotkeys {
            if h.key.trim().is_empty() || h.command.trim().is_empty() {
                bail!("every [[hotkeys]] entry needs both `key` and `command`");
            }
        }
        for o in &self.commands {
            if o.match_.trim().is_empty() {
                bail!("every [[commands]] entry needs `match` (a command id or title)");
            }
        }
        let mut mcp_ids = HashSet::new();
        for m in &self.mcp_servers {
            m.validate()?;
            if !mcp_ids.insert(m.id.as_str()) {
                bail!("mcp server id `{}` is used twice", m.id);
            }
        }
        Ok(())
    }

    pub fn scripts_dir(&self, config_dir: &Path) -> PathBuf {
        match &self.general.scripts_dir {
            Some(p) => expand_tilde(p),
            None => config_dir.join("scripts"),
        }
    }

    pub fn default_provider_id(&self) -> Option<&str> {
        self.general
            .default_provider
            .as_deref()
            .or_else(|| self.providers.first().map(|p| p.id.as_str()))
    }

    /// The theme file, if configured. Relative paths resolve against the config dir.
    pub fn theme_path(&self, config_dir: &Path) -> Option<PathBuf> {
        let p = self.general.theme_file.as_ref()?;
        let p = expand_tilde(p);
        Some(if p.is_absolute() { p } else { config_dir.join(p) })
    }

    pub fn load_theme(&self, config_dir: &Path) -> Result<Option<ThemeFile>> {
        let Some(path) = self.theme_path(config_dir) else {
            return Ok(None);
        };
        let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        ThemeFile::parse(&src)
            .map(Some)
            .with_context(|| format!("in {}", path.display()))
    }
}

#[cfg(unix)]
fn make_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(p)?.permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(p, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_parses() {
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.general.hotkey, "Alt+Space");
        assert_eq!(cfg.default_provider_id(), Some("ollama"));
        assert!(cfg.providers[0].is_local());
        assert!(cfg.ai_commands.iter().any(|c| c.id == "fix-grammar"));
        assert_eq!(cfg.quicklinks[0].key(), "search-github");
        assert_eq!(cfg.general.fallbacks, vec!["quicklinks", "files"]);
        assert_eq!(cfg.keys.ask_ai, "Tab");
    }

    #[test]
    fn empty_config_is_valid() {
        let cfg = Config::parse("").unwrap();
        assert!(cfg.providers.is_empty());
        assert_eq!(cfg.default_provider_id(), None);
        assert_eq!(cfg.general.fallback_threshold, 3);
        assert!(cfg.general.script_login_shell);
    }

    #[test]
    fn rejects_unknown_default_provider() {
        let err = Config::parse("[general]\ndefault_provider = \"nope\"").unwrap_err();
        assert!(format!("{err:#}").contains("nope"));
    }

    #[test]
    fn rejects_duplicate_provider_ids() {
        let src = r#"
            [[providers]]
            id = "a"
            kind = "openai"
            model = "m"
            [[providers]]
            id = "a"
            kind = "gemini"
            model = "m"
        "#;
        assert!(Config::parse(src).is_err());
    }

    #[test]
    fn api_key_from_cmd_and_models_dedupe() {
        let p = ProviderConfig {
            id: "x".into(),
            kind: ProviderKind::Openai,
            base_url: None,
            api_key_env: Some("OPENCAST_TEST_SURELY_UNSET_VAR".into()),
            api_key_cmd: Some("printf ' sk-test \\n'".into()),
            model: "a".into(),
            models: vec!["b".into(), "a".into()],
            headers: Default::default(),
            max_tokens: None,
            temperature: None,
            vision: None,
        };
        assert_eq!(p.resolve_api_key().unwrap().as_deref(), Some("sk-test"));
        assert_eq!(p.all_models(), vec!["a", "b"]);
    }

    #[test]
    fn init_writes_config_and_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load_or_init(dir.path()).unwrap();
        assert!(dir.path().join("config.toml").exists());
        assert!(dir.path().join("scripts/uuid.sh").exists());
        assert!(dir.path().join("scripts/recent-branches.sh").exists());
        assert_eq!(cfg.scripts_dir(dir.path()), dir.path().join("scripts"));
    }

    #[test]
    fn set_general_key_inserts_and_replaces_keeping_comments() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "# top comment\n[general]\nhotkey = \"Alt+Space\" # keep me\n\n[[providers]]\nid = \"x\"\nkind = \"openai\"\nmodel = \"m\"\n",
        )
        .unwrap();
        set_general_key(dir.path(), "theme", "dark").unwrap();
        let src = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(src.contains("theme = \"dark\""));
        assert!(src.contains("# top comment"));
        assert!(src.contains("# keep me"));
        set_general_key(dir.path(), "theme", "light").unwrap();
        let src = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(src.contains("theme = \"light\""));
        assert!(!src.contains("theme = \"dark\""));
        assert!(src.contains("hotkey = \"Alt+Space\""));
        // Still parses, provider intact.
        let cfg = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert_eq!(cfg.providers[0].id, "x");
        assert_eq!(cfg.general.theme, "light");
    }

    #[test]
    fn set_general_key_creates_the_table() {
        let out = set_general_key_in("[[quicklinks]]\ntitle = \"a\"\nurl = \"http://x\"\n", "theme", "dark").unwrap();
        let cfg = Config::parse(&out).unwrap();
        assert_eq!(cfg.general.theme, "dark");
        assert_eq!(cfg.quicklinks.len(), 1);
        let out = set_general_key_in("", "hyperkey", "true").unwrap();
        assert!(Config::parse(&out).unwrap().general.hyperkey);
    }

    #[test]
    fn command_overrides_keys_and_ai_options_parse() {
        let cfg = Config::parse(
            r#"
            [general]
            fallbacks = ["link:search-kagi", "files"]
            fallback_threshold = 2
            theme_file = "themes/mine.toml"

            [keys]
            ask_ai = "Cmd+Return"

            [[commands]]
            match = "Slack"
            alias = "sl"
            aliases = ["SLK"]
            boost = 50
            icon = "bubble.left.fill"

            [[commands]]
            match = "builtin:shortcuts"
            hidden = true

            [[ai_commands]]
            id = "fix"
            title = "Fix"
            prompt = "{selection}"
            temperature = 0.1
            max_tokens = 500
            output = "replace"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.commands[0].all_aliases(), vec!["sl", "slk"]);
        assert_eq!(cfg.commands[0].boost, 50);
        assert!(cfg.commands[1].hidden);
        assert_eq!(cfg.keys.ask_ai, "Cmd+Return");
        assert_eq!(cfg.keys.actions, "Cmd+K");
        assert_eq!(cfg.ai_commands[0].output, AiOutput::Replace);
        assert_eq!(cfg.ai_commands[0].max_tokens, Some(500));
        assert_eq!(cfg.general.fallback_threshold, 2);
        assert_eq!(
            cfg.theme_path(Path::new("/cfg")),
            Some(PathBuf::from("/cfg/themes/mine.toml"))
        );
    }

    #[test]
    fn add_and_remove_providers_keep_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), DEFAULT_CONFIG).unwrap();
        let p = ProviderConfig {
            id: "claude".into(),
            kind: ProviderKind::Anthropic,
            base_url: None,
            api_key_env: None,
            api_key_cmd: Some("security find-generic-password -s opencast-claude -w".into()),
            model: "claude-sonnet-5-5".into(),
            models: vec!["claude-opus-5-5".into()],
            headers: Default::default(),
            max_tokens: None,
            temperature: None,
            vision: None,
        };
        add_provider(dir.path(), &p).unwrap();
        // Adding again replaces, not duplicates.
        add_provider(dir.path(), &p).unwrap();
        let cfg = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert_eq!(cfg.providers.iter().filter(|x| x.id == "claude").count(), 1);
        assert!(cfg.providers.iter().any(|x| x.id == "ollama"));
        let src = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(src.contains("# OpenCast configuration"));
        set_general_key(dir.path(), "default_provider", "claude").unwrap();
        assert!(remove_provider(dir.path(), "claude").unwrap());
        assert!(!remove_provider(dir.path(), "claude").unwrap());
        let cfg = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert!(cfg.general.default_provider.is_none());
        assert!(cfg.providers.iter().all(|x| x.id != "claude"));
    }

    #[test]
    fn theme_file_parses_and_validates() {
        let t = ThemeFile::parse("accent = \"#ff6a00\"\nradius = 12\nglass = false\n").unwrap();
        assert_eq!(t.accent.as_deref(), Some("#ff6a00"));
        assert_eq!(t.radius, Some(12.0));
        assert_eq!(t.glass, Some(false));
        assert!(ThemeFile::parse("accent = \"orange\"").is_err());
        assert_eq!(ThemeFile::parse("").unwrap(), ThemeFile::default());
    }
}

#[cfg(test)]
mod header_regression {
    /// The pre-0.2 writer copied everything above `[general]` on each change;
    /// 24 theme toggles grew one config to 2.7 GB.
    #[test]
    fn repeated_writes_keep_one_header() {
        let mut s = crate::config::DEFAULT_CONFIG.to_string();
        for i in 0..6 {
            s = super::set_general_key_in(&s, "theme", if i % 2 == 0 { "dark" } else { "light" }).unwrap();
            s = super::set_general_key_in(&s, "presentation", "spotlight").unwrap();
        }
        assert_eq!(s.matches("# OpenCast configuration").count(), 1);
        assert!(s.len() < crate::config::DEFAULT_CONFIG.len() + 200);
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    #[test]
    fn command_hotkeys_set_replace_clear_and_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let base = format!(
            "{DEFAULT_CONFIG}\n[[hotkeys]]\nkey = \"Ctrl+Alt+Left\"\ncommand = \"Left Half\"\n\n[[commands]]\nmatch = \"builtin:clipboard\"\nalias = \"cb\"\n"
        );
        std::fs::write(dir.path().join("config.toml"), &base).unwrap();
        set_command_hotkey(dir.path(), "builtin:clipboard", "Hyper+V").unwrap();
        let c = Config::load_from(&dir.path().join("config.toml")).unwrap();
        let cb = c.commands.iter().find(|o| o.match_ == "builtin:clipboard").unwrap();
        assert_eq!(cb.hotkey.as_deref(), Some("Hyper+V"));
        assert_eq!(cb.alias.as_deref(), Some("cb"), "other fields kept");
        // New entry for a command without one; conflict refused.
        set_command_hotkey(dir.path(), "builtin:notes", "Ctrl+Alt+N").unwrap();
        let err = set_command_hotkey(dir.path(), "builtin:chat", "ctrl+alt+left").unwrap_err();
        assert!(err.to_string().contains("Left Half"), "{err}");
        assert!(
            set_command_hotkey(dir.path(), "builtin:chat", "Alt+Space").is_err(),
            "launcher hotkey"
        );
        // Moving [[hotkeys]] binding into [[commands]] removes the old entry.
        set_command_hotkey(dir.path(), "Left Half", "Ctrl+Alt+H").unwrap();
        let c = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert!(c.hotkeys.is_empty());
        // Three bindings plus the default Act on Selection hotkey.
        assert_eq!(c.all_hotkeys().len(), 4);
        // Clear.
        set_command_hotkey(dir.path(), "builtin:clipboard", "").unwrap();
        let c = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert!(c
            .commands
            .iter()
            .find(|o| o.match_ == "builtin:clipboard")
            .unwrap()
            .hotkey
            .is_none());
    }

    #[test]
    fn snippets_and_includes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("team.toml"),
            "[[snippets]]\nkeyword = \";addr\"\ntext = \"1 Main St\"\n\n[[snippets]]\nkeyword = \";sig\"\ntext = \"TEAM SIG\"\n\n[[ai_presets]]\nid = \"rev\"\ntitle = \"Reviewer\"\ntools = [\"github__*\"]\n",
        )
        .unwrap();
        let base = DEFAULT_CONFIG.replacen("[general]", "[general]\ninclude = [\"team.toml\", \"missing.toml\"]", 1);
        std::fs::write(dir.path().join("config.toml"), base).unwrap();
        let c = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert!(c.snippets.iter().any(|s| s.keyword == ";addr"));
        let sig: Vec<_> = c.snippets.iter().filter(|s| s.keyword == ";sig").collect();
        assert_eq!(sig.len(), 1);
        assert_ne!(sig[0].text, "TEAM SIG", "local wins");
        assert_eq!(c.ai_presets[0].tools.as_deref(), Some(&["github__*".to_string()][..]));
        assert_eq!(c.include_warnings.len(), 1);

        add_snippet(dir.path(), ";hi", "Hello {cursor}", Some("Greeting")).unwrap();
        assert!(add_snippet(dir.path(), ";hi", "again", None).is_err());
        let c = Config::load_from(&dir.path().join("config.toml")).unwrap();
        assert!(c
            .snippets
            .iter()
            .any(|s| s.keyword == ";hi" && s.text == "Hello {cursor}"));
    }

    #[test]
    fn unknown_config_keys_are_reported() {
        // Typos serde would swallow: a wrong key, a wrong section name, a wrong
        // key name inside a known section.
        let c = Config::parse(
            "[general]\nscipt_timeout_secs = 5\n\n[[ai_command]]\nid = \"x\"\ntitle = \"X\"\nprompt = \"p\"\n\n[keys]\naction = \"Cmd+K\"\n",
        )
        .unwrap();
        let w = c.parse_warnings.join("\n");
        assert!(
            w.contains("scipt_timeout_secs") && w.contains("did you mean `script_timeout_secs`"),
            "{w}"
        );
        assert!(
            w.contains("`ai_command`") && w.contains("did you mean `ai_commands`"),
            "{w}"
        );
        assert!(
            w.contains("[keys] action") && w.contains("did you mean `actions`"),
            "{w}"
        );

        // The documented surface must stay known: a setting in the example config
        // that the structs do not have fails here.
        assert_eq!(unknown_keys(DEFAULT_CONFIG), Vec::<String>::new());
        assert!(Config::parse(DEFAULT_CONFIG).unwrap().parse_warnings.is_empty());
    }

    #[test]
    fn globs() {
        assert!(glob_match("github__*", "GitHub__search"));
        assert!(glob_match("*__get_*", "gh__get_me"));
        assert!(!glob_match("github__*", "gitlab__x"));
        assert!(glob_match("exact", "exact"));
        assert!(glob_match("*", "anything"));
    }
}
