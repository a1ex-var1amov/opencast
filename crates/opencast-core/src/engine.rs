//! Engine: config + registry + providers, and the decision of what a
//! command does. The shell only carries out the returned [`Plan`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};

use crate::apps::{self, AppEntry};
use crate::commands::{Argument, Command, CommandKind, Registry};
use crate::config::{AiCommandConfig, Config, QuicklinkConfig};
use crate::layouts;
use crate::providers::Router;
use crate::scripts::{self, ScriptMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Chat,
    Reload,
    OpenConfig,
    OpenScripts,
    Quit,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    PresentationIsland,
    PresentationSpotlight,
    Settings,
    Shortcuts,
    StayAwakeOff,
    StayAwakeIndefinite,
    StayAwake30m,
    StayAwake1h,
    StayAwake3h,
    EmptyTrash,
    LockScreen,
    Sleep,
    EjectAll,
    QuitFrontApp,
    KillFrontApp,
    ClipboardHistory,
    SearchScreenshots,
    PasteLatestScreenshot,
    ToggleDarkMode,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
    EmojiPicker,
    NewNote,
    Notes,
}

const BUILTINS: &[(Builtin, &str, &str, &str, &[&str])] = &[
    (
        Builtin::Chat,
        "chat",
        "AI Chat",
        "💬",
        &["ask", "assistant", "llm", "gpt", "claude"],
    ),
    (
        Builtin::Reload,
        "reload",
        "Reload Commands",
        "🔄",
        &["refresh", "config"],
    ),
    (
        Builtin::OpenConfig,
        "config",
        "Open Config Folder",
        "📁",
        &["toml", "providers", "hotkeys", "folder", "file"],
    ),
    (
        Builtin::OpenScripts,
        "scripts",
        "Open Scripts Folder",
        "📂",
        &["script commands"],
    ),
    (Builtin::Quit, "quit", "Quit OpenCast", "⏻", &["exit"]),
    (
        Builtin::ThemeSystem,
        "theme-system",
        "Appearance: System",
        "🖥️",
        &["theme", "appearance", "dark", "light", "color", "settings"],
    ),
    (
        Builtin::ThemeLight,
        "theme-light",
        "Appearance: Light",
        "☀️",
        &["theme", "appearance", "light", "color", "settings"],
    ),
    (
        Builtin::ThemeDark,
        "theme-dark",
        "Appearance: Dark",
        "🌙",
        &["theme", "appearance", "dark", "color", "settings"],
    ),
    (
        Builtin::PresentationIsland,
        "presentation-island",
        "Presentation: Island",
        "🏝️",
        &["presentation", "island", "layout", "panel", "settings"],
    ),
    (
        Builtin::PresentationSpotlight,
        "presentation-spotlight",
        "Presentation: Spotlight",
        "🔦",
        &["presentation", "spotlight", "layout", "panel", "centered", "settings"],
    ),
    (
        Builtin::Settings,
        "settings",
        "Settings",
        "⚙️",
        &["config", "preferences", "theme", "appearance", "presentation", "awake"],
    ),
    (
        Builtin::Shortcuts,
        "shortcuts",
        "Shortcuts",
        "⌨️",
        &["hotkey", "keys", "keymap", "tab", "click", "shortcut"],
    ),
    (
        Builtin::StayAwakeOff,
        "stay-awake-off",
        "Stay Awake: Off",
        "💤",
        &["awake", "caffeine", "sleep", "amphetamine", "keepingyouawake", "off"],
    ),
    (
        Builtin::StayAwakeIndefinite,
        "stay-awake-indefinite",
        "Stay Awake: Indefinite",
        "☕️",
        &["awake", "caffeine", "sleep", "amphetamine", "keepingyouawake"],
    ),
    (
        Builtin::StayAwake30m,
        "stay-awake-30m",
        "Stay Awake: 30 minutes",
        "☕️",
        &["awake", "caffeine", "sleep", "amphetamine", "keepingyouawake", "30"],
    ),
    (
        Builtin::StayAwake1h,
        "stay-awake-1h",
        "Stay Awake: 1 hour",
        "☕️",
        &["awake", "caffeine", "sleep", "amphetamine", "keepingyouawake", "60"],
    ),
    (
        Builtin::StayAwake3h,
        "stay-awake-3h",
        "Stay Awake: 3 hours",
        "☕️",
        &["awake", "caffeine", "sleep", "amphetamine", "keepingyouawake"],
    ),
    (
        Builtin::ClipboardHistory,
        "clipboard",
        "Clipboard History",
        "📋",
        &["clip", "clipboard", "paste", "cb", "history"],
    ),
    (
        Builtin::SearchScreenshots,
        "screenshots",
        "Search Screenshots",
        "🖼",
        &["shots", "screenshot", "screenshots", "capture", "ocr"],
    ),
    (
        Builtin::PasteLatestScreenshot,
        "paste-latest-screenshot",
        "Paste Latest Screenshot",
        "🖼",
        &["screenshot", "paste", "latest", "shot"],
    ),
    (
        Builtin::EmojiPicker,
        "emoji",
        "Emoji",
        "😀",
        &["emoji", "emo", "emojis", "glyph"],
    ),
    (
        Builtin::Notes,
        "notes",
        "Notes",
        "📝",
        &["note", "notes", "memo"],
    ),
    (
        Builtin::NewNote,
        "new-note",
        "New Note",
        "📝",
        &["note", "notes", "memo", "write"],
    ),
    (
        Builtin::ToggleDarkMode,
        "dark-mode",
        "Toggle Dark Mode",
        "🌗",
        &["dark", "light", "appearance", "system", "macos"],
    ),
    (
        Builtin::VolumeUp,
        "volume-up",
        "Volume Up",
        "🔊",
        &["volume", "sound", "audio", "louder"],
    ),
    (
        Builtin::VolumeDown,
        "volume-down",
        "Volume Down",
        "🔉",
        &["volume", "sound", "audio", "quieter"],
    ),
    (
        Builtin::VolumeMute,
        "volume-mute",
        "Toggle Mute",
        "🔇",
        &["volume", "mute", "unmute", "sound", "audio"],
    ),
    (
        Builtin::BrightnessUp,
        "brightness-up",
        "Brightness Up",
        "☀️",
        &["brightness", "display", "screen", "brighter"],
    ),
    (
        Builtin::BrightnessDown,
        "brightness-down",
        "Brightness Down",
        "🌤",
        &["brightness", "display", "screen", "dimmer"],
    ),
    (
        Builtin::EmptyTrash,
        "empty-trash",
        "Empty Trash",
        "🗑",
        &["trash", "bin", "system"],
    ),
    (
        Builtin::LockScreen,
        "lock-screen",
        "Lock Screen",
        "🔒",
        &["lock", "screensaver", "system"],
    ),
    (
        Builtin::Sleep,
        "sleep",
        "Sleep",
        "😴",
        &["sleep", "suspend", "system"],
    ),
    (
        Builtin::EjectAll,
        "eject-all",
        "Eject All",
        "💿",
        &["eject", "disk", "volume", "system"],
    ),
    (
        Builtin::QuitFrontApp,
        "quit-front",
        "Quit Front App",
        "✕",
        &["quit", "close", "system"],
    ),
    (
        Builtin::KillFrontApp,
        "kill-front",
        "Force Quit Front App",
        "☠️",
        &["kill", "force", "quit", "system"],
    ),
];

/// In-app key rows. Searchable via the Shortcuts command (`query` = `shortcut`).
const SHORTCUT_ROWS: &[(&str, &str, &str, &[&str])] = &[
    ("shortcut:return", "Return — Run selected", "↩", &["return", "enter", "shortcut"]),
    ("shortcut:tab", "Tab — Ask AI", "⇥", &["tab", "ask", "shortcut"]),
    ("shortcut:backtick", "Backtick — Click Text", "`", &["`", "click", "seek", "shortcut"]),
    ("shortcut:esc", "Esc — Back / hide", "⎋", &["escape", "esc", "shortcut"]),
    ("shortcut:arrows", "Arrows — Move selection", "↑↓", &["up", "down", "arrows", "shortcut"]),
];

/// Rectangle-style window actions on the frontmost window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    FirstThird,
    CenterThird,
    LastThird,
    Maximize,
    Center,
    AlmostMaximize,
    NextDisplay,
    PreviousDisplay,
    RestoreSize,
}

pub const WINDOW_ACTIONS: &[(WindowAction, &str, &str, &str)] = &[
    (WindowAction::LeftHalf, "left-half", "Left Half", "◧"),
    (WindowAction::RightHalf, "right-half", "Right Half", "◨"),
    (WindowAction::TopHalf, "top-half", "Top Half", "⬒"),
    (WindowAction::BottomHalf, "bottom-half", "Bottom Half", "⬓"),
    (WindowAction::TopLeft, "top-left", "Top Left Quarter", "◰"),
    (WindowAction::TopRight, "top-right", "Top Right Quarter", "◳"),
    (WindowAction::BottomLeft, "bottom-left", "Bottom Left Quarter", "◱"),
    (WindowAction::BottomRight, "bottom-right", "Bottom Right Quarter", "◲"),
    (WindowAction::FirstThird, "first-third", "First Third", "⅓"),
    (WindowAction::CenterThird, "center-third", "Center Third", "⅓"),
    (WindowAction::LastThird, "last-third", "Last Third", "⅓"),
    (WindowAction::Maximize, "maximize", "Maximize", "⛶"),
    (WindowAction::AlmostMaximize, "almost-maximize", "Almost Maximize", "❐"),
    (WindowAction::Center, "center", "Center", "◎"),
    (WindowAction::NextDisplay, "next-display", "Next Display", "→"),
    (WindowAction::PreviousDisplay, "previous-display", "Previous Display", "←"),
    (WindowAction::RestoreSize, "restore", "Restore Window Size", "↩"),
];

impl WindowAction {
    pub fn key(self) -> &'static str {
        WINDOW_ACTIONS.iter().find(|(a, ..)| *a == self).unwrap().1
    }

    pub fn title(self) -> &'static str {
        WINDOW_ACTIONS.iter().find(|(a, ..)| *a == self).unwrap().2
    }

    /// Target rectangle as fractions `(x, y, w, h)` of the work area, or
    /// `None` for [`WindowAction::Center`], which keeps the window's size.
    pub fn frac(self) -> Option<(f64, f64, f64, f64)> {
        let third = 1.0 / 3.0;
        Some(match self {
            Self::LeftHalf => (0.0, 0.0, 0.5, 1.0),
            Self::RightHalf => (0.5, 0.0, 0.5, 1.0),
            Self::TopHalf => (0.0, 0.0, 1.0, 0.5),
            Self::BottomHalf => (0.0, 0.5, 1.0, 0.5),
            Self::TopLeft => (0.0, 0.0, 0.5, 0.5),
            Self::TopRight => (0.5, 0.0, 0.5, 0.5),
            Self::BottomLeft => (0.0, 0.5, 0.5, 0.5),
            Self::BottomRight => (0.5, 0.5, 0.5, 0.5),
            Self::FirstThird => (0.0, 0.0, third, 1.0),
            Self::CenterThird => (third, 0.0, third, 1.0),
            Self::LastThird => (2.0 * third, 0.0, third, 1.0),
            Self::Maximize => (0.0, 0.0, 1.0, 1.0),
            Self::AlmostMaximize => (0.05, 0.05, 0.9, 0.9),
            Self::Center | Self::NextDisplay | Self::PreviousDisplay | Self::RestoreSize => {
                return None
            }
        })
    }

    pub fn is_display_move(self) -> bool {
        matches!(self, Self::NextDisplay | Self::PreviousDisplay)
    }

    pub fn is_restore(self) -> bool {
        matches!(self, Self::RestoreSize)
    }
}

/// Operations on saved window layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutOp {
    Remember(String),
    /// Restore by slug (the id suffix of a `layout:restore:` command).
    Restore(String),
    Forget(String),
}

/// What the shell should do for a command.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    LaunchApp(PathBuf),
    RunScript {
        meta: ScriptMeta,
        args: Vec<String>,
    },
    OpenUrl(String),
    Copy(String),
    /// Open AI chat. When `needs_selection`, the shell grabs the frontmost
    /// app's selected text and calls [`fill_selection`] on the prompt first.
    Chat {
        prompt: Option<String>,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        needs_selection: bool,
    },
    Builtin(Builtin),
    /// Write `theme` to config and tell the panel to apply it.
    SetTheme(&'static str),
    /// Write `presentation` to config and tell the panel to apply it.
    SetPresentation(&'static str),
    /// Prevent idle sleep: `off`, `indefinite`, `30m`, `1h`, `3h`.
    StayAwake(&'static str),
    /// Keep the panel open and set the search field.
    FillQuery(String),
    /// Copy snippet text (shell pastes when possible).
    PasteText(String),
    /// Run a one-shot system shell line (empty trash, lock, …).
    SystemShell(&'static str),
    /// Quit an app by display name (AppleScript).
    QuitApp(String),
    /// Persist a quick note and toast.
    SaveNote { title: String, body: String },
    /// Activate the current selection (Return).
    RunSelected,
    /// Esc behavior in the panel.
    DoEscape,
    /// Flash a hint; stay open.
    Hint(String),
    /// Ask AI with whatever is in the search field (Swift supplies the text).
    AskAi,
    Window(WindowAction),
    Layout(LayoutOp),
    /// Superkey's Seek: OCR the screen, let the user pick text, click it.
    Seek,
}

impl PartialEq for ScriptMeta {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

pub struct Engine {
    pub config: Config,
    pub config_dir: PathBuf,
    pub registry: Registry,
    pub router: Router,
    pub warnings: Vec<String>,
    apps: HashMap<String, PathBuf>,
    scripts: HashMap<String, ScriptMeta>,
    ai: HashMap<String, AiCommandConfig>,
    links: HashMap<String, QuicklinkConfig>,
    snippets: HashMap<String, String>,
    /// `layout:restore:<slug>` command id → layout slug.
    restore_ids: HashMap<String, String>,
}

impl Engine {
    /// Load everything from the config dir, creating it on first run.
    /// Never fails: problems become warnings so the launcher still opens.
    pub fn load(config_dir: &Path) -> Self {
        Self::load_with_apps(config_dir, apps::scan())
    }

    /// Like [`Self::load`], but use a pre-built app list (macOS Spotlight scan).
    pub fn load_with_apps(config_dir: &Path, apps: Vec<AppEntry>) -> Self {
        let mut warnings = Vec::new();
        let config = Config::load_or_init(config_dir).unwrap_or_else(|e| {
            warnings.push(format!("{e:#}"));
            Config::default()
        });
        let (scripts, script_warnings) = scripts::discover(&config.scripts_dir(config_dir));
        warnings.extend(script_warnings);
        let saved_layouts = layouts::list(config_dir);
        Self::assemble(config, config_dir.to_path_buf(), apps, scripts, saved_layouts, warnings)
    }

    /// Build an engine from already-gathered parts (used by tests).
    pub fn assemble(
        config: Config,
        config_dir: PathBuf,
        app_list: Vec<AppEntry>,
        script_list: Vec<ScriptMeta>,
        saved_layouts: Vec<(String, String, usize)>,
        mut warnings: Vec<String>,
    ) -> Self {
        let (router, router_warnings) = Router::from_config(&config);
        warnings.extend(router_warnings);

        let mut commands = Vec::new();
        let mut apps = HashMap::new();
        let mut scripts = HashMap::new();
        let mut ai = HashMap::new();
        let mut links = HashMap::new();
        let mut restore_ids = HashMap::new();

        for (_, key, title, icon, kw) in BUILTINS {
            commands.push(Command {
                id: format!("builtin:{key}"),
                title: title.to_string(),
                subtitle: None,
                icon: Some(icon.to_string()),
                kind: CommandKind::Builtin,
                arguments: vec![],
                hotkey: None,
                keywords: kw.iter().map(|s| s.to_string()).collect(),
            });
        }
        if let Some(c) = commands.iter_mut().find(|c| c.id == "builtin:new-note") {
            c.arguments = vec![
                Argument {
                    placeholder: "Title".into(),
                    optional: true,
                    secure: false,
                },
                Argument {
                    placeholder: "Body".into(),
                    optional: false,
                    secure: false,
                },
            ];
        }

        for (_, key, title, icon) in WINDOW_ACTIONS {
            commands.push(Command {
                id: format!("window:{key}"),
                title: title.to_string(),
                subtitle: None,
                icon: Some(icon.to_string()),
                kind: CommandKind::Window,
                arguments: vec![],
                hotkey: None,
                keywords: vec!["window".into(), "snap".into(), "tile".into(), "rectangle".into()],
            });
        }

        commands.push(Command {
            id: "seek".into(),
            title: "Click Text".into(),
            subtitle: Some("Type what you see on screen, click it".into()),
            icon: Some("🎯".into()),
            kind: CommandKind::Builtin,
            arguments: vec![],
            hotkey: None,
                keywords: vec!["seek".into(), "click".into(), "ocr".into(), "mouse".into(), "superkey".into()],
        });

        commands.push(Command {
            id: "layout:remember".into(),
            title: "Remember Layout".into(),
            subtitle: Some("Save where every window is right now".into()),
            icon: Some("📸".into()),
            kind: CommandKind::Layout,
            arguments: vec![Argument {
                placeholder: "Layout name".into(),
                optional: false,
                secure: false,
            }],
            hotkey: None,
                keywords: vec!["window".into(), "workspace".into(), "arrangement".into(), "save".into()],
        });
        for (slug, name, n) in &saved_layouts {
            let id = format!("layout:restore:{slug}");
            commands.push(Command {
                id: id.clone(),
                title: format!("Restore Layout: {name}"),
                subtitle: Some(format!("{n} window{}", if *n == 1 { "" } else { "s" })),
                icon: Some("🗂".into()),
                kind: CommandKind::Layout,
                arguments: vec![],
                hotkey: None,
                keywords: vec!["window".into(), "workspace".into(), "layout".into(), name.clone()],
            });
            restore_ids.insert(id, slug.clone());
        }
        if !saved_layouts.is_empty() {
            commands.push(Command {
                id: "layout:forget".into(),
                title: "Forget Layout".into(),
                subtitle: Some("Delete a saved layout".into()),
                icon: Some("🗑".into()),
                kind: CommandKind::Layout,
                arguments: vec![Argument {
                    placeholder: "Layout name".into(),
                    optional: false,
                    secure: false,
                }],
                hotkey: None,
                keywords: vec!["layout".into(), "delete".into()],
            });
        }

        for c in &config.ai_commands {
            let has_input = c.prompt.contains("{input}");
            let has_sel = c.prompt.contains("{selection}");
            let id = format!("ai:{}", c.id);
            let provider = c.provider.as_deref().or(config.default_provider_id());
            commands.push(Command {
                id: id.clone(),
                title: c.title.clone(),
                subtitle: Some(match (provider, &c.model) {
                    (Some(p), Some(m)) => format!("AI · {p} · {m}"),
                    (Some(p), None) => format!("AI · {p}"),
                    _ => "AI · no provider configured".into(),
                }),
                icon: c.icon.clone().or(Some("✨".into())),
                kind: CommandKind::Ai,
                arguments: if has_input {
                    vec![Argument {
                        placeholder: "Input".into(),
                        optional: has_sel,
                        secure: false,
                    }]
                } else {
                    vec![]
                },
                hotkey: None,
                keywords: c.keywords.clone(),
            });
            ai.insert(id, c.clone());
        }

        for s in script_list {
            let id = format!("script:{}", s.path.display());
            commands.push(Command {
                id: id.clone(),
                title: s.title.clone(),
                subtitle: Some(
                    s.description
                        .clone()
                        .or(s.package.clone())
                        .unwrap_or_else(|| "Script".into()),
                ),
                icon: s.icon.clone().or(Some("📜".into())),
                kind: CommandKind::Script,
                arguments: s.arguments.clone(),
                hotkey: None,
                keywords: s.package.iter().cloned().collect(),
            });
            scripts.insert(id, s);
        }

        for l in &config.quicklinks {
            let id = format!("link:{}", l.key());
            commands.push(Command {
                id: id.clone(),
                title: l.title.clone(),
                subtitle: Some(host_of(&l.url)),
                icon: l.icon.clone().or(Some("🔗".into())),
                kind: CommandKind::Quicklink,
                arguments: if l.url.contains("{query}") {
                    vec![Argument {
                        placeholder: "Query".into(),
                        optional: false,
                        secure: false,
                    }]
                } else {
                    vec![]
                },
                hotkey: None,
                keywords: l.keywords.clone(),
            });
            links.insert(id, l.clone());
        }

        let mut snippets = HashMap::new();
        for s in &config.snippets {
            let key = s.keyword.trim();
            if key.is_empty() || s.text.is_empty() {
                continue;
            }
            let id = format!("snippet:{key}");
            let mut kw = s.keywords.clone();
            kw.push(key.to_string());
            kw.push("snippet".into());
            commands.push(Command {
                id: id.clone(),
                title: s.name.clone().unwrap_or_else(|| format!("Snippet: {key}")),
                subtitle: Some(s.text.chars().take(60).collect::<String>()),
                icon: Some("✂️".into()),
                kind: CommandKind::Snippet,
                arguments: vec![],
                hotkey: None,
                keywords: kw,
            });
            snippets.insert(id, s.text.clone());
        }

        for a in app_list {
            let id = format!("app:{}", a.path.display());
            commands.push(Command {
                id: id.clone(),
                title: a.name,
                subtitle: None,
                icon: None,
                kind: CommandKind::App,
                arguments: vec![],
                hotkey: None,
                keywords: vec![],
            });
            apps.insert(id, a.path);
        }

        for (id, title, icon, kw) in SHORTCUT_ROWS {
            commands.push(Command {
                id: (*id).into(),
                title: (*title).into(),
                subtitle: Some("In-app key".into()),
                icon: Some((*icon).into()),
                kind: CommandKind::Builtin,
                arguments: vec![],
                hotkey: None,
                keywords: kw.iter().map(|s| (*s).into()).collect(),
            });
        }

        match config.general.theme.as_str() {
            "system" | "light" | "dark" => {}
            other => warnings.push(format!(
                "theme `{other}` is unknown; use system, light, or dark"
            )),
        }
        match config.general.presentation.as_str() {
            "island" | "spotlight" => {}
            other => warnings.push(format!(
                "presentation `{other}` is unknown; use island or spotlight"
            )),
        }

        // Attach [[hotkeys]] to matching commands (by id or title).
        let mut key_for: HashMap<String, String> = HashMap::new();
        for h in &config.hotkeys {
            let wanted = h.command.trim();
            let id = commands
                .iter()
                .find(|c| c.id == wanted || c.title.eq_ignore_ascii_case(wanted))
                .map(|c| c.id.clone());
            if let Some(id) = id {
                key_for.insert(id, h.key.trim().to_string());
            } else {
                warnings.push(format!(
                    "hotkey `{}`: no command named `{}` (use a command title or id)",
                    h.key, h.command
                ));
            }
        }
        for c in &mut commands {
            if let Some(key) = key_for.get(&c.id) {
                c.hotkey = Some(key.clone());
                c.keywords.push(key.clone());
            }
        }

        Self {
            config,
            config_dir,
            registry: Registry::new(commands),
            router,
            warnings,
            apps,
            scripts,
            ai,
            links,
            snippets,
            restore_ids,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Command> {
        let usage = crate::usage::counts(&self.config_dir);
        let pins = crate::pins::load(&self.config_dir);
        let mut out = self.registry.search_with_usage(query, limit, &usage, &pins);
        let q = query.trim();
        let ql = q.to_lowercase();

        // Emoji overlay (not in the permanent registry).
        for row in crate::emoji::matching(q, 20) {
            if !out.iter().any(|c| c.id == row.id) {
                out.push(row);
            }
        }

        for row in crate::notes::matching(&self.config_dir, q, 20) {
            if !out.iter().any(|c| c.id == row.id) {
                out.push(row);
            }
        }

        // Quit-by-name: only when the query looks like a quit.
        if ql == "quit" || ql.starts_with("quit ") || ql.starts_with("kill ") {
            let needle = ql
                .trim_start_matches("quit")
                .trim_start_matches("kill")
                .trim();
            for (id, path) in &self.apps {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("App");
                if !needle.is_empty() && !name.to_lowercase().contains(needle) {
                    continue;
                }
                let quit_id = format!("quit:{id}");
                if out.iter().any(|c| c.id == quit_id) {
                    continue;
                }
                out.push(Command {
                    id: quit_id,
                    title: format!("Quit {name}"),
                    subtitle: Some("System".into()),
                    icon: Some("✕".into()),
                    kind: CommandKind::Builtin,
                    arguments: vec![],
                    hotkey: None,
                    keywords: vec!["quit".into(), "close".into(), "kill".into()],
                });
                if out.len() >= limit {
                    break;
                }
            }
        }

        // Fallback quicklinks when hits are thin.
        if !q.is_empty() {
            let strong = out
                .iter()
                .filter(|c| c.kind != CommandKind::Calculator)
                .count();
            if strong < 3 {
                for link in &self.config.quicklinks {
                    if !link.url.contains("{query}") {
                        continue;
                    }
                    let id = format!("link:{}", link.key());
                    if out.iter().any(|c| c.id == id) {
                        continue;
                    }
                    if let Some(c) = self.registry.get(&id) {
                        out.push(c.clone());
                    }
                }
            }
        }

        if out.len() > limit {
            out.truncate(limit);
        }
        out
    }

    pub fn scripts_dir(&self) -> PathBuf {
        self.config.scripts_dir(&self.config_dir)
    }

    pub fn script_timeout(&self) -> Duration {
        Duration::from_secs(self.config.general.script_timeout_secs.max(1))
    }

    /// Resolve `[[hotkeys]]` entries to command ids. `command` may be a
    /// command id or a command title (case-insensitive). Unresolved entries
    /// become warnings, not errors.
    pub fn hotkey_bindings(&self) -> (Vec<(String, String)>, Vec<String>) {
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        for h in &self.config.hotkeys {
            let wanted = h.command.trim();
            let id = if self.registry.get(wanted).is_some() {
                Some(wanted.to_string())
            } else {
                let lower = wanted.to_lowercase();
                self.registry
                    .iter()
                    .find(|c| c.title.to_lowercase() == lower)
                    .map(|c| c.id.clone())
            };
            match id {
                Some(id) => out.push((h.key.trim().to_string(), id)),
                None => warnings.push(format!(
                    "hotkey `{}`: no command named `{}` (use a command title or id)",
                    h.key, h.command
                )),
            }
        }
        (out, warnings)
    }

    /// Decide what running `id` with `args` means.
    pub fn plan(&self, id: &str, args: &[String]) -> Result<Plan> {
        if let Some(value) = id.strip_prefix("calc:") {
            return Ok(Plan::Copy(value.to_string()));
        }
        if let Some(key) = id.strip_prefix("builtin:") {
            return BUILTINS
                .iter()
                .find(|(_, k, ..)| *k == key)
                .map(|(b, ..)| match b {
                    Builtin::ThemeSystem => Plan::SetTheme("system"),
                    Builtin::ThemeLight => Plan::SetTheme("light"),
                    Builtin::ThemeDark => Plan::SetTheme("dark"),
                    Builtin::PresentationIsland => Plan::SetPresentation("island"),
                    Builtin::PresentationSpotlight => Plan::SetPresentation("spotlight"),
                    Builtin::StayAwakeOff => Plan::StayAwake("off"),
                    Builtin::StayAwakeIndefinite => Plan::StayAwake("indefinite"),
                    Builtin::StayAwake30m => Plan::StayAwake("30m"),
                    Builtin::StayAwake1h => Plan::StayAwake("1h"),
                    Builtin::StayAwake3h => Plan::StayAwake("3h"),
                    Builtin::Settings => Plan::FillQuery("settings".into()),
                    Builtin::Shortcuts => Plan::FillQuery("shortcut".into()),
                    Builtin::ClipboardHistory => Plan::FillQuery("clip".into()),
                    Builtin::SearchScreenshots => Plan::FillQuery("shots".into()),
                    // Swift intercepts `builtin:paste-latest-screenshot` before execute.
                    Builtin::PasteLatestScreenshot => Plan::Hint(
                        "Paste Latest Screenshot runs from the launcher".into(),
                    ),
                    Builtin::EmojiPicker => Plan::FillQuery("emoji".into()),
                    Builtin::Notes => Plan::FillQuery("note".into()),
                    Builtin::NewNote => {
                        let title = args.first().cloned().unwrap_or_default();
                        let body = args.get(1).cloned().unwrap_or_default();
                        let (title, body) = if body.trim().is_empty() {
                            (String::new(), title)
                        } else {
                            (title, body)
                        };
                        Plan::SaveNote { title, body }
                    }
                    Builtin::ToggleDarkMode => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode'",
                    ),
                    Builtin::VolumeUp => Plan::SystemShell(
                        "osascript -e 'set volume output volume (output volume of (get volume settings) + 10)'",
                    ),
                    Builtin::VolumeDown => Plan::SystemShell(
                        "osascript -e 'set volume output volume (output volume of (get volume settings) - 10)'",
                    ),
                    Builtin::VolumeMute => Plan::SystemShell(
                        "osascript -e 'set volume output muted not (output muted of (get volume settings))'",
                    ),
                    // Accessibility key codes (144/145) — needs Accessibility permission.
                    Builtin::BrightnessUp => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to key code 145'",
                    ),
                    Builtin::BrightnessDown => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to key code 144'",
                    ),
                    Builtin::EmptyTrash => Plan::SystemShell(
                        "osascript -e 'tell application \"Finder\" to empty the trash'",
                    ),
                    Builtin::LockScreen => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to keystroke \"q\" using {command down, control down}'",
                    ),
                    Builtin::Sleep => {
                        Plan::SystemShell("osascript -e 'tell application \"System Events\" to sleep'")
                    }
                    Builtin::EjectAll => Plan::SystemShell(
                        "osascript -e 'tell application \"Finder\" to eject (every disk whose ejectable is true)'",
                    ),
                    Builtin::QuitFrontApp => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to keystroke \"q\" using command down'",
                    ),
                    Builtin::KillFrontApp => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to set pid to unix id of first process whose frontmost is true' -e 'do shell script \"kill -9 \" & pid'",
                    ),
                    other => Plan::Builtin(*other),
                })
                .ok_or_else(|| anyhow::anyhow!("unknown builtin `{key}`"));
        }
        if let Some(path) = id.strip_prefix("file:") {
            return Ok(Plan::OpenUrl(path.to_string()));
        }
        if let Some(glyph) = crate::emoji::glyph(id) {
            return Ok(Plan::PasteText(glyph.to_string()));
        }
        if id.starts_with("note:") {
            if let Some(n) = crate::notes::get(&self.config_dir, id) {
                return Ok(Plan::PasteText(n.body));
            }
            bail!("note not found");
        }
        if let Some(app_id) = id.strip_prefix("quit:") {
            if let Some(path) = self.apps.get(app_id) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                return Ok(Plan::QuitApp(name));
            }
            bail!("unknown app to quit");
        }
        if let Some(text) = self.snippets.get(id) {
            return Ok(Plan::PasteText(text.clone()));
        }
        match id {
            "shortcut:return" => {
                return Ok(Plan::Hint("Press Return on a result to run it".into()));
            }
            "shortcut:tab" => return Ok(Plan::AskAi),
            "shortcut:backtick" => return Ok(Plan::Seek),
            "shortcut:esc" => return Ok(Plan::DoEscape),
            "shortcut:arrows" => {
                return Ok(Plan::Hint("Use ↑ and ↓ to move through the list".into()));
            }
            _ => {}
        }
        if let Some(key) = id.strip_prefix("window:") {
            return WINDOW_ACTIONS
                .iter()
                .find(|(_, k, ..)| *k == key)
                .map(|(a, ..)| Plan::Window(*a))
                .ok_or_else(|| anyhow::anyhow!("unknown window action `{key}`"));
        }
        if id == "seek" {
            return Ok(Plan::Seek);
        }
        if id == "layout:remember" || id == "layout:forget" {
            let name = args.first().map(|s| s.trim()).unwrap_or("");
            if name.is_empty() {
                bail!("give the layout a name");
            }
            return Ok(Plan::Layout(if id == "layout:remember" {
                LayoutOp::Remember(name.to_string())
            } else {
                LayoutOp::Forget(name.to_string())
            }));
        }
        if let Some(slug) = self.restore_ids.get(id) {
            return Ok(Plan::Layout(LayoutOp::Restore(slug.clone())));
        }
        if let Some(path) = self.apps.get(id) {
            return Ok(Plan::LaunchApp(path.clone()));
        }
        if let Some(meta) = self.scripts.get(id) {
            for (i, a) in meta.arguments.iter().enumerate() {
                if !a.optional && args.get(i).is_none_or(|s| s.trim().is_empty()) {
                    bail!("`{}` needs: {}", meta.title, a.placeholder);
                }
            }
            return Ok(Plan::RunScript {
                meta: meta.clone(),
                args: args.to_vec(),
            });
        }
        if let Some(link) = self.links.get(id) {
            let q = args.first().map(String::as_str).unwrap_or("");
            if link.url.contains("{query}") && q.trim().is_empty() {
                bail!("`{}` needs a query", link.title);
            }
            return Ok(Plan::OpenUrl(link.url.replace("{query}", &url_encode(q))));
        }
        if let Some(c) = self.ai.get(id) {
            let input = args.first().map(String::as_str).unwrap_or("");
            let needs_selection = c.prompt.contains("{selection}");
            if c.prompt.contains("{input}") && !needs_selection && input.trim().is_empty() {
                bail!("`{}` needs some input", c.title);
            }
            return Ok(Plan::Chat {
                prompt: Some(c.prompt.replace("{input}", input)),
                system: c.system.clone(),
                provider: c.provider.clone(),
                model: c.model.clone(),
                needs_selection,
            });
        }
        bail!("command not found — try Reload Commands")
    }
}

/// Insert the selected text into an AI command prompt.
pub fn fill_selection(prompt: &str, selection: &str) -> String {
    prompt.replace("{selection}", selection)
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?']).next())
        .unwrap_or(url)
        .to_string()
}

/// Percent-encode for a query-string value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_CONFIG;

    fn engine_with(layouts: Vec<(String, String, usize)>, extra_toml: &str) -> Engine {
        let cfg = Config::parse(&format!("{DEFAULT_CONFIG}\n{extra_toml}")).unwrap();
        let apps = vec![AppEntry {
            name: "Safari".into(),
            path: "/Applications/Safari.app".into(),
        }];
        let script = scripts::parse_header(
            Path::new("/s/uuid.sh"),
            include_str!("../../../examples/scripts/uuid.sh"),
        )
        .unwrap();
        Engine::assemble(cfg, "/cfg".into(), apps, vec![script], layouts, vec![])
    }

    fn engine() -> Engine {
        engine_with(vec![], "")
    }

    #[test]
    fn plans_each_kind() {
        let e = engine();
        assert_eq!(e.plan("calc:42", &[]).unwrap(), Plan::Copy("42".into()));
        assert_eq!(e.plan("builtin:chat", &[]).unwrap(), Plan::Builtin(Builtin::Chat));
        assert_eq!(
            e.plan("app:/Applications/Safari.app", &[]).unwrap(),
            Plan::LaunchApp("/Applications/Safari.app".into())
        );
        assert!(matches!(
            e.plan("script:/s/uuid.sh", &[]).unwrap(),
            Plan::RunScript { .. }
        ));
        assert_eq!(
            e.plan("link:search-github", &["rust & tauri".into()]).unwrap(),
            Plan::OpenUrl("https://github.com/search?q=rust+%26+tauri".into())
        );
        assert!(e.plan("link:search-github", &[]).is_err());
        assert!(e.plan("nope", &[]).is_err());
    }

    #[test]
    fn plans_window_and_layout_commands() {
        let e = engine_with(vec![("work".into(), "Work".into(), 3)], "");
        assert_eq!(
            e.plan("window:left-half", &[]).unwrap(),
            Plan::Window(WindowAction::LeftHalf)
        );
        assert!(e.plan("window:diagonal", &[]).is_err());
        assert_eq!(e.plan("seek", &[]).unwrap(), Plan::Seek);
        assert_eq!(
            e.plan("layout:remember", &[" Focus ".into()]).unwrap(),
            Plan::Layout(LayoutOp::Remember("Focus".into()))
        );
        assert!(e.plan("layout:remember", &[]).is_err());
        assert_eq!(
            e.plan("layout:restore:work", &[]).unwrap(),
            Plan::Layout(LayoutOp::Restore("work".into()))
        );
        assert_eq!(
            e.plan("layout:forget", &["work".into()]).unwrap(),
            Plan::Layout(LayoutOp::Forget("work".into()))
        );
        // Without saved layouts there is no restore command and no forget.
        let bare = engine();
        assert!(bare.plan("layout:restore:work", &[]).is_err());
        assert!(bare.registry.get("layout:forget").is_none());
        assert!(bare.registry.get("layout:remember").is_some());
    }

    #[test]
    fn window_fractions() {
        assert_eq!(WindowAction::LeftHalf.frac(), Some((0.0, 0.0, 0.5, 1.0)));
        assert_eq!(WindowAction::BottomRight.frac(), Some((0.5, 0.5, 0.5, 0.5)));
        assert_eq!(WindowAction::Center.frac(), None);
        let (x, _, w, _) = WindowAction::LastThird.frac().unwrap();
        assert!((x + w - 1.0).abs() < 1e-9);
        assert_eq!(WindowAction::Maximize.key(), "maximize");
        assert_eq!(WindowAction::TopLeft.title(), "Top Left Quarter");
    }

    #[test]
    fn hotkeys_resolve_titles_and_ids() {
        let e = engine_with(
            vec![],
            r#"
            [[hotkeys]]
            key = "Ctrl+Alt+Left"
            command = "left half"
            [[hotkeys]]
            key = "Cmd+Alt+S"
            command = "Safari"
            [[hotkeys]]
            key = "Cmd+Alt+G"
            command = "ai:fix-grammar"
            [[hotkeys]]
            key = "Cmd+Alt+Z"
            command = "No Such Thing"
            "#,
        );
        let (bindings, warnings) = e.hotkey_bindings();
        assert_eq!(
            bindings,
            vec![
                ("Ctrl+Alt+Left".to_string(), "window:left-half".to_string()),
                ("Cmd+Alt+S".to_string(), "app:/Applications/Safari.app".to_string()),
                ("Cmd+Alt+G".to_string(), "ai:fix-grammar".to_string()),
            ]
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("No Such Thing"));
    }

    #[test]
    fn ai_commands_fill_input_and_flag_selection() {
        let e = engine();
        match e.plan("ai:fix-grammar", &[]).unwrap() {
            Plan::Chat {
                prompt,
                needs_selection,
                system,
                ..
            } => {
                assert!(needs_selection);
                assert!(system.unwrap().contains("copy editor"));
                let filled = fill_selection(&prompt.unwrap(), "teh cat");
                assert!(filled.ends_with("teh cat"));
            }
            p => panic!("unexpected {p:?}"),
        }
        match e.plan("ai:shell", &["list big files".into()]).unwrap() {
            Plan::Chat {
                prompt,
                needs_selection,
                ..
            } => {
                assert!(!needs_selection);
                assert_eq!(prompt.as_deref(), Some("list big files"));
            }
            p => panic!("unexpected {p:?}"),
        }
        assert!(e.plan("ai:shell", &[]).is_err());
        let shell = e.registry.get("ai:shell").unwrap();
        assert_eq!(shell.arguments.len(), 1);
        assert_eq!(shell.subtitle.as_deref(), Some("AI · ollama"));
    }

    #[test]
    fn search_reaches_every_kind() {
        let e = engine_with(vec![("work".into(), "Work".into(), 2)], "");
        assert_eq!(e.search("saf", 5)[0].kind, CommandKind::App);
        assert_eq!(e.search("uuid", 5)[0].kind, CommandKind::Script);
        assert_eq!(e.search("grammar", 5)[0].kind, CommandKind::Ai);
        assert_eq!(e.search("github", 5)[0].kind, CommandKind::Quicklink);
        assert_eq!(e.search("reload", 5)[0].kind, CommandKind::Builtin);
        assert_eq!(e.search("left half", 5)[0].id, "window:left-half");
        assert_eq!(e.search("restore layout", 5)[0].id, "layout:restore:work");
        assert_eq!(e.search("remember", 5)[0].id, "layout:remember");
    }
}
