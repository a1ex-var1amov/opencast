//! Engine: config + registry + providers, and the decision of what a
//! command does. The shell only carries out the returned [`Plan`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::apps::{self, AppEntry};
use crate::commands::{Action, Argument, Command, CommandKind, Override, Registry};
use crate::config::{AiCommandConfig, AiOutput, Config, QuicklinkConfig, ThemeFile};
use crate::keymap::KeyBinding;
use crate::layouts;
use crate::notes::Note;
use crate::providers::Router;
use crate::scripts::{self, ScriptMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Chat,
    Reload,
    OpenConfig,
    OpenScripts,
    Quit,
    Appearance,
    PanelPosition,
    Settings,
    Shortcuts,
    StayAwake,
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
    /// The panel searches the front app's menu bar (Swift, AX).
    MenuItems,
    ChatHistory,
    /// Start on-device dictation into the search field (Swift).
    Dictate,
    /// Today's calendar (Swift, EventKit).
    MySchedule,
    SyncICloud,
    CheckUpdates,
    RefreshMcp,
    InstallExtension,
    UpdateExtensions,
    ManageExtensions,
    BrowseExtensions,
    ImportAlfredWorkflow,
    Doctor,
    /// Panel-side (Swift): Contacts, Reminders, dictionary, Large Type, media keys…
    Contacts,
    Reminders,
    NewReminder,
    DefineWord,
    LargeType,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    Spotify,
    SpotifyPlay,
    SpotifyNext,
    SpotifyPrevious,
    SpotifyNowPlaying,
    SpotifySearch,
    ActOnSelection,
    StartFocus,
    StopFocus,
    PauseFocus,
    SearchBookmarks,
    SearchHistory,
    EditNote,
    Restart,
    ShutDown,
    LogOut,
    ToggleHiddenFiles,
    QuitAllApps,
    HideOthers,
    ShowDesktop,
    Spell,
    ArrangeWindow,
    Translate,
    AskPage,
    SummarizePage,
    CompareModels,
    DictateAnywhere,
    CheatSheet,
    StickyNote,
    SaveSnippet,
    Stats,
    EditProfile,
    BrowseFiles,
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
        Builtin::Appearance,
        "appearance",
        "Appearance",
        "🖥️",
        &["theme", "appearance", "dark", "light", "color", "system", "settings"],
    ),
    (
        Builtin::PanelPosition,
        "panel-position",
        "Panel Position",
        "🏝️",
        &[
            "presentation",
            "position",
            "island",
            "spotlight",
            "layout",
            "panel",
            "centered",
            "settings",
        ],
    ),
    (
        Builtin::Settings,
        "settings",
        "Settings",
        "⚙️",
        &[
            "config",
            "preferences",
            "theme",
            "appearance",
            "presentation",
            "position",
            "awake",
        ],
    ),
    (
        Builtin::Shortcuts,
        "shortcuts",
        "OpenCast Keys",
        "⌨️",
        &["hotkey", "keys", "keymap", "tab", "click", "shortcut", "opencast"],
    ),
    (
        Builtin::StayAwake,
        "stay-awake",
        "Stay Awake",
        "☕️",
        &[
            "awake",
            "caffeine",
            "sleep",
            "amphetamine",
            "keepingyouawake",
            "indefinite",
            "30",
            "60",
        ],
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
    (Builtin::Notes, "notes", "Notes", "📝", &["note", "notes", "memo"]),
    (
        Builtin::NewNote,
        "new-note",
        "New Note",
        "📝",
        &["note", "notes", "memo", "write"],
    ),
    (
        Builtin::MenuItems,
        "menu-items",
        "Search Menu Items",
        "☰",
        &["menu", "menus", "menubar", "command palette"],
    ),
    (
        Builtin::ChatHistory,
        "chat-history",
        "Chat History",
        "🗂",
        &["chats", "history", "ai", "conversations"],
    ),
    (
        Builtin::Dictate,
        "dictate",
        "Dictate",
        "🎙",
        &["dictation", "voice", "speak", "microphone", "speech"],
    ),
    (
        Builtin::MySchedule,
        "schedule",
        "My Schedule",
        "📅",
        &["calendar", "events", "meetings", "today", "agenda", "join"],
    ),
    (
        Builtin::SyncICloud,
        "sync-icloud",
        "Sync Config with iCloud Drive",
        "☁️",
        &["sync", "icloud", "backup", "settings", "macs"],
    ),
    (
        Builtin::CheckUpdates,
        "check-updates",
        "Check for Updates",
        "⬆️",
        &["update", "upgrade", "version", "release"],
    ),
    (
        Builtin::RefreshMcp,
        "mcp-refresh",
        "Refresh MCP Tools",
        "🔌",
        &["mcp", "tools", "servers", "reload"],
    ),
    (
        Builtin::InstallExtension,
        "install-extension",
        "Install Extension",
        "🧩",
        &[
            "extension",
            "plugin",
            "add",
            "github",
            "git",
            "script commands",
            "store",
        ],
    ),
    (
        Builtin::UpdateExtensions,
        "update-extensions",
        "Update Extensions",
        "🧩",
        &["extension", "plugin", "upgrade", "pull"],
    ),
    (
        Builtin::ManageExtensions,
        "manage-extensions",
        "Manage Extensions",
        "🧩",
        &["extension", "plugin", "remove", "uninstall", "installed"],
    ),
    (
        Builtin::BrowseExtensions,
        "browse-extensions",
        "Browse Extensions",
        "🧩",
        &["extension", "plugin", "store", "discover", "raycast", "script commands"],
    ),
    (
        Builtin::ImportAlfredWorkflow,
        "import-alfred",
        "Import Alfred Workflow or Snippets",
        "🎩",
        &[
            "alfred",
            "workflow",
            "alfredworkflow",
            "alfredsnippets",
            "snippets",
            "extension",
            "import",
        ],
    ),
    (
        Builtin::Doctor,
        "doctor",
        "OpenCast Doctor",
        "🩺",
        &[
            "health",
            "diagnose",
            "check",
            "slow",
            "problem",
            "debug",
            "troubleshoot",
        ],
    ),
    (
        Builtin::Contacts,
        "contacts",
        "Search Contacts",
        "👤",
        &["people", "phone", "email", "address book", "call", "message"],
    ),
    (
        Builtin::Reminders,
        "reminders",
        "Reminders",
        "☑️",
        &["todo", "tasks", "to-do", "due"],
    ),
    (
        Builtin::NewReminder,
        "new-reminder",
        "New Reminder",
        "☑️",
        &["todo", "task", "remind me", "add"],
    ),
    (
        Builtin::DefineWord,
        "define",
        "Define Word",
        "📖",
        &["dictionary", "meaning", "definition", "thesaurus"],
    ),
    (
        Builtin::LargeType,
        "large-type",
        "Large Type",
        "🔠",
        &["big text", "show", "display", "fullscreen"],
    ),
    (
        Builtin::MediaPlayPause,
        "media-play",
        "Play / Pause",
        "⏯",
        // No "spotify": that query belongs to the Spotify integration family.
        &["music", "pause", "resume", "media"],
    ),
    (
        Builtin::MediaNext,
        "media-next",
        "Next Track",
        "⏭",
        &["music", "skip", "song", "media"],
    ),
    (
        Builtin::MediaPrevious,
        "media-previous",
        "Previous Track",
        "⏮",
        &["music", "back", "song", "media"],
    ),
    // The Spotify family talks to Spotify itself: media keys follow whatever owns
    // Now Playing (a browser tab included) and cannot read anything back.
    (
        Builtin::Spotify,
        "spotify",
        "Spotify",
        "🎧",
        &[
            "music",
            "player",
            "track",
            "album",
            "playlist",
            "volume",
            "shuffle",
            "now playing",
        ],
    ),
    (
        Builtin::SpotifyPlay,
        "spotify-play",
        "Play / Pause",
        "🎧",
        // Phrase keywords: fuzzy scores the whole query against one field.
        &[
            "music",
            "pause",
            "resume",
            "player",
            "spotify",
            "spotify play",
            "spotify pause",
        ],
    ),
    (
        Builtin::SpotifyNext,
        "spotify-next",
        "Next Track",
        "🎧",
        &["music", "skip", "song", "player", "spotify", "spotify next"],
    ),
    (
        Builtin::SpotifyPrevious,
        "spotify-previous",
        "Previous Track",
        "🎧",
        &[
            "music",
            "back",
            "song",
            "player",
            "spotify",
            "spotify previous",
            "spotify back",
        ],
    ),
    (
        Builtin::SpotifyNowPlaying,
        "spotify-now",
        "Now Playing",
        "🎧",
        &[
            "music",
            "track",
            "song",
            "album",
            "what is playing",
            "now playing",
            "spotify now playing",
            "player",
            "spotify",
        ],
    ),
    // Spotify's scripting dictionary has no search command, so this one hands the
    // query to the app over its deep link — no API credentials needed.
    (
        Builtin::SpotifySearch,
        "spotify-search",
        "Search",
        "🎧",
        &[
            "music",
            "find",
            "artist",
            "album",
            "playlist",
            "query",
            "spotify",
            "spotify search",
        ],
    ),
    (
        Builtin::ActOnSelection,
        "selection",
        "Act on Selection",
        "✳️",
        &["selected text", "universal actions", "selection", "files"],
    ),
    (
        Builtin::StartFocus,
        "focus-start",
        "Start Focus",
        "🎯",
        &["focus", "pomodoro", "deep work", "block", "distraction"],
    ),
    (
        Builtin::PauseFocus,
        "focus-pause",
        "Pause / Resume Focus",
        "⏸",
        &["focus", "pause", "resume", "break"],
    ),
    (
        Builtin::StopFocus,
        "focus-stop",
        "Stop Focus",
        "🎯",
        &["focus", "end", "unblock"],
    ),
    (
        Builtin::SearchBookmarks,
        "bookmarks",
        "Search Bookmarks",
        "🔖",
        &["bm", "browser", "safari", "chrome", "arc", "favorites"],
    ),
    (
        Builtin::SearchHistory,
        "history",
        "Search History",
        "🕘",
        &["hist", "browser history", "visited", "chrome", "safari"],
    ),
    (
        Builtin::EditNote,
        "edit-note",
        "Notes Editor",
        "📝",
        &["note", "notes", "write", "editor", "scratchpad"],
    ),
    (Builtin::Restart, "restart", "Restart", "🔁", &["reboot", "system"]),
    (
        Builtin::ShutDown,
        "shut-down",
        "Shut Down",
        "⏻",
        &["shutdown", "power off", "turn off", "system"],
    ),
    (
        Builtin::LogOut,
        "log-out",
        "Log Out",
        "🚪",
        &["logout", "sign out", "system"],
    ),
    (
        Builtin::ToggleHiddenFiles,
        "hidden-files",
        "Toggle Hidden Files",
        "👁",
        &["finder", "dotfiles", "show hidden", "system"],
    ),
    (
        Builtin::QuitAllApps,
        "quit-all",
        "Quit All Apps",
        "✕",
        &["close all", "quit everything", "system"],
    ),
    (
        Builtin::HideOthers,
        "hide-others",
        "Hide All Apps Except Front",
        "🙈",
        &["hide others", "focus", "system"],
    ),
    (
        Builtin::ShowDesktop,
        "show-desktop",
        "Show Desktop",
        "🖥",
        &["desktop", "clear", "system"],
    ),
    (
        Builtin::ArrangeWindow,
        "arrange-window",
        "Arrange Window",
        "🪟",
        &["window", "snap", "tile", "layout", "resize", "move"],
    ),
    (
        Builtin::Translate,
        "translate",
        "Translate",
        "🌐",
        &["translate", "translation", "language", "tr"],
    ),
    (
        Builtin::AskPage,
        "ask-page",
        "Ask About This Page",
        "🧭",
        &[
            "browser", "page", "web", "tab", "safari", "chrome", "arc", "ai", "context",
        ],
    ),
    (
        Builtin::SummarizePage,
        "summarize-page",
        "Summarize This Page",
        "📰",
        &["browser", "page", "web", "tab", "tldr", "summary", "ai"],
    ),
    (
        Builtin::CompareModels,
        "compare-models",
        "Compare Models",
        "⚖️",
        &["ai", "llm", "side by side", "benchmark", "models", "providers"],
    ),
    (
        Builtin::DictateAnywhere,
        "dictate-anywhere",
        "Dictate Anywhere",
        "🎙",
        &["dictation", "speech", "voice", "talk", "transcribe", "speak"],
    ),
    (
        Builtin::CheatSheet,
        "cheat-sheet",
        "App Menu Shortcuts",
        "⌨️",
        &[
            "cheat sheet",
            "cheatsheet",
            "shortcuts",
            "keyboard",
            "hotkeys",
            "menu",
            "keys",
            "app menu",
        ],
    ),
    (
        Builtin::StickyNote,
        "sticky-note",
        "New Sticky Note",
        "🗒",
        &["note", "sticky", "floating", "scratch", "post-it"],
    ),
    (
        Builtin::Spell,
        "spell",
        "Spell Word",
        "🔤",
        &["spelling", "spellcheck", "how to spell"],
    ),
    (
        Builtin::SaveSnippet,
        "save-snippet",
        "Save as Snippet",
        "📎",
        &["snippet", "new snippet", "text expansion"],
    ),
    (
        Builtin::Stats,
        "stats",
        "OpenCast Stats",
        "📊",
        &["usage", "statistics", "most used"],
    ),
    (
        Builtin::EditProfile,
        "ai-profile",
        "Edit AI Profile",
        "🪪",
        &["profile", "memory", "about me", "ai", "remember"],
    ),
    (
        Builtin::BrowseFiles,
        "browse-files",
        "Browse Files",
        "📂",
        &["finder", "folders", "path", "navigate", "files"],
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
    (Builtin::Sleep, "sleep", "Sleep", "😴", &["sleep", "suspend", "system"]),
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

/// In-app key rows. Searchable via OpenCast Keys (`query` = `shortcut`).
const SHORTCUT_ROWS: &[(&str, &str, &str, &[&str])] = &[
    (
        "shortcut:return",
        "Return — Run selected",
        "↩",
        &["return", "enter", "shortcut"],
    ),
    ("shortcut:tab", "Tab — Ask AI", "⇥", &["tab", "ask", "shortcut"]),
    (
        "shortcut:backtick",
        "Backtick — Click Text",
        "`",
        &["`", "click", "seek", "shortcut"],
    ),
    ("shortcut:esc", "Esc — Back / hide", "⎋", &["escape", "esc", "shortcut"]),
    (
        "shortcut:arrows",
        "Arrows — Move selection",
        "↑↓",
        &["up", "down", "arrows", "shortcut"],
    ),
];

// Window geometry actions live in `window`; re-export so Plan / FFI keep
// compiling via `engine::WindowAction`.
pub use crate::window::{WindowAction, WINDOW_ACTIONS};

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
    /// app's selected text and calls [`fill_placeholders`] on the prompt first.
    Chat {
        prompt: Option<String>,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        needs_selection: bool,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        output: AiOutput,
        /// `[[ai_presets]]` id: tool filter and profile applied when streaming.
        preset: Option<String>,
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
    /// Show text in the panel (Return there copies it).
    ShowText {
        title: String,
        text: String,
    },
    /// Persist a quick note and toast.
    SaveNote {
        title: String,
        body: String,
    },
    /// Run text through several steps (`[[pipelines]]`).
    Pipeline {
        title: String,
        input: crate::pipeline::Input,
        /// The typed argument (input = "query").
        query: Option<String>,
        steps: Vec<crate::pipeline::Step>,
        output: crate::pipeline::Output,
    },
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
    /// Run a Shortcuts.app shortcut, optionally with text input.
    RunShortcut {
        name: String,
        input: Option<String>,
    },
    /// Call an MCP tool and show its output.
    McpCall {
        server: String,
        tool: String,
        arguments: serde_json::Value,
    },
    /// Read an MCP resource (a document a server exposes) and show its text.
    McpReadResource {
        server: String,
        uri: String,
    },
    /// Ask an MCP server to fill in one of its own prompt templates; the panel
    /// opens a chat with the result, editable before it is sent.
    McpGetPrompt {
        server: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Reopen a saved chat.
    ResumeChat(Box<crate::chats::SavedChat>),
    /// Panel-side feature (menu search, dictation, schedule, settings window…).
    Ui(&'static str),
    /// Extensions: install / update / manage / browse.
    Extensions(ExtensionOp),
    /// Import an Alfred workflow file or folder.
    ImportAlfred(String),
    /// Health report for the config folder.
    Doctor,
    /// Window command the panel resolves through `window::target` (custom, display-N).
    WindowKey(String),
    /// Append a `[[snippets]]` entry and reload.
    AddSnippet {
        keyword: String,
        text: String,
    },
    /// Panel-side feature that takes input (define, large type, new reminder…).
    UiArgs {
        action: &'static str,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionOp {
    Install(String),
    Update,
    /// List installed ones (`run` step: `open:`/`remove:`/`update` arg).
    Manage,
    /// Curated + GitHub listings (`run` step installs the arg).
    Browse,
}

impl PartialEq for ScriptMeta {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

/// Command sources the platform shell discovers (and caches) on its own.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    /// Shortcuts.app shortcut names.
    pub shortcuts: Vec<String>,
    /// MCP tools by server (from `mcp-tools.json`).
    pub mcp_tools: crate::mcp::ToolCache,
    /// MCP resources and prompts by server (from `mcp-extras.json`).
    pub mcp_extras: crate::mcp::ExtrasCache,
    /// Browser bookmarks (`bookmarks.json`).
    pub bookmarks: Vec<crate::bookmarks::Bookmark>,
    /// Browser history (`history.json`).
    pub history: Vec<crate::history::Visit>,
    /// Recent documents (`recents.json`).
    pub recents: Vec<crate::recents::Visit>,
    /// Exchange rates (`rates.json`).
    pub rates: Option<crate::currency::Rates>,
}

impl Extras {
    /// Read the caches the shell wrote last time.
    pub fn load_cached(config_dir: &Path) -> Self {
        let shortcuts = std::fs::read_to_string(config_dir.join("shortcuts.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default();
        Self {
            shortcuts,
            mcp_tools: crate::mcp::load_cache(config_dir),
            mcp_extras: crate::mcp::load_extras(config_dir),
            bookmarks: crate::bookmarks::load_cache(config_dir),
            history: crate::history::load_cache(config_dir),
            recents: crate::recents::load_cache(config_dir),
            rates: crate::currency::load(config_dir),
        }
    }
}

/// What the preview pane shows for a row (text or a file).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Preview {
    pub title: String,
    /// Markdown-ish text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A file the panel can render (image, text, Quick Look).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Small key/value facts shown under the body.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<(String, String)>,
}

/// An MCP tool offered to the AI chat.
#[derive(Debug, Clone)]
pub struct ChatTool {
    pub spec: crate::providers::ToolSpec,
    pub server: String,
    pub tool: String,
    /// The AI may call it without asking.
    pub auto_approve: bool,
}

/// Small per-user state files, held in memory so search never touches disk.
#[derive(Default)]
struct Sidecars {
    chats: Vec<crate::chats::SavedChat>,
    usage: crate::usage::Usage,
    /// Newest first (the on-disk order).
    pins: Vec<String>,
    pin_set: HashSet<String>,
    notes: Vec<Note>,
}

impl Sidecars {
    fn load(dir: &Path) -> Self {
        let pins = crate::pins::load_list(dir);
        Self {
            usage: crate::usage::Usage::load(dir),
            pin_set: pins.iter().cloned().collect(),
            pins,
            notes: crate::notes::load_all(dir),
            chats: crate::chats::load_all(dir),
        }
    }
}

pub struct Engine {
    pub config: Config,
    pub config_dir: PathBuf,
    pub registry: Registry,
    pub router: Router,
    pub warnings: Vec<String>,
    /// Parsed `theme_file`, when one is configured and valid.
    pub theme: Option<ThemeFile>,
    pub keymap: Vec<KeyBinding>,
    /// `app:<path>` id → path, sorted by name for stable listings.
    apps: Vec<(String, PathBuf, String)>,
    app_index: HashMap<String, usize>,
    scripts: HashMap<String, ScriptMeta>,
    ai: HashMap<String, AiCommandConfig>,
    links: HashMap<String, QuicklinkConfig>,
    snippets: HashMap<String, String>,
    /// `layout:restore:<slug>` command id → layout slug.
    restore_ids: HashMap<String, String>,
    /// `mcp:<server>:<tool>` → tool.
    mcp_tools: HashMap<String, crate::mcp::McpTool>,
    /// `mcp:<server>:res:<uri>` → resource a server exposes.
    mcp_resources: HashMap<String, crate::mcp::McpResource>,
    /// `mcp:<server>:prompt:<name>` → prompt template a server exposes.
    mcp_prompts: HashMap<String, crate::mcp::McpPrompt>,
    extras: Extras,
    /// Which browser bookmark stores exist here, and which OpenCast may not read.
    /// Lets the bookmarks command explain an empty result instead of showing none.
    pub bookmark_scan: crate::bookmarks::Scan,
    /// Same idea for browser history stores.
    pub history_scan: crate::history::Scan,
    /// Integrations switched on (`spotify`) — Doctor reports these.
    pub integrations_on: Vec<&'static str>,
    /// Command id → ⌘-menu actions an integration adds to it (app rows).
    app_actions: HashMap<String, Vec<Action>>,
    sidecars: RwLock<Sidecars>,
    /// This Mac's time zone (`America/New_York`).
    local_tz: String,
    /// Parsed tz files, loaded on first use (search never re-reads them).
    zones: std::sync::Mutex<HashMap<String, Option<std::sync::Arc<crate::tz::Zone>>>>,
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
        let (mut scripts, script_warnings) = scripts::discover(&config.scripts_dir(config_dir));
        warnings.extend(script_warnings);
        let (ext_scripts, ext_warnings) = crate::extensions::discover_all(config_dir);
        scripts.extend(ext_scripts);
        warnings.extend(ext_warnings);
        let saved_layouts = layouts::list(config_dir);
        let extras = Extras::load_cached(config_dir);
        Self::assemble_with(
            config,
            config_dir.to_path_buf(),
            apps,
            scripts,
            saved_layouts,
            warnings,
            extras,
        )
    }

    /// Build an engine from already-gathered parts (used by tests).
    pub fn assemble(
        config: Config,
        config_dir: PathBuf,
        app_list: Vec<AppEntry>,
        script_list: Vec<ScriptMeta>,
        saved_layouts: Vec<(String, String, usize)>,
        warnings: Vec<String>,
    ) -> Self {
        Self::assemble_with(
            config,
            config_dir,
            app_list,
            script_list,
            saved_layouts,
            warnings,
            Extras::default(),
        )
    }

    pub fn assemble_with(
        mut config: Config,
        config_dir: PathBuf,
        app_list: Vec<AppEntry>,
        script_list: Vec<ScriptMeta>,
        saved_layouts: Vec<(String, String, usize)>,
        mut warnings: Vec<String>,
        extras: Extras,
    ) -> Self {
        // `@opencast.hotkey` in a script header (e.g. an imported Alfred hotkey)
        // acts like a `[[hotkeys]]` entry unless you bound that command yourself.
        for s in &script_list {
            let Some(key) = s.hotkey.as_ref().filter(|k| !k.trim().is_empty()) else {
                continue;
            };
            let id = format!("script:{}", s.path.display());
            let mine = |c: &str| c == id || c.eq_ignore_ascii_case(&s.title);
            let bound = config.hotkeys.iter().any(|h| mine(&h.command))
                || config.commands.iter().any(|c| c.hotkey.is_some() && mine(&c.match_));
            if !bound {
                config.hotkeys.push(crate::config::HotkeyBinding {
                    key: key.trim().to_string(),
                    command: id,
                });
            }
        }
        let (router, router_warnings) = Router::from_config(&config);
        warnings.extend(router_warnings);
        // A window-hotkey preset that doesn't exist is a typo, not "off".
        let preset = config.general.window_hotkeys.trim();
        if !preset.is_empty() && preset != "off" && crate::config::window_hotkey_preset(preset).is_none() {
            warnings.push(format!(
                "[general] window_hotkeys = \"{preset}\" is not a preset; use {} or \"off\"",
                crate::config::window_hotkey_preset_names()
            ));
        }
        // Two presets can want the same key; the one named first gets it, and the
        // other command ends up keyless. Say so.
        warnings.extend(config.all_hotkeys_and_clashes().1);
        let theme = match config.load_theme(&config_dir) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("{e:#}"));
                None
            }
        };
        let (keymap, key_warnings) = crate::keymap::bindings(&config.keys);
        warnings.extend(key_warnings);

        let mut commands = Vec::new();
        let mut apps: Vec<(String, PathBuf, String)> = Vec::new();
        let mut scripts = HashMap::new();
        let mut ai = HashMap::new();
        let mut links = HashMap::new();
        let mut restore_ids = HashMap::new();

        // Integrations are opt-out: `auto` turns one on when its app is installed,
        // and a switched-off integration drops its whole command family.
        let installed: HashSet<String> = app_list.iter().map(|a| a.name.to_lowercase()).collect();
        let (integrations_on, integration_warnings) = config.integrations_enabled(&installed);
        warnings.extend(integration_warnings);
        if config.spotify.wants_list() && !config.spotify.list_enabled() {
            warnings.push(
                "[spotify] search = \"list\" needs client_id and client_secret_cmd — using open until both are set"
                    .into(),
            );
        }

        for (_, key, title, icon, kw) in BUILTINS {
            if let Some(name) = Config::integration_for_key(key) {
                if !integrations_on.contains(&name) {
                    continue;
                }
            }
            // One glyph for the Spotify family; the subtitle carries the kind/verb home.
            let subtitle = match *key {
                "spotify" => Some("Player Controls".into()),
                k if k.starts_with("spotify") => Some("Spotify".into()),
                _ => None,
            };
            commands.push(Command {
                id: format!("builtin:{key}"),
                title: title.to_string(),
                subtitle,
                icon: Some(icon.to_string()),
                kind: CommandKind::Builtin,
                arguments: vec![],
                hotkey: None,
                keywords: kw.iter().map(|s| s.to_string()).collect(),
            });
        }
        for (id, args) in [
            (
                "builtin:install-extension",
                &[("owner/repo, owner/repo/folder or git URL", false)][..],
            ),
            (
                "builtin:import-alfred",
                &[("path to .alfredworkflow, .alfredsnippets or a workflow folder", false)],
            ),
            (
                "builtin:new-reminder",
                &[("Remind me to…", false), ("when (tomorrow 9am)", true)],
            ),
            ("builtin:define", &[("word", false)]),
            ("builtin:spotify-search", &[("artist, track, album or playlist", false)]),
            ("builtin:large-type", &[("text", false)]),
            ("builtin:focus-start", &[("minutes", true), ("goal (optional)", true)]),
            ("builtin:spell", &[("word", false)]),
            (
                "builtin:translate",
                &[("text (empty: the selection)", true), ("to language", true)],
            ),
            ("builtin:ask-page", &[("question (optional)", true)]),
            ("builtin:compare-models", &[("prompt (optional)", true)]),
            ("builtin:save-snippet", &[("keyword (;sig)", false), ("text", false)]),
        ] {
            if let Some(c) = commands.iter_mut().find(|c| c.id == id) {
                c.arguments = args
                    .iter()
                    .map(|(placeholder, optional)| Argument {
                        placeholder: (*placeholder).into(),
                        optional: *optional,
                        secure: false,
                    })
                    .collect();
            }
        }
        commands.extend(crate::panes::commands());
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

        for n in 1..=3 {
            commands.push(Command {
                id: format!("window:display-{n}"),
                title: format!("Move to Display {n}"),
                subtitle: None,
                icon: Some(format!("{n}")),
                kind: CommandKind::Window,
                arguments: vec![],
                hotkey: None,
                keywords: vec!["window".into(), "screen".into(), "monitor".into(), "display".into()],
            });
        }
        for c in &config.window_commands {
            commands.push(Command {
                id: format!("window:custom:{}", c.id),
                title: c.title.clone(),
                subtitle: Some("Window".into()),
                icon: c.icon.clone().or(Some("▣".into())),
                kind: CommandKind::Window,
                arguments: vec![],
                hotkey: None,
                keywords: vec!["window".into(), "snap".into(), "tile".into()],
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
            keywords: vec![
                "seek".into(),
                "click".into(),
                "ocr".into(),
                "mouse".into(),
                "superkey".into(),
            ],
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

        for p in &config.pipelines {
            let input = crate::pipeline::Input::parse(p.input.as_deref());
            commands.push(Command {
                id: format!("pipe:{}", p.id),
                title: p.title.clone(),
                subtitle: Some(
                    p.description
                        .clone()
                        .unwrap_or_else(|| format!("Pipeline · {}", crate::pipeline::describe(&p.steps))),
                ),
                icon: p.icon.clone().or(Some("⛓".into())),
                kind: CommandKind::Script,
                arguments: if input == crate::pipeline::Input::Query {
                    vec![Argument {
                        placeholder: "Input".into(),
                        optional: false,
                        secure: false,
                    }]
                } else {
                    vec![]
                },
                hotkey: None,
                keywords: p.keywords.iter().cloned().chain(["pipeline".into()]).collect(),
            });
        }

        for p in &config.ai_presets {
            let provider = p.provider.as_deref().or(config.default_provider_id());
            commands.push(Command {
                id: format!("preset:{}", p.id),
                title: p.title.clone(),
                subtitle: Some(match (provider, &p.model) {
                    (Some(pr), Some(m)) => format!("AI preset · {pr} · {m}"),
                    (Some(pr), None) => format!("AI preset · {pr}"),
                    _ => "AI preset".into(),
                }),
                icon: p.icon.clone().or(Some("🧑‍💻".into())),
                kind: CommandKind::Ai,
                arguments: vec![],
                hotkey: None,
                keywords: p
                    .keywords
                    .iter()
                    .cloned()
                    .chain(["chat".into(), "ask".into(), "preset".into()])
                    .collect(),
            });
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
                keywords: s.package.iter().chain(&s.keywords).cloned().collect(),
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
                title: a.name.clone(),
                subtitle: None,
                icon: None,
                kind: CommandKind::App,
                arguments: vec![],
                hotkey: None,
                keywords: vec![],
            });
            apps.push((id, a.path, a.name));
        }
        apps.sort_by_key(|a| a.2.to_lowercase());

        // An integration puts its verbs on the app row it drives, so the Spotify
        // row offers Play / Pause and Now Playing without a trip through search.
        let mut app_actions: HashMap<String, Vec<Action>> = HashMap::new();
        for (name, app, _, acts) in crate::config::INTEGRATIONS {
            if !integrations_on.contains(name) {
                continue;
            }
            if let Some(row) = commands
                .iter()
                .find(|c| c.kind == CommandKind::App && c.title.eq_ignore_ascii_case(app))
            {
                app_actions.insert(
                    row.id.clone(),
                    acts.iter()
                        .map(|(id, title)| Action {
                            id: (*id).into(),
                            title: (*title).into(),
                            key: None,
                        })
                        .collect(),
                );
            }
        }
        let app_index: HashMap<String, usize> = apps.iter().enumerate().map(|(i, a)| (a.0.clone(), i)).collect();

        if config.general.shortcuts {
            for name in &extras.shortcuts {
                commands.push(Command {
                    id: format!("sc:{name}"),
                    title: name.clone(),
                    subtitle: Some("Shortcut".into()),
                    icon: Some("⚡️".into()),
                    kind: CommandKind::Shortcut,
                    arguments: vec![],
                    hotkey: None,
                    keywords: vec!["shortcut".into(), "shortcuts".into()],
                });
            }
        }

        let mut mcp_tools = HashMap::new();
        for server in &config.mcp_servers {
            let Some(tools) = extras.mcp_tools.get(&server.id) else {
                continue;
            };
            for t in tools.iter().filter(|t| server.allows(&t.name)) {
                let id = t.command_id();
                if server.in_launcher {
                    let arguments = if t.simple_form() {
                        t.params()
                            .into_iter()
                            .take(3)
                            .map(|(name, _, required, _)| Argument {
                                placeholder: name,
                                optional: !required,
                                secure: false,
                            })
                            .collect()
                    } else {
                        vec![Argument {
                            placeholder: "Arguments (JSON)".into(),
                            optional: t.params().iter().all(|p| !p.2),
                            secure: false,
                        }]
                    };
                    let mut desc: String = t.description.lines().next().unwrap_or("").chars().take(70).collect();
                    if desc.is_empty() {
                        desc = "MCP tool".into();
                    }
                    commands.push(Command {
                        id: id.clone(),
                        title: t.display_title(),
                        subtitle: Some(format!("{} · {desc}", server.id)),
                        icon: Some("🔌".into()),
                        kind: CommandKind::Mcp,
                        arguments,
                        hotkey: None,
                        keywords: vec![server.id.clone(), t.name.clone(), "mcp".into()],
                    });
                }
                mcp_tools.insert(id, t.clone());
            }
        }

        // Resources and prompts a server advertises (cached, so listing them
        // never connects). Reading one is a command like any other.
        let mut mcp_resources = HashMap::new();
        let mut mcp_prompts = HashMap::new();
        for server in &config.mcp_servers {
            for r in extras.mcp_extras.resources.get(&server.id).cloned().unwrap_or_default() {
                let id = r.command_id();
                if server.in_launcher {
                    let desc: String = r.description.lines().next().unwrap_or("").chars().take(70).collect();
                    // One glyph for the whole MCP family, so the subtitle says
                    // what kind of thing this is: a document, not a tool.
                    let what = if r.mime.is_empty() {
                        "resource".to_string()
                    } else {
                        r.mime.clone()
                    };
                    let sub = if desc.is_empty() {
                        format!("{} · {what}", server.id)
                    } else {
                        format!("{} · {what} · {desc}", server.id)
                    };
                    commands.push(Command {
                        id: id.clone(),
                        title: r.display_title(),
                        subtitle: Some(sub),
                        icon: Some("🔌".into()),
                        kind: CommandKind::McpResource,
                        arguments: vec![],
                        hotkey: None,
                        keywords: vec![server.id.clone(), r.uri.clone(), "mcp".into(), "resource".into()],
                    });
                }
                mcp_resources.insert(id, r);
            }
            for pr in extras.mcp_extras.prompts.get(&server.id).cloned().unwrap_or_default() {
                let id = pr.command_id();
                if server.in_launcher {
                    let desc: String = pr.description.lines().next().unwrap_or("").chars().take(70).collect();
                    // "prompt" in the subtitle: same glyph as the tools, and it
                    // runs a chat, not the server's action.
                    let sub = if desc.is_empty() {
                        format!("{} · prompt", server.id)
                    } else {
                        format!("{} · prompt · {desc}", server.id)
                    };
                    commands.push(Command {
                        id: id.clone(),
                        title: pr.display_title(),
                        subtitle: Some(sub),
                        icon: Some("🔌".into()),
                        kind: CommandKind::McpPrompt,
                        arguments: pr
                            .arguments
                            .iter()
                            .take(3)
                            .map(|a| Argument {
                                placeholder: a.name.clone(),
                                optional: !a.required,
                                secure: false,
                            })
                            .collect(),
                        hotkey: None,
                        keywords: vec![server.id.clone(), pr.name.clone(), "mcp".into(), "prompt".into()],
                    });
                }
                mcp_prompts.insert(id, pr);
            }
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
            other => warnings.push(format!("theme `{other}` is unknown; use system, light, or dark")),
        }
        match config.general.presentation.as_str() {
            "island" | "spotlight" => {}
            other => warnings.push(format!("presentation `{other}` is unknown; use island or spotlight")),
        }

        // Resolve "id or title" references the way [[hotkeys]] and [[commands]] use them.
        let find = |commands: &[Command], wanted: &str| -> Option<usize> {
            let wanted = wanted.trim();
            commands
                .iter()
                .position(|c| c.id == wanted)
                .or_else(|| commands.iter().position(|c| c.title.eq_ignore_ascii_case(wanted)))
        };

        // [[commands]] overrides: rename / re-icon first so hotkeys can match the new title too.
        let mut overrides: HashMap<String, Override> = HashMap::new();
        for o in &config.commands {
            let Some(i) = find(&commands, &o.match_) else {
                warnings.push(format!("[[commands]] `{}`: no command with that id or title", o.match_));
                continue;
            };
            if let Some(t) = &o.title {
                if !t.trim().is_empty() {
                    commands[i].title = t.trim().to_string();
                }
            }
            if let Some(icon) = &o.icon {
                if !icon.trim().is_empty() {
                    commands[i].icon = Some(icon.trim().to_string());
                }
            }
            let entry = overrides.entry(commands[i].id.clone()).or_default();
            entry.aliases.extend(o.all_aliases());
            entry.boost += o.boost;
            entry.hidden |= o.hidden;
        }

        warnings.extend(config.include_warnings.iter().cloned());
        warnings.extend(config.parse_warnings.iter().cloned());
        // Attach [[hotkeys]] and `[[commands]] hotkey` to matching commands (by id or title).
        let mut key_for: HashMap<String, String> = HashMap::new();
        let mut seen_keys: HashMap<String, String> = HashMap::new();
        if let Some(k) = crate::keymap::normalize_hotkey(&config.general.hotkey) {
            seen_keys.insert(k, "the OpenCast hotkey".into());
        }
        for h in config.all_hotkeys() {
            if let Some(k) = crate::keymap::normalize_hotkey(&h.key) {
                if let Some(other) = seen_keys.get(&k) {
                    warnings.push(format!(
                        "hotkey `{}` for `{}` is already used by {other}",
                        h.key, h.command
                    ));
                    continue;
                }
                seen_keys.insert(k, format!("`{}`", h.command));
            }
            if let Some(i) = find(&commands, &h.command) {
                key_for.insert(commands[i].id.clone(), h.key.trim().to_string());
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

        let sidecars = RwLock::new(Sidecars::load(&config_dir));
        // Not a load-time warning: whether a browser store is readable depends on
        // what this machine has and what TCC allows, and a fresh install must stay
        // warning-free. Doctor and the Bookmarks command both say it out loud.
        let bookmark_scan = crate::bookmarks::scan_here();
        let history_scan = crate::history::scan_here();
        let mut engine = Self {
            config,
            config_dir,
            registry: Registry::with_overrides(commands, &overrides),
            router,
            warnings,
            theme,
            keymap,
            apps,
            app_index,
            scripts,
            ai,
            links,
            snippets,
            restore_ids,
            mcp_tools,
            mcp_resources,
            mcp_prompts,
            extras,
            bookmark_scan,
            history_scan,
            integrations_on,
            app_actions,
            sidecars,
            local_tz: crate::tz::local_zone_name(),
            zones: Default::default(),
        };
        // Report broken pipeline steps at load time, not on first run.
        let bad: Vec<String> = engine
            .config
            .pipelines
            .iter()
            .flat_map(|p| {
                p.steps
                    .iter()
                    .filter_map(|s| engine.pipeline_step(s).err())
                    .map(|e| format!("{e:#}"))
            })
            .collect();
        engine.warnings.extend(bad);
        engine
    }

    // ---- sidecar state: usage, pins, notes ---------------------------------

    /// Count a run of `id`, chosen after typing `query` (empty for hotkeys and
    /// deep links). Writes `usage.json` when the id is tracked.
    pub fn bump_usage(&self, id: &str, query: &str) {
        // Save outside the write lock so a slow disk flush cannot block search readers.
        let dirty = {
            let mut s = self.sidecars.write().unwrap();
            s.usage.record(id, query, crate::usage::now()).then(|| s.usage.clone())
        };
        if let Some(usage) = dirty {
            usage.save(&self.config_dir);
        }
    }

    /// Toggle a pin. Returns whether the id is pinned afterwards.
    pub fn toggle_pin(&self, id: &str) -> bool {
        let mut s = self.sidecars.write().unwrap();
        let pinned = crate::pins::toggle_in(&mut s.pins, id);
        if crate::pins::pinnable(id) {
            s.pin_set = s.pins.iter().cloned().collect();
            crate::pins::save_list(&self.config_dir, &s.pins);
        }
        pinned
    }

    pub fn is_pinned(&self, id: &str) -> bool {
        self.sidecars.read().unwrap().pin_set.contains(id)
    }

    pub fn add_note(&self, title: &str, body: &str) -> Note {
        let mut s = self.sidecars.write().unwrap();
        let note = crate::notes::add_in(&mut s.notes, title, body);
        crate::notes::save_all(&self.config_dir, &s.notes);
        note
    }

    pub fn note(&self, id: &str) -> Option<Note> {
        self.sidecars.read().unwrap().notes.iter().find(|n| n.id == id).cloned()
    }

    pub fn delete_note(&self, id: &str) -> bool {
        let mut s = self.sidecars.write().unwrap();
        let before = s.notes.len();
        s.notes.retain(|n| n.id != id);
        let removed = s.notes.len() != before;
        if removed {
            crate::notes::save_all(&self.config_dir, &s.notes);
        }
        removed
    }

    // ---- saved chats ---------------------------------------------------------

    pub fn save_chat(&self, chat: crate::chats::SavedChat) {
        let mut s = self.sidecars.write().unwrap();
        crate::chats::upsert(&mut s.chats, chat);
        crate::chats::save_all(&self.config_dir, &s.chats);
    }

    pub fn delete_chat(&self, id: &str) -> bool {
        let mut s = self.sidecars.write().unwrap();
        let removed = crate::chats::remove(&mut s.chats, id);
        if removed {
            crate::chats::save_all(&self.config_dir, &s.chats);
        }
        removed
    }

    pub fn chat(&self, id: &str) -> Option<crate::chats::SavedChat> {
        self.sidecars.read().unwrap().chats.iter().find(|c| c.id == id).cloned()
    }

    // ---- extras: snippets, MCP, shortcuts -----------------------------------

    /// `(keyword, text)` for snippets that expand while typing in other apps.
    pub fn expanding_snippets(&self) -> Vec<(String, String)> {
        if !self.config.general.snippet_expansion {
            return Vec::new();
        }
        self.config
            .snippets
            .iter()
            .filter(|s| !s.keyword.trim().is_empty() && !s.text.is_empty() && s.expands_while_typing())
            .map(|s| (s.keyword.trim().to_string(), s.text.clone()))
            .collect()
    }

    pub fn mcp_servers(&self) -> &[crate::mcp::McpServerConfig] {
        &self.config.mcp_servers
    }

    /// MCP tools offered to the AI chat (servers with `in_chat`).
    pub fn chat_tools(&self) -> Vec<ChatTool> {
        let mut out = Vec::new();
        for server in self.config.mcp_servers.iter().filter(|s| s.in_chat) {
            let Some(tools) = self.extras.mcp_tools.get(&server.id) else {
                continue;
            };
            for t in tools.iter().filter(|t| server.allows(&t.name)) {
                out.push(ChatTool {
                    spec: crate::providers::ToolSpec {
                        name: t.llm_name(),
                        description: if t.description.is_empty() {
                            t.display_title()
                        } else {
                            t.description.clone()
                        },
                        parameters: t.input_schema.clone(),
                    },
                    server: server.id.clone(),
                    tool: t.name.clone(),
                    auto_approve: server.auto_approves(&t.name),
                });
            }
        }
        out
    }

    pub fn shortcut_names(&self) -> &[String] {
        &self.extras.shortcuts
    }

    // ---- preview pane --------------------------------------------------------

    /// What the preview pane shows for `id`, when core knows (the panel handles
    /// clipboard rows and plain files itself).
    pub fn preview(&self, id: &str) -> Option<Preview> {
        let p = |title: &str, text: Option<String>, path: Option<String>, facts: Vec<(&str, String)>| Preview {
            title: title.to_string(),
            text,
            path,
            facts: facts.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        };
        if id.starts_with("note:") {
            let n = self.note(id)?;
            return Some(p(
                if n.title.is_empty() { "Note" } else { &n.title },
                Some(n.body),
                None,
                vec![],
            ));
        }
        if id.starts_with("chat:") {
            let c = self.chat(id)?;
            let text = c
                .messages
                .iter()
                .filter(|m| matches!(m.role, crate::providers::Role::User | crate::providers::Role::Assistant))
                .filter(|m| !m.content.trim().is_empty())
                .map(|m| {
                    let who = if m.role == crate::providers::Role::User {
                        "**You**"
                    } else {
                        "**AI**"
                    };
                    format!("{who}\n{}", m.content.trim())
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            return Some(p(
                &c.title,
                Some(text),
                None,
                vec![("Model", c.model.unwrap_or_default())],
            ));
        }
        if let Some(text) = self.snippets.get(id) {
            return Some(p("Snippet", Some(text.clone()), None, vec![]));
        }
        if let Some(meta) = self.scripts.get(id) {
            let src = std::fs::read_to_string(&meta.path)
                .map(|s| s.chars().take(4000).collect::<String>())
                .ok();
            return Some(p(
                &meta.title,
                src.map(|s| format!("```\n{s}\n```")),
                Some(meta.path.display().to_string()),
                vec![("Mode", format!("{:?}", meta.mode))],
            ));
        }
        if let Some(c) = self.ai.get(id) {
            let mut facts = vec![];
            if let Some(m) = &c.model {
                facts.push(("Model", m.clone()));
            }
            facts.push(("Output", format!("{:?}", c.output).to_lowercase()));
            let mut text = c.prompt.clone();
            if let Some(sys) = &c.system {
                text = format!("*System:* {sys}\n\n{text}");
            }
            return Some(p(&c.title, Some(text), None, facts));
        }
        if let Some(l) = self.links.get(id) {
            return Some(p(&l.title, Some(l.url.clone()), None, vec![]));
        }
        if let Some(t) = self.mcp_tools.get(id) {
            let params = t
                .params()
                .into_iter()
                .map(|(n, ty, req, d)| {
                    format!(
                        "- `{n}` ({ty}{}){}",
                        if req { ", required" } else { "" },
                        if d.is_empty() { String::new() } else { format!(": {d}") }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Some(p(
                &t.display_title(),
                Some(format!("{}\n\n{params}", t.description)),
                None,
                vec![("Server", t.server.clone()), ("Tool", t.name.clone())],
            ));
        }
        if let Some(r) = self.mcp_resources.get(id) {
            return Some(p(
                &r.display_title(),
                (!r.description.is_empty()).then(|| r.description.clone()),
                None,
                vec![
                    ("Server", r.server.clone()),
                    ("URI", r.uri.clone()),
                    (
                        "Type",
                        if r.mime.is_empty() {
                            "resource".into()
                        } else {
                            r.mime.clone()
                        },
                    ),
                ],
            ));
        }
        if let Some(pr) = self.mcp_prompts.get(id) {
            let params = if pr.arguments.is_empty() {
                "No arguments.".to_string()
            } else {
                pr.arguments
                    .iter()
                    .map(|a| {
                        format!(
                            "- `{}`{}{}",
                            a.name,
                            if a.required { ", required" } else { "" },
                            if a.description.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", a.description)
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return Some(p(
                &pr.display_title(),
                Some(format!("{}\n\n{params}", pr.description)),
                None,
                vec![("Server", pr.server.clone()), ("Prompt", pr.name.clone())],
            ));
        }
        if let Some(path) = self.app_path(id) {
            return Some(p(
                self.registry.get(id).map(|c| c.title.as_str()).unwrap_or("App"),
                None,
                Some(path.display().to_string()),
                vec![],
            ));
        }
        None
    }

    /// Re-read the sidecar files (another process or a sync tool changed them).
    pub fn reload_sidecars(&self) {
        *self.sidecars.write().unwrap() = Sidecars::load(&self.config_dir);
    }

    const WEB_FALLBACK_ID: &'static str = "fallback:web";
    const WEB_FALLBACK_URL: &'static str = "https://duckduckgo.com/?q={query}";

    /// Fallback entries that are not special tokens, resolved to commands.
    fn fallback_commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        for f in &self.config.general.fallbacks {
            match f.trim().to_lowercase().as_str() {
                "files" => {}
                "web" => {
                    out.push(Command {
                        id: Self::WEB_FALLBACK_ID.into(),
                        title: "Search the Web".into(),
                        subtitle: Some(host_of(Self::WEB_FALLBACK_URL)),
                        icon: Some("🔗".into()),
                        kind: CommandKind::Quicklink,
                        arguments: vec![Argument {
                            placeholder: "Query".into(),
                            optional: false,
                            secure: false,
                        }],
                        hotkey: None,
                        keywords: vec![],
                    });
                }
                "quicklinks" => {
                    for link in &self.config.quicklinks {
                        if link.url.contains("{query}") {
                            if let Some(c) = self.registry.get(&format!("link:{}", link.key())) {
                                out.push(c.clone());
                            }
                        }
                    }
                }
                _ => {
                    if let Some(c) = self.registry.resolve(f) {
                        out.push(c.clone());
                    }
                }
            }
        }
        out
    }

    /// Whether `files` is one of the fallbacks (the panel expands to Spotlight).
    pub fn fallback_files(&self) -> bool {
        self.config
            .general
            .fallbacks
            .iter()
            .any(|f| f.trim().eq_ignore_ascii_case("files"))
    }

    /// Currency and time-zone answers for the top of the list.
    fn conversions(&self, q: &str) -> Vec<Command> {
        let mut out = Vec::new();
        if q.len() < 4 {
            return out;
        }
        let row = |id: String, title: String, subtitle: String, icon: &str| Command {
            id,
            title,
            subtitle: Some(subtitle),
            icon: Some(icon.into()),
            kind: CommandKind::Calculator,
            arguments: vec![],
            hotkey: None,
            keywords: vec![],
        };
        if let Some(rates) = self.extras.rates.as_ref().filter(|_| self.config.general.currency) {
            if let Some(c) = crate::currency::convert(q, rates) {
                let value = crate::currency::format_money(c.value).replace(',', "");
                out.push(row(format!("calc:{value}"), c.title(), c.subtitle(rates), "💱"));
            }
        }
        let load = |name: &str| -> Option<std::sync::Arc<crate::tz::Zone>> {
            let mut cache = self.zones.lock().unwrap();
            cache
                .entry(name.to_string())
                .or_insert_with(|| crate::tz::Zone::load(name).map(std::sync::Arc::new))
                .clone()
        };
        if let Some(t) = crate::tz::answer_with(q, crate::usage::now() as i64, &self.local_tz, &load) {
            out.push(row(format!("calc:{}", t.copy), t.title, t.subtitle, "🕒"));
        }
        out
    }

    pub fn update_note(&self, id: &str, title: &str, body: &str) -> bool {
        let mut s = self.sidecars.write().unwrap();
        let Some(n) = s.notes.iter_mut().find(|n| n.id == id) else {
            return false;
        };
        n.title = title.trim().to_string();
        n.body = body.to_string();
        n.updated = crate::usage::now();
        // Most recently edited first.
        let i = s.notes.iter().position(|n| n.id == id).unwrap_or(0);
        let note = s.notes.remove(i);
        s.notes.insert(0, note);
        crate::notes::save_all(&self.config_dir, &s.notes);
        true
    }

    /// `profile.md`, created with a short header when missing (Edit AI Profile).
    pub fn profile_path_created(&self) -> PathBuf {
        let p = self.config_dir.join(PROFILE_FILE);
        if !p.exists() {
            let _ = std::fs::write(
                &p,
                "# About me\n\n<!-- OpenCast adds this file to AI chats (ai_profile = true). One fact per line. -->\n",
            );
        }
        p
    }

    /// "OpenCast Stats": totals, top commands and learned queries, from `usage.json`.
    pub fn stats_markdown(&self) -> String {
        let s = self.sidecars.read().unwrap();
        let usage = &s.usage;
        let total: u64 = usage.commands.values().map(|e| e.count as u64).sum();
        let mut top: Vec<(&String, &crate::usage::Entry)> = usage.commands.iter().collect();
        top.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        let title = |id: &str| {
            self.registry.get(id).map(|c| c.title.clone()).unwrap_or_else(|| {
                id.split_once(':')
                    .map(|(_, t)| t.to_string())
                    .unwrap_or_else(|| id.to_string())
            })
        };
        let mut md = format!(
            "# OpenCast Stats\n\n**{total}** runs of **{}** different commands.\n\n## Most used\n\n",
            usage.commands.len()
        );
        for (id, e) in top.iter().take(15) {
            md.push_str(&format!("- {} — {}×\n", title(id), e.count));
        }
        let mut picks: Vec<(&String, u32)> = usage
            .picks
            .iter()
            .map(|(q, p)| (q, p.ids.values().sum::<u32>()))
            .filter(|(q, _)| !q.is_empty())
            .collect();
        picks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        if !picks.is_empty() {
            md.push_str("\n## What you type most\n\n");
            for (q, n) in picks.iter().take(10) {
                let best = usage.picks[*q]
                    .ids
                    .iter()
                    .max_by_key(|(_, c)| **c)
                    .map(|(id, _)| title(id));
                md.push_str(&format!("- `{q}` → {} ({n}×)\n", best.unwrap_or_default()));
            }
        }
        md.push_str(&format!(
            "\n## Library\n\n- {} commands, {} snippets, {} notes, {} saved chats\n",
            self.registry.len(),
            self.config.snippets.len(),
            s.notes.len(),
            s.chats.len()
        ));
        md
    }

    /// Scripts and AI commands that declare they act on `kind` (`text`, `url`, `file`).
    pub fn accepting(&self, kind: &str) -> Vec<Command> {
        let mut out: Vec<Command> = self
            .registry
            .iter()
            .filter(|c| {
                let declared = match c.kind {
                    CommandKind::Script => self
                        .scripts
                        .get(&c.id)
                        .is_some_and(|m| m.accepts.iter().any(|a| a == kind)),
                    CommandKind::Ai => self.ai.get(&c.id).is_some_and(|a| a.accepts(kind)),
                    _ => false,
                };
                // Pipelines that start from the selection take text.
                let pipe = kind == "text"
                    && c.id.strip_prefix("pipe:").is_some_and(|pid| {
                        self.config.pipelines.iter().any(|p| {
                            p.id == pid
                                && crate::pipeline::Input::parse(p.input.as_deref())
                                    == crate::pipeline::Input::Selection
                        })
                    });
                declared || pipe || (kind == "text" && c.id == "builtin:translate")
            })
            .cloned()
            .collect();
        out.sort_by_key(|c| c.title.to_lowercase());
        out
    }

    /// Order action ids by what was picked for `query` before (`ua:text`…), then by use.
    pub fn rank_ids(&self, query: &str, ids: &[String]) -> Vec<String> {
        let s = self.sidecars.read().unwrap();
        let learned = s.usage.learned_all(&query.to_lowercase());
        let now = crate::usage::now();
        let mut scored: Vec<(i64, usize, &String)> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (learned.get(id).copied().unwrap_or(0) + s.usage.frecency(id, now), i, id))
            .collect();
        // Stable: equal scores keep the given (sensible default) order.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, id)| id.clone()).collect()
    }

    pub fn preset(&self, id: &str) -> Option<&crate::config::AiPreset> {
        self.config.ai_presets.iter().find(|p| p.id == id)
    }

    /// What a chat stream adds on top of its request: the profile text (when
    /// enabled globally or by the preset) and the preset's tool globs.
    pub fn chat_context(&self, preset: Option<&str>, profile_wanted: bool) -> (Option<String>, Option<Vec<String>>) {
        let p = preset.and_then(|id| self.preset(id));
        let use_profile = profile_wanted && p.and_then(|p| p.profile).unwrap_or(self.config.general.ai_profile);
        let profile = use_profile
            .then(|| std::fs::read_to_string(self.config_dir.join(PROFILE_FILE)).ok())
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        (profile, p.and_then(|p| p.tools.clone()))
    }

    /// "Remember this": append a line to `profile.md`.
    pub fn profile_append(&self, line: &str) -> Result<()> {
        use std::io::Write;
        let line = line.trim().replace('\n', " ");
        anyhow::ensure!(!line.is_empty(), "nothing to remember");
        let path = self.config_dir.join(PROFILE_FILE);
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(f, "- {line}")?;
        Ok(())
    }

    pub fn notes(&self) -> Vec<Note> {
        self.sidecars.read().unwrap().notes.clone()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Command> {
        let q = query.trim();
        let ql = q.to_lowercase();
        // Clone sidecars under a short read lock so typing cannot pile up readers
        // that stall bump_usage (and thus Return/click) on the write lock.
        let (usage, pin_set, notes, chats) = {
            let s = self.sidecars.read().unwrap();
            (s.usage.clone(), s.pin_set.clone(), s.notes.clone(), s.chats.clone())
        };
        let mut out = self.registry.search_with_usage(query, limit, &usage, &pin_set);
        for (i, row) in self.conversions(q).into_iter().enumerate() {
            out.insert(i, row);
        }
        if let Some(word) = ql.strip_prefix("spell ").map(str::trim).filter(|w| !w.is_empty()) {
            out.insert(
                0,
                Command {
                    id: format!("spell:{word}"),
                    title: format!("Spell “{word}”"),
                    subtitle: Some("Spelling suggestions".into()),
                    icon: Some("🔤".into()),
                    kind: CommandKind::Builtin,
                    arguments: vec![],
                    hotkey: None,
                    keywords: vec![],
                },
            );
        }
        if let Some((text, lang)) = parse_translate(q) {
            let to = lang.clone().unwrap_or_else(|| self.config.general.translate_to.clone());
            out.insert(
                0,
                Command {
                    id: format!("translate:{}\u{1f}{text}", lang.unwrap_or_default()),
                    title: format!("Translate “{text}”"),
                    subtitle: Some(format!("to {to} · AI")),
                    icon: Some("🌐".into()),
                    kind: CommandKind::Builtin,
                    arguments: vec![],
                    hotkey: None,
                    keywords: vec![],
                },
            );
        }
        if let Some(word) = ql.strip_prefix("define ").map(str::trim).filter(|w| !w.is_empty()) {
            out.insert(
                0,
                Command {
                    id: format!("define:{word}"),
                    title: format!("Define “{word}”"),
                    subtitle: Some("Dictionary".into()),
                    icon: Some("📖".into()),
                    kind: CommandKind::Builtin,
                    arguments: vec![],
                    hotkey: None,
                    keywords: vec![],
                },
            );
        }
        for row in crate::notes::matching_in(&notes, q, 20) {
            if !out.iter().any(|c| c.id == row.id) {
                out.push(row);
            }
        }
        let bm = crate::bookmarks::matching(&self.extras.bookmarks, q);
        if crate::bookmarks::needle(q).1 {
            // `bm …`: bookmarks only, first.
            out.retain(|c| c.kind != CommandKind::Calculator);
            let rest = std::mem::take(&mut out);
            out = bm;
            out.extend(rest);
        } else {
            for row in bm {
                if !out.iter().any(|c| c.id == row.id) {
                    out.push(row);
                }
            }
        }
        let hist = crate::history::matching(&self.extras.history, q);
        if crate::history::needle(q).1 {
            // `hist …` / `history …`: history only, first.
            out.retain(|c| c.kind != CommandKind::Calculator);
            let rest = std::mem::take(&mut out);
            out = hist;
            out.extend(rest);
        } else {
            for row in hist {
                if !out.iter().any(|c| c.id == row.id) {
                    out.push(row);
                }
            }
        }
        let rec = crate::recents::matching(&self.extras.recents, q);
        if crate::recents::needle(q).1 {
            // `recent …` / `recents …`: recents only, first.
            out.retain(|c| c.kind != CommandKind::Calculator);
            let rest = std::mem::take(&mut out);
            out = rec;
            out.extend(rest);
        } else {
            for row in rec {
                if !out.iter().any(|c| c.id == row.id) {
                    out.push(row);
                }
            }
        }
        // `chats …` lists saved chats first.
        let chat_rows = crate::chats::matching_in(&chats, q, 30);
        if !chat_rows.is_empty() {
            out.retain(|c| !chat_rows.iter().any(|r| r.id == c.id));
            let rest = std::mem::take(&mut out);
            out = chat_rows;
            out.extend(rest);
        }

        // Emoji overlay (not in the permanent registry): recent ones first.
        let recent_emoji: Vec<String> = if crate::emoji::needle(q).is_some() {
            let s = self.sidecars.read().unwrap();
            let now = crate::usage::now();
            let mut ids: Vec<(&String, i64)> = s
                .usage
                .commands
                .keys()
                .filter(|k| k.starts_with("emoji:"))
                .map(|k| (k, s.usage.frecency(k, now)))
                .collect();
            ids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            ids.into_iter().take(8).map(|(k, _)| k.clone()).collect()
        } else {
            Vec::new()
        };
        for row in crate::emoji::matching_with(q, 40, &recent_emoji, &self.config.general.emoji_skin_tone) {
            if !out.iter().any(|c| c.id == row.id) {
                out.push(row);
            }
        }

        // Quit-by-name: only when the query looks like a quit.
        if ql == "quit" || ql.starts_with("quit ") || ql.starts_with("kill ") {
            let needle = ql.trim_start_matches("quit").trim_start_matches("kill").trim();
            for (id, _, name) in &self.apps {
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

        // Fallbacks when hits are thin.
        if !q.is_empty() {
            let strong = out.iter().filter(|c| c.kind != CommandKind::Calculator).count();
            if strong < self.config.general.fallback_threshold {
                for c in self.fallback_commands() {
                    if !out.iter().any(|x| x.id == c.id) {
                        out.push(c);
                    }
                }
            }
        }

        if out.len() > limit {
            out.truncate(limit);
        }
        out
    }

    /// Resolve the app path behind an `app:` or `quit:app:` id.
    pub fn app_path(&self, id: &str) -> Option<&Path> {
        self.app_index.get(id).map(|&i| self.apps[i].1.as_path())
    }

    /// Every indexed app bundle, sorted by name (icon prefetch).
    /// Bookmarks currently in the cache (search never reads the store files).
    pub fn bookmark_count(&self) -> usize {
        self.extras.bookmarks.len()
    }

    pub fn history_count(&self) -> usize {
        self.extras.history.len()
    }

    /// ⌘-menu actions an integration added to this command (the app row of a
    /// running integration), if any.
    pub fn actions_for(&self, id: &str) -> Option<&[Action]> {
        self.app_actions.get(id).map(|a| a.as_slice())
    }

    pub fn app_paths(&self) -> Vec<&Path> {
        self.apps.iter().map(|a| a.1.as_path()).collect()
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
        for h in self.config.all_hotkeys() {
            let id = self.registry.resolve(&h.command).map(|c| c.id.clone());
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

    /// Translate `text` (empty: the selection) into `lang` or `translate_to`.
    pub fn translate_plan(&self, text: &str, lang: Option<&str>) -> Plan {
        let g = &self.config.general;
        let to = lang.map(str::to_string).unwrap_or_else(|| g.translate_to.clone());
        let alt = g
            .translate_alt
            .clone()
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| {
                if to.eq_ignore_ascii_case("english") {
                    "Spanish".into()
                } else {
                    "English".into()
                }
            });
        let system = format!(
            "You are a translator. Translate the user's text into {to}. If it is already in {to}, \
             translate it into {alt} instead. Keep the meaning, tone and formatting. Reply with the \
             translation only, no notes or quotes."
        );
        let selection = text.trim().is_empty();
        Plan::Chat {
            prompt: Some(if selection {
                "{selection}".into()
            } else {
                text.trim().to_string()
            }),
            system: Some(system),
            provider: None,
            model: None,
            needs_selection: selection,
            temperature: Some(0.2),
            max_tokens: None,
            output: AiOutput::Chat,
            preset: None,
        }
    }

    /// Resolve one `[[pipelines]]` step string.
    pub fn pipeline_step(&self, spec: &str) -> Result<crate::pipeline::Step> {
        use crate::pipeline::{Step, Transform};
        let spec = spec.trim();
        let ai_step = |c: &AiCommandConfig| Step::Ai {
            label: c.title.clone(),
            prompt: c.prompt.clone(),
            system: c.system.clone(),
            provider: c.provider.clone(),
            model: c.model.clone(),
            temperature: c.temperature,
            max_tokens: c.max_tokens,
        };
        let script = |key: &str| -> Option<Step> {
            let path = crate::config::expand_tilde(Path::new(key));
            self.scripts
                .get(&format!("script:{}", path.display()))
                .or_else(|| self.scripts.values().find(|m| m.title.eq_ignore_ascii_case(key)))
                .map(|m| Step::Script(Box::new(m.clone())))
        };
        let ai = |id: &str| self.ai.get(&format!("ai:{id}")).or_else(|| self.ai.get(id));
        let (kind, rest) = spec.split_once(':').unwrap_or(("", spec));
        Ok(match kind.trim() {
            "ai" => {
                ai_step(ai(rest.trim()).with_context(|| format!("pipeline step `{spec}`: no AI command `{rest}`"))?)
            }
            "prompt" => Step::Ai {
                label: "AI prompt".into(),
                prompt: rest.trim().to_string(),
                system: None,
                provider: None,
                model: None,
                temperature: None,
                max_tokens: None,
            },
            "shell" => Step::Shell(rest.trim().to_string()),
            "transform" => Step::Transform(
                Transform::parse(rest).with_context(|| format!("pipeline step `{spec}`: unknown transform"))?,
            ),
            "script" => script(rest.trim()).with_context(|| format!("pipeline step `{spec}`: no such script"))?,
            _ => {
                if let Some(t) = Transform::parse(spec) {
                    Step::Transform(t)
                } else if let Some(c) = ai(spec) {
                    ai_step(c)
                } else if let Some(s) = script(spec) {
                    s
                } else {
                    bail!("pipeline step `{spec}`: use ai:, prompt:, script:, shell: or transform:")
                }
            }
        })
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
                    Builtin::Appearance => Plan::Ui("appearance-menu"),
                    Builtin::PanelPosition => Plan::Ui("panel-position-menu"),
                    Builtin::StayAwake => Plan::Ui("stay-awake-menu"),
                    Builtin::Settings => Plan::Ui("settings"),
                    Builtin::InstallExtension => {
                        Plan::Extensions(ExtensionOp::Install(args.first().cloned().unwrap_or_default()))
                    }
                    Builtin::UpdateExtensions => Plan::Extensions(ExtensionOp::Update),
                    Builtin::ManageExtensions => Plan::Extensions(ExtensionOp::Manage),
                    Builtin::BrowseExtensions => Plan::Extensions(ExtensionOp::Browse),
                    Builtin::ImportAlfredWorkflow => Plan::ImportAlfred(args.first().cloned().unwrap_or_default()),
                    Builtin::Doctor => Plan::Doctor,
                    Builtin::Contacts => Plan::Ui("contacts"),
                    Builtin::Reminders => Plan::Ui("reminders"),
                    Builtin::NewReminder => Plan::UiArgs {
                        action: "new-reminder",
                        args: args.to_vec(),
                    },
                    Builtin::DefineWord => Plan::UiArgs {
                        action: "define",
                        args: args.to_vec(),
                    },
                    Builtin::LargeType => Plan::UiArgs {
                        action: "large-type",
                        args: args.to_vec(),
                    },
                    Builtin::MediaPlayPause => Plan::Ui("media-play"),
                    Builtin::MediaNext => Plan::Ui("media-next"),
                    Builtin::MediaPrevious => Plan::Ui("media-previous"),
                    Builtin::Spotify => Plan::Ui("spotify"),
                    Builtin::SpotifyPlay => Plan::Ui("spotify-play"),
                    Builtin::SpotifyNext => Plan::Ui("spotify-next"),
                    Builtin::SpotifyPrevious => Plan::Ui("spotify-previous"),
                    Builtin::SpotifyNowPlaying => Plan::Ui("spotify-now"),
                    Builtin::SpotifySearch => {
                        let q = args.first().map(|s| s.as_str()).unwrap_or("").trim();
                        // List rows pass the chosen URI back as the arg.
                        if q.starts_with("spotify:") && !q.starts_with("spotify:search:") {
                            Plan::OpenUrl(q.to_string())
                        } else if self.config.spotify.list_enabled() {
                            Plan::UiArgs {
                                action: "spotify-search-list",
                                args: vec![q.to_string()],
                            }
                        } else {
                            Plan::OpenUrl(crate::spotify::search_deep_link(q))
                        }
                    }
                    Builtin::ActOnSelection => Plan::Ui("selection"),
                    Builtin::StartFocus => Plan::UiArgs {
                        action: "focus-start",
                        args: args.to_vec(),
                    },
                    Builtin::StopFocus => Plan::Ui("focus-stop"),
                    Builtin::PauseFocus => Plan::Ui("focus-pause"),
                    Builtin::SearchBookmarks => {
                        if self.extras.bookmarks.is_empty() {
                            // Empty and silent looks broken; tell them why.
                            Plan::ShowText {
                                title: "Bookmarks".into(),
                                text: crate::bookmarks::why_empty(&self.bookmark_scan),
                            }
                        } else {
                            Plan::FillQuery("bm ".into())
                        }
                    }
                    Builtin::SearchHistory => {
                        if self.extras.history.is_empty() {
                            Plan::ShowText {
                                title: "History".into(),
                                text: crate::history::why_empty(&self.history_scan),
                            }
                        } else {
                            Plan::FillQuery("hist ".into())
                        }
                    }
                    Builtin::EditNote => Plan::UiArgs {
                        action: "note-editor",
                        args: vec![],
                    },
                    Builtin::Restart => {
                        Plan::SystemShell("osascript -e 'tell application \"System Events\" to restart'")
                    }
                    Builtin::ShutDown => {
                        Plan::SystemShell("osascript -e 'tell application \"System Events\" to shut down'")
                    }
                    Builtin::LogOut => {
                        Plan::SystemShell("osascript -e 'tell application \"System Events\" to log out'")
                    }
                    Builtin::ToggleHiddenFiles => Plan::SystemShell(
                        "v=$(defaults read com.apple.finder AppleShowAllFiles 2>/dev/null); \
                         if [ \"$v\" = 1 ] || [ \"$v\" = TRUE ] || [ \"$v\" = true ]; then n=false; else n=true; fi; \
                         defaults write com.apple.finder AppleShowAllFiles -bool $n && killall Finder",
                    ),
                    Builtin::QuitAllApps => Plan::SystemShell(
                        "osascript -e 'tell application \"System Events\" to set names to name of every application process whose background only is false and name is not \"Finder\" and name is not \"OpenCast\"' -e 'repeat with n in names' -e 'try' -e 'tell application (contents of n) to quit' -e 'end try' -e 'end repeat'",
                    ),
                    Builtin::HideOthers => Plan::Ui("hide-others"),
                    Builtin::BrowseFiles => Plan::Ui("browse-files"),
                    Builtin::ArrangeWindow => Plan::Ui("arrange-window"),
                    Builtin::Translate => {
                        let text = args.first().map(|s| s.trim()).unwrap_or("");
                        let lang = args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty());
                        self.translate_plan(text, lang)
                    }
                    Builtin::AskPage => Plan::UiArgs {
                        action: "page",
                        args: vec!["ask".into(), args.first().cloned().unwrap_or_default()],
                    },
                    Builtin::SummarizePage => Plan::UiArgs {
                        action: "page",
                        args: vec!["summarize".into()],
                    },
                    Builtin::DictateAnywhere => Plan::Ui("dictate-anywhere"),
                    Builtin::CheatSheet => Plan::Ui("shortcuts"),
                    Builtin::StickyNote => Plan::Ui("sticky-note"),
                    Builtin::CompareModels => Plan::UiArgs {
                        action: "compare",
                        args: args.to_vec(),
                    },
                    Builtin::ShowDesktop => Plan::SystemShell(
                        "'/System/Applications/Mission Control.app/Contents/MacOS/Mission Control' 1",
                    ),
                    Builtin::Spell => Plan::UiArgs {
                        action: "spell",
                        args: args.to_vec(),
                    },
                    Builtin::SaveSnippet => Plan::AddSnippet {
                        keyword: args.first().cloned().unwrap_or_default(),
                        text: args.get(1).cloned().unwrap_or_default(),
                    },
                    Builtin::Stats => Plan::ShowText {
                        title: "OpenCast Stats".into(),
                        text: self.stats_markdown(),
                    },
                    Builtin::EditProfile => Plan::OpenUrl(self.profile_path_created().to_string_lossy().into_owned()),
                    Builtin::MenuItems => Plan::Ui("menu-items"),
                    Builtin::ChatHistory => Plan::FillQuery("chats".into()),
                    Builtin::Dictate => Plan::Ui("dictate"),
                    Builtin::MySchedule => Plan::Ui("schedule"),
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
        if let Some(pane) = id.strip_prefix("pane:") {
            return crate::panes::url(pane)
                .map(Plan::OpenUrl)
                .ok_or_else(|| anyhow::anyhow!("unknown settings pane `{pane}`"));
        }
        if let Some(url) = id.strip_prefix("bm:") {
            return Ok(Plan::OpenUrl(url.to_string()));
        }
        if let Some(url) = id.strip_prefix("hist:") {
            return Ok(Plan::OpenUrl(url.to_string()));
        }
        if let Some(word) = id.strip_prefix("spell:") {
            return Ok(Plan::UiArgs {
                action: "spell",
                args: vec![word.to_string()],
            });
        }
        if let Some(rest) = id.strip_prefix("translate:") {
            let (lang, text) = rest.split_once('\u{1f}').unwrap_or(("", rest));
            return Ok(self.translate_plan(text, Some(lang).filter(|l| !l.is_empty())));
        }
        if let Some(word) = id.strip_prefix("define:") {
            return Ok(Plan::UiArgs {
                action: "define",
                args: vec![word.to_string()],
            });
        }
        if let Some(name) = id.strip_prefix("sc:") {
            let input = args.first().filter(|s| !s.trim().is_empty()).cloned();
            return Ok(Plan::RunShortcut {
                name: name.to_string(),
                input,
            });
        }
        if let Some(t) = self.mcp_tools.get(id) {
            return Ok(Plan::McpCall {
                server: t.server.clone(),
                tool: t.name.clone(),
                arguments: t.arguments_from(args)?,
            });
        }
        if let Some(r) = self.mcp_resources.get(id) {
            return Ok(Plan::McpReadResource {
                server: r.server.clone(),
                uri: r.uri.clone(),
            });
        }
        if let Some(p) = self.mcp_prompts.get(id) {
            return Ok(Plan::McpGetPrompt {
                server: p.server.clone(),
                name: p.name.clone(),
                arguments: p.arguments_from(args)?,
            });
        }
        if id.starts_with("chat:") {
            return self
                .chat(id)
                .map(|c| Plan::ResumeChat(Box::new(c)))
                .ok_or_else(|| anyhow::anyhow!("chat not found"));
        }
        if let Some(glyph) = crate::emoji::glyph_with_tone(id, &self.config.general.emoji_skin_tone) {
            return Ok(Plan::PasteText(glyph));
        }
        if id.starts_with("note:") {
            if self.note(id).is_some() {
                // Return opens the note in the Notes editor window.
                return Ok(Plan::UiArgs {
                    action: "note-editor",
                    args: vec![id.to_string()],
                });
            }
            bail!("note not found");
        }
        if let Some(app_id) = id.strip_prefix("quit:") {
            if let Some(&i) = self.app_index.get(app_id) {
                // AppleScript addresses apps by their bundle name, not the display name.
                let name = self.apps[i]
                    .1
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
            if key.starts_with("custom:") || key.starts_with("display-") {
                return Ok(Plan::WindowKey(key.to_string()));
            }
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
        if let Some(path) = self.app_path(id) {
            return Ok(Plan::LaunchApp(path.to_path_buf()));
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
        if id == Self::WEB_FALLBACK_ID {
            let q = args.first().map(String::as_str).unwrap_or("");
            if q.trim().is_empty() {
                bail!("`Search the Web` needs a query");
            }
            return Ok(Plan::OpenUrl(Self::WEB_FALLBACK_URL.replace("{query}", &url_encode(q))));
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
                temperature: c.temperature,
                max_tokens: c.max_tokens,
                output: c.output,
                preset: None,
            });
        }
        if let Some(pid) = id.strip_prefix("pipe:") {
            let p = self
                .config
                .pipelines
                .iter()
                .find(|p| p.id == pid)
                .context("pipeline not found — try Reload Commands")?;
            anyhow::ensure!(!p.steps.is_empty(), "pipeline `{}` has no steps", p.id);
            let steps = p
                .steps
                .iter()
                .map(|s| self.pipeline_step(s))
                .collect::<Result<Vec<_>>>()?;
            return Ok(Plan::Pipeline {
                title: p.title.clone(),
                input: crate::pipeline::Input::parse(p.input.as_deref()),
                query: args.first().cloned(),
                steps,
                output: crate::pipeline::Output::parse(p.output.as_deref()),
            });
        }
        if let Some(p) = id.strip_prefix("preset:").and_then(|pid| self.preset(pid)) {
            let prompt = args.first().filter(|a| !a.trim().is_empty()).cloned();
            return Ok(Plan::Chat {
                prompt,
                system: p.system.clone(),
                provider: p.provider.clone(),
                model: p.model.clone(),
                needs_selection: false,
                temperature: p.temperature,
                max_tokens: p.max_tokens,
                output: AiOutput::Chat,
                preset: Some(p.id.clone()),
            });
        }
        bail!("command not found — try Reload Commands")
    }
}

/// Languages recognised after "to" in `translate … to <language>`.
const LANGUAGES: &[&str] = &[
    "english",
    "spanish",
    "french",
    "german",
    "italian",
    "portuguese",
    "dutch",
    "swedish",
    "norwegian",
    "danish",
    "finnish",
    "polish",
    "czech",
    "slovak",
    "ukrainian",
    "russian",
    "belarusian",
    "bulgarian",
    "serbian",
    "croatian",
    "romanian",
    "hungarian",
    "greek",
    "turkish",
    "arabic",
    "hebrew",
    "persian",
    "hindi",
    "bengali",
    "urdu",
    "thai",
    "vietnamese",
    "indonesian",
    "malay",
    "filipino",
    "chinese",
    "mandarin",
    "cantonese",
    "japanese",
    "korean",
    "georgian",
    "armenian",
    "lithuanian",
    "latvian",
    "estonian",
    "slovenian",
    "catalan",
    "basque",
    "irish",
    "welsh",
    "swahili",
    "latin",
];

/// `translate hola` / `tr hello to german` / `translate to french: hi` →
/// (text, language). The language is only split off when it is a known one,
/// so "translate how to cook" keeps its words.
pub fn parse_translate(q: &str) -> Option<(String, Option<String>)> {
    let lower = q.to_lowercase();
    let rest = ["translate ", "tr ", "tl "]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map(|p| q[p.len()..].trim())?;
    if rest.is_empty() {
        return None;
    }
    let title = |l: &str| {
        let mut c = l.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    };
    // "to french: text"
    if let Some(after) = rest.strip_prefix("to ").or_else(|| rest.strip_prefix("into ")) {
        if let Some((lang, text)) = after.split_once(':') {
            let l = lang.trim().to_lowercase();
            if LANGUAGES.contains(&l.as_str()) && !text.trim().is_empty() {
                return Some((text.trim().to_string(), Some(title(&l))));
            }
        }
    }
    // "text to french"
    let rl = rest.to_lowercase();
    for sep in [" to ", " into ", " in "] {
        if let Some(i) = rl.rfind(sep) {
            let lang = rl[i + sep.len()..].trim();
            if LANGUAGES.contains(&lang) && !rest[..i].trim().is_empty() {
                return Some((rest[..i].trim().to_string(), Some(title(lang))));
            }
        }
    }
    Some((rest.to_string(), None))
}

/// Plain-text notes about the user, prepended to AI chats (`ai_profile`).
pub const PROFILE_FILE: &str = "profile.md";

/// Wrap the profile and a chat's own system prompt into one system message.
pub fn system_with_profile(profile: Option<&str>, system: Option<&str>) -> Option<String> {
    let system = system.map(str::trim).filter(|s| !s.is_empty());
    match (profile, system) {
        (None, s) => s.map(str::to_string),
        (Some(p), None) => Some(format!("About the user (from their profile):\n{p}")),
        (Some(p), Some(s)) => Some(format!("{s}\n\nAbout the user (from their profile):\n{p}")),
    }
}

/// Insert the selected text into an AI command prompt.
pub fn fill_selection(prompt: &str, selection: &str) -> String {
    prompt.replace("{selection}", selection)
}

/// Fill every runtime placeholder: `{selection}`, `{clipboard}`, `{app}`.
/// Missing values become empty strings.
pub fn fill_placeholders(prompt: &str, selection: &str, clipboard: &str, app: &str) -> String {
    prompt
        .replace("{selection}", selection)
        .replace("{clipboard}", clipboard)
        .replace("{app}", app)
}

/// True when the prompt wants the clipboard text (so the shell reads it).
pub fn needs_clipboard(prompt: &str) -> bool {
    prompt.contains("{clipboard}")
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?']).next())
        .unwrap_or(url)
        .to_string()
}

pub use crate::encode::url_encode;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_CONFIG;

    fn engine_with(layouts: Vec<(String, String, usize)>, extra_toml: &str) -> Engine {
        engine_from(&format!("{DEFAULT_CONFIG}\n{extra_toml}"), layouts)
    }

    /// An engine whose installed-app list is exactly `apps` — integrations decide
    /// their own commands from it, so tests need to control it.
    /// An engine whose installed-app list is exactly `apps`. `integrations`
    /// rewrites the line the example config ships, because a second `[general]`
    /// table would not parse.
    fn engine_apps(apps: Vec<AppEntry>, integrations: Option<&str>) -> Engine {
        let mut src = DEFAULT_CONFIG.to_string();
        if let Some(spec) = integrations {
            let before = src.clone();
            src = src.replace("integrations = \"auto\"", &format!("integrations = \"{spec}\""));
            assert_ne!(
                src, before,
                "examples/config.toml lost the integrations line tests rewrite"
            );
        }
        let cfg = Config::parse(&src).unwrap();
        Engine::assemble_with(cfg, "/cfg".into(), apps, vec![], vec![], vec![], Extras::default())
    }

    fn spotify() -> AppEntry {
        AppEntry {
            name: "Spotify".into(),
            path: "/Applications/Spotify.app".into(),
        }
    }

    fn engine_from(src: &str, layouts: Vec<(String, String, usize)>) -> Engine {
        let cfg = Config::parse(src).unwrap();
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

    #[test]
    fn presets_combine_and_the_first_named_wins_a_clash() {
        // The spectacle set claims ⌃⌥⇧←/→ for make-smaller / make-larger, which
        // is also where the space keys live. Whichever preset is named first gets
        // them, and the other says out loud that its command ended up keyless.
        let e = engine_from(
            &DEFAULT_CONFIG.replace(
                "window_hotkeys = \"rectangle\"",
                "window_hotkeys = \"spaces, spectacle\"",
            ),
            vec![],
        );
        let (bindings, _) = e.hotkey_bindings();
        assert!(
            bindings.iter().any(|(_, b)| b == "window:space-next"),
            "the space keys did not bind"
        );
        assert!(
            bindings.iter().any(|(_, b)| b == "window:left-half"),
            "the tiling keys did not bind"
        );
        let lost: Vec<&String> = e.warnings.iter().filter(|w| w.contains("got no key")).collect();
        assert_eq!(lost.len(), 2, "make-larger / make-smaller lost theirs: {lost:?}");
        assert!(lost.iter().all(|w| w.contains("Make")), "{lost:?}");

        let e = engine_from(
            &DEFAULT_CONFIG.replace(
                "window_hotkeys = \"rectangle\"",
                "window_hotkeys = \"spectacle, spaces\"",
            ),
            vec![],
        );
        let (bindings, _) = e.hotkey_bindings();
        assert!(bindings.iter().any(|(_, b)| b == "window:left-half"));
        assert!(
            !bindings.iter().any(|(_, b)| b == "window:space-next"),
            "spectacle holds the shared keys here"
        );
        assert!(
            e.warnings
                .iter()
                .any(|w| w.contains("got no key") && w.contains("Next Space")),
            "{:?}",
            e.warnings
        );
    }

    #[test]
    fn rectangle_and_spaces_do_not_collide() {
        let e = engine_from(
            &DEFAULT_CONFIG.replace(
                "window_hotkeys = \"rectangle\"",
                "window_hotkeys = \"rectangle, spaces\"",
            ),
            vec![],
        );
        let (bindings, _) = e.hotkey_bindings();
        assert!(
            bindings.iter().any(|(_, b)| b == "window:space-next"),
            "the space keys did not bind"
        );
        assert!(
            bindings.iter().any(|(_, b)| b == "window:left-half"),
            "the tiling keys did not bind"
        );
        assert!(e.warnings.iter().all(|w| !w.contains("got no key")), "{:?}", e.warnings);
    }

    #[test]
    fn a_bad_name_in_a_combined_preset_warns() {
        let e = engine_from(
            &DEFAULT_CONFIG.replace(
                "window_hotkeys = \"rectangle\"",
                "window_hotkeys = \"rectangle, spces\"",
            ),
            vec![],
        );
        assert!(e.warnings.iter().any(|w| w.contains("spces")), "{:?}", e.warnings);
    }

    #[test]
    fn the_window_key_preset_resolves_and_a_typo_is_reported() {
        // The default config ships the Rectangle set, so every one of its ids
        // must name a real command (a bad id would warn on every launch).
        let e = engine_with(vec![], "");
        let (bindings, warnings) = e.hotkey_bindings();
        assert!(warnings.is_empty(), "{warnings:?}");
        for (_, id) in crate::config::window_hotkey_preset("rectangle")
            .unwrap()
            .iter()
            .copied()
        {
            assert!(bindings.iter().any(|(_, b)| b == id), "{id} is not bound");
        }
        let e = engine_from(
            &DEFAULT_CONFIG.replace("window_hotkeys = \"rectangle\"", "window_hotkeys = \"retangle\""),
            vec![],
        );
        assert!(
            e.warnings
                .iter()
                .any(|w| w.contains("retangle") && w.contains("`rectangle`")),
            "{:?}",
            e.warnings
        );
    }

    fn engine() -> Engine {
        engine_with(vec![], "")
    }

    #[test]
    fn translate_queries() {
        let p = |q: &str| parse_translate(q);
        assert_eq!(p("translate hola amigo"), Some(("hola amigo".into(), None)));
        assert_eq!(
            p("tr good morning to German"),
            Some(("good morning".into(), Some("German".into())))
        );
        assert_eq!(
            p("translate to french: see you"),
            Some(("see you".into(), Some("French".into())))
        );
        assert_eq!(p("translate how to cook"), Some(("how to cook".into(), None)));
        assert_eq!(p("translate "), None);
        assert_eq!(p("trash"), None);
        let e = engine();
        let row = &e.search("tr bonjour to english", 10)[0];
        assert!(row.id.starts_with("translate:English"));
        match e.plan(&row.id, &[]).unwrap() {
            Plan::Chat {
                prompt,
                system,
                needs_selection,
                ..
            } => {
                assert_eq!(prompt.as_deref(), Some("bonjour"));
                assert!(system.unwrap().contains("into English"));
                assert!(!needs_selection);
            }
            p => panic!("{p:?}"),
        }
        // No text: translate the selection, offered by Act on Selection.
        assert!(matches!(
            e.plan("builtin:translate", &[]).unwrap(),
            Plan::Chat {
                needs_selection: true,
                ..
            }
        ));
        assert!(e.accepting("text").iter().any(|c| c.id == "builtin:translate"));
    }

    #[test]
    fn pipelines_resolve_steps() {
        let e = engine_with(
            vec![],
            r#"
[[pipelines]]
id = "tidy"
title = "Tidy and Fix"
steps = ["trim", "ai:fix-grammar", "prompt:Make it shorter: {input}", "shell:wc -w", "script:/s/uuid.sh", "transform:upper"]
output = "copy"
hotkey = "Ctrl+Alt+P"

[[pipelines]]
id = "q"
title = "Shout Typed Text"
input = "query"
steps = ["upper"]

[[pipelines]]
id = "bad"
title = "Bad"
steps = ["ai:nope"]
"#,
        );
        let c = e.registry.resolve("Tidy and Fix").unwrap();
        assert_eq!(c.id, "pipe:tidy");
        assert_eq!(c.hotkey.as_deref(), Some("Ctrl+Alt+P"));
        assert!(c.subtitle.as_deref().unwrap().contains("trim → fix-grammar"));
        match e.plan("pipe:tidy", &[]).unwrap() {
            Plan::Pipeline {
                steps, output, input, ..
            } => {
                assert_eq!(steps.len(), 6);
                assert!(
                    matches!(&steps[1], crate::pipeline::Step::Ai { prompt, .. } if prompt.contains("{selection}"))
                );
                assert!(matches!(&steps[4], crate::pipeline::Step::Script(m) if m.path == Path::new("/s/uuid.sh")));
                assert_eq!(output, crate::pipeline::Output::Copy);
                assert_eq!(input, crate::pipeline::Input::Selection);
            }
            p => panic!("{p:?}"),
        }
        // Typed input → an argument; selection pipelines show up in Act on Selection.
        assert_eq!(e.registry.resolve("pipe:q").unwrap().arguments.len(), 1);
        assert!(e.accepting("text").iter().any(|c| c.id == "pipe:tidy"));
        assert!(!e.accepting("text").iter().any(|c| c.id == "pipe:q"));
        let err = format!("{:#}", e.plan("pipe:bad", &[]).unwrap_err());
        assert!(err.contains("no AI command `nope`"), "{err}");
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
    fn space_commands_plan_search_and_share_a_glyph() {
        let e = engine_with(vec![], "");
        for (id, a) in [
            ("window:space-next", WindowAction::SpaceNext),
            ("window:space-prev", WindowAction::SpacePrev),
            ("window:space-move-next", WindowAction::SpaceMoveNext),
            ("window:space-move-prev", WindowAction::SpaceMovePrev),
            ("window:spaces", WindowAction::SpaceSwitcher),
        ] {
            assert_eq!(e.plan(id, &[]).unwrap(), Plan::Window(a), "{id}");
            assert!(e.registry.get(id).is_some(), "{id} has no row");
        }
        let found = |q: &str, id: &str| {
            assert!(
                e.search(q, 12).iter().any(|c| c.id == id),
                "'{q}' missed {id}: {:?}",
                e.search(q, 12).iter().map(|c| c.id.as_str()).collect::<Vec<_>>()
            )
        };
        found("switch space", "window:spaces");
        found("move window to next space", "window:space-move-next");
        found("previous space", "window:space-prev");
        // One glyph for the family; the title carries the verb.
        let icons: Vec<String> = e
            .search("space", 20)
            .iter()
            .filter(|c| c.id.starts_with("window:space"))
            .map(|c| c.icon.clone().unwrap_or_default())
            .collect();
        assert!(icons.len() >= 4, "only {icons:?}");
        assert!(icons.iter().all(|i| i == "🗔"), "{icons:?}");
        assert!(WindowAction::SpaceMoveNext.is_space_move());
        assert!(!WindowAction::SpaceNext.is_space_move());
    }

    #[test]
    fn spotify_commands_plan_search_and_share_a_glyph() {
        let e = engine_apps(
            vec![
                AppEntry {
                    name: "Safari".into(),
                    path: "/Applications/Safari.app".into(),
                },
                spotify(),
            ],
            None,
        );
        let ids = [
            "builtin:spotify",
            "builtin:spotify-play",
            "builtin:spotify-next",
            "builtin:spotify-previous",
            "builtin:spotify-now",
        ];
        for id in ids {
            assert!(e.registry.get(id).is_some(), "{id} has no row");
        }
        assert_eq!(e.plan("builtin:spotify", &[]).unwrap(), Plan::Ui("spotify"));
        assert_eq!(e.plan("builtin:spotify-play", &[]).unwrap(), Plan::Ui("spotify-play"));
        assert_eq!(e.plan("builtin:spotify-next", &[]).unwrap(), Plan::Ui("spotify-next"));
        assert_eq!(
            e.plan("builtin:spotify-previous", &[]).unwrap(),
            Plan::Ui("spotify-previous")
        );
        assert_eq!(e.plan("builtin:spotify-now", &[]).unwrap(), Plan::Ui("spotify-now"));
        let found = |q: &str, id: &str| {
            assert!(
                e.search(q, 12).iter().any(|c| c.id == id),
                "'{q}' missed {id}: {:?}",
                e.search(q, 12).iter().map(|c| c.id.as_str()).collect::<Vec<_>>()
            )
        };
        found("spotify", "builtin:spotify");
        found("spotify now playing", "builtin:spotify-now");
        found("what is playing", "builtin:spotify-now");
        found("spotify next", "builtin:spotify-next");
        found("spotify search", "builtin:spotify-search");
        // One glyph for the family; the subtitle carries the home.
        let spotify_rows: Vec<_> = e
            .search("spotify", 20)
            .into_iter()
            .filter(|c| c.id.starts_with("builtin:spotify"))
            .collect();
        assert!(
            spotify_rows.len() >= 4,
            "only {:?}",
            spotify_rows.iter().map(|c| &c.id).collect::<Vec<_>>()
        );
        assert!(spotify_rows.iter().all(|c| c.icon.as_deref() == Some("🎧")));
        assert!(spotify_rows
            .iter()
            .all(|c| c.subtitle.as_deref() == Some("Spotify") || c.id == "builtin:spotify"));
        assert_eq!(
            e.registry.get("builtin:spotify").and_then(|c| c.subtitle.as_deref()),
            Some("Player Controls")
        );
        assert_eq!(
            e.registry.get("builtin:spotify-play").map(|c| c.title.as_str()),
            Some("Play / Pause")
        );
        // The media-key family stays: it drives whatever owns Now Playing.
        assert!(e.registry.get("builtin:media-play").is_some());
        // Typing "spotify" should not also surface the system media-key Play / Pause.
        assert!(
            !e.search("spotify", 20).iter().any(|c| c.id == "builtin:media-play"),
            "media-play should not match 'spotify'"
        );
    }

    #[test]
    fn an_integration_turns_itself_on_when_its_app_is_installed() {
        let e = engine_apps(vec![spotify()], None);
        assert_eq!(e.integrations_on, vec!["spotify"]);
        assert!(e.registry.get("builtin:spotify-search").is_some());
        assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    }

    #[test]
    fn switching_an_integration_off_drops_its_whole_command_family() {
        let e = engine_apps(vec![spotify()], Some("off"));
        assert!(e.integrations_on.is_empty());
        for key in [
            "spotify",
            "spotify-play",
            "spotify-next",
            "spotify-previous",
            "spotify-now",
            "spotify-search",
        ] {
            assert!(e.registry.get(&format!("builtin:{key}")).is_none(), "{key} survived");
        }
        // Media keys are not an integration: they work without any one app.
        assert!(e.registry.get("builtin:media-play").is_some());
        assert!(e
            .search("spotify", 20)
            .iter()
            .all(|c| !c.id.starts_with("builtin:spotify")));
    }

    #[test]
    fn an_integration_named_without_its_app_says_so() {
        let e = engine_apps(vec![], Some("spotify"));
        assert!(e.integrations_on.is_empty());
        assert!(
            e.warnings.iter().any(|w| w.contains("Spotify is not installed")),
            "{:?}",
            e.warnings
        );
    }

    #[test]
    fn a_typo_in_the_integration_list_is_not_silence() {
        let e = engine_apps(vec![spotify()], Some("spotipy"));
        assert!(
            e.warnings
                .iter()
                .any(|w| w.contains("not a known integration") && w.contains("spotify")),
            "{:?}",
            e.warnings
        );
    }

    #[test]
    fn auto_can_subtract_one_integration() {
        let e = engine_apps(vec![spotify()], Some("auto, -spotify"));
        assert!(e.integrations_on.is_empty());
        assert!(e.registry.get("builtin:spotify").is_none());
    }

    #[test]
    fn the_app_row_carries_the_integration_verbs() {
        let e = engine_apps(vec![spotify()], None);
        let acts = e
            .actions_for("app:/Applications/Spotify.app")
            .expect("the Spotify app row has no actions");
        let ids: Vec<&str> = acts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "spotify",
                "spotify-play",
                "spotify-previous",
                "spotify-next",
                "spotify-now",
                "spotify-search"
            ]
        );
        assert_eq!(acts[1].title, "Play / Pause");
        // Off means off: no commands, and no verbs on the app row either.
        let off = engine_apps(vec![spotify()], Some("off"));
        assert!(off.actions_for("app:/Applications/Spotify.app").is_none());
        // Ordinary rows get nothing.
        assert!(e.actions_for("builtin:reload").is_none());
    }

    #[test]
    fn spotify_search_hands_the_query_to_the_app() {
        let e = engine_apps(vec![spotify()], None);
        assert_eq!(
            e.plan("builtin:spotify-search", &["korol i shut".into()]).unwrap(),
            Plan::OpenUrl("spotify:search:korol%20i%20shut".into())
        );
        // Non-ASCII survives as UTF-8 percent escapes, which the app decodes.
        assert_eq!(
            e.plan("builtin:spotify-search", &["сектор".into()]).unwrap(),
            Plan::OpenUrl("spotify:search:%D1%81%D0%B5%D0%BA%D1%82%D0%BE%D1%80".into())
        );
        // Choosing a list row re-runs with the URI → open that URI.
        assert_eq!(
            e.plan("builtin:spotify-search", &["spotify:track:abc".into()]).unwrap(),
            Plan::OpenUrl("spotify:track:abc".into())
        );
        // The row asks for the query instead of running with none.
        let row = e.registry.get("builtin:spotify-search").unwrap();
        assert_eq!(row.arguments.len(), 1);
        assert!(!row.arguments[0].optional);
    }

    #[test]
    fn spotify_search_list_mode_plans_ui_args_when_creds_resolve() {
        let mut src = DEFAULT_CONFIG.to_string();
        src = src.replace("integrations = \"auto\"", "integrations = \"spotify\"");
        src.push_str("\n[spotify]\nsearch = \"list\"\nclient_id = \"cid\"\nclient_secret_cmd = \"printf secret\"\n");
        let cfg = Config::parse(&src).unwrap();
        let e = Engine::assemble_with(
            cfg,
            "/cfg".into(),
            vec![spotify()],
            vec![],
            vec![],
            vec![],
            Extras::default(),
        );
        assert_eq!(
            e.plan("builtin:spotify-search", &["rammstein mutter".into()]).unwrap(),
            Plan::UiArgs {
                action: "spotify-search-list",
                args: vec!["rammstein mutter".into()],
            }
        );
    }

    #[test]
    fn plans_window_and_layout_commands() {
        let e = engine_with(vec![("work".into(), "Work".into(), 3)], "");
        assert_eq!(
            e.plan("window:left-half", &[]).unwrap(),
            Plan::Window(WindowAction::LeftHalf)
        );
        // Full screen has an enter and an exit, not only a toggle.
        for (id, a) in [
            ("window:fullscreen", WindowAction::ToggleFullscreen),
            ("window:full-screen", WindowAction::Fullscreen),
            ("window:exit-fullscreen", WindowAction::ExitFullscreen),
        ] {
            assert_eq!(e.plan(id, &[]).unwrap(), Plan::Window(a), "{id}");
            assert!(e.registry.get(id).is_some(), "{id} has no row");
        }
        assert_eq!(e.search("exit full screen", 5)[0].id, "window:exit-fullscreen");
        assert_eq!(e.search("full screen", 3)[0].kind, CommandKind::Window);
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
        let has = |k: &str, id: &str| bindings.iter().any(|(bk, bi)| bk == k && bi == id);
        assert!(has("Ctrl+Alt+Left", "window:left-half"), "{bindings:?}");
        assert!(has("Cmd+Alt+S", "app:/Applications/Safari.app"));
        assert!(has("Cmd+Alt+G", "ai:fix-grammar"));
        // The default Act on Selection hotkey.
        assert!(has("Hyper+Space", "builtin:selection"));
        assert_eq!(
            bindings.iter().filter(|(_, id)| id == "window:left-half").count(),
            1,
            "your binding replaces the preset one instead of adding a second"
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
    fn command_overrides_alias_rename_hide() {
        let e = engine_with(
            vec![],
            r#"
            [[commands]]
            match = "Safari"
            alias = "sf"
            title = "Safari (web)"
            icon = "safari"
            boost = 10

            [[commands]]
            match = "builtin:shortcuts"
            hidden = true

            [[commands]]
            match = "No Such Command"
            alias = "x"
            "#,
        );
        assert!(e.warnings.iter().any(|w| w.contains("No Such Command")));
        let safari = e.registry.get("app:/Applications/Safari.app").unwrap();
        assert_eq!(safari.title, "Safari (web)");
        assert_eq!(safari.icon.as_deref(), Some("safari"));
        assert_eq!(e.search("sf", 5)[0].id, "app:/Applications/Safari.app");
        assert!(e.search("shortcuts", 20).iter().all(|c| c.id != "builtin:shortcuts"));
        assert!(e.search("", 200).iter().all(|c| c.id != "builtin:shortcuts"));
    }

    #[test]
    fn fallbacks_follow_config_order() {
        // The default config already has [general]; edit its fallbacks line rather than add a second table.
        let src = DEFAULT_CONFIG.replace(
            "fallbacks = [\"quicklinks\", \"files\"]",
            "fallbacks = [\"Search Kagi\", \"builtin:chat\", \"files\"]",
        );
        assert_ne!(
            src, DEFAULT_CONFIG,
            "default config fallbacks line changed; update this test"
        );
        let cfg = Config::parse(&src).unwrap();
        let e = Engine::assemble(cfg, "/cfg".into(), vec![], vec![], vec![], vec![]);
        assert!(e.fallback_files());
        let rows = e.search("zzqqxx", 10);
        assert_eq!(rows[0].id, "link:search-kagi");
        assert_eq!(rows[1].id, "builtin:chat");
        assert_eq!(rows.len(), 2);
        // Default config: every {query} quicklink.
        let d = engine();
        let rows = d.search("zzqqxx", 10);
        assert!(rows.iter().any(|c| c.id == "link:search-github"));
        assert!(rows.iter().any(|c| c.id == "link:search-kagi"));
    }

    #[test]
    fn web_fallback_injects_and_plans() {
        let src = DEFAULT_CONFIG.replace("fallbacks = [\"quicklinks\", \"files\"]", "fallbacks = [\"web\"]");
        let cfg = Config::parse(&src).unwrap();
        let e = Engine::assemble(cfg, "/cfg".into(), vec![], vec![], vec![], vec![]);
        let rows = e.search("zzqqxx", 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "fallback:web");
        assert_eq!(rows[0].title, "Search the Web");
        assert_eq!(rows[0].kind, CommandKind::Quicklink);
        assert_eq!(
            e.plan("fallback:web", &["rust & tauri".into()]).unwrap(),
            Plan::OpenUrl("https://duckduckgo.com/?q=rust+%26+tauri".into())
        );
        assert!(e.plan("fallback:web", &[]).is_err());
    }

    #[test]
    fn sidecars_live_in_memory_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        let apps = vec![AppEntry {
            name: "Safari".into(),
            path: "/Applications/Safari.app".into(),
        }];
        let e = Engine::assemble(cfg, dir.path().to_path_buf(), apps, vec![], vec![], vec![]);
        assert!(e.toggle_pin("window:left-half"));
        assert!(e.is_pinned("window:left-half"));
        assert_eq!(e.search("", 5)[0].id, "window:left-half");
        e.bump_usage("window:maximize", "max");
        e.bump_usage("window:maximize", "max");
        let note = e.add_note("Todo", "ship it");
        assert!(e.search("note ship", 20).iter().any(|c| c.id == note.id));
        assert_eq!(
            e.plan(&note.id, &[]).unwrap(),
            Plan::UiArgs {
                action: "note-editor",
                args: vec![note.id.clone()]
            }
        );
        // Plain words find notes too, after the commands.
        assert!(e.search("ship", 20).iter().any(|c| c.id == note.id));
        // A fresh engine reads the same state back from disk.
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        let e2 = Engine::assemble(cfg, dir.path().to_path_buf(), vec![], vec![], vec![], vec![]);
        assert!(e2.is_pinned("window:left-half"));
        assert_eq!(crate::usage::counts(dir.path()).get("window:maximize"), Some(&2));
        assert!(e2.search("notes", 20).iter().any(|c| c.id == note.id));
    }

    #[test]
    fn ai_options_and_placeholders() {
        let e = engine_with(
            vec![],
            r#"
            [[ai_commands]]
            id = "clip"
            title = "Tidy Clipboard"
            prompt = "Tidy this from {app}:\n{clipboard}"
            temperature = 0.3
            max_tokens = 200
            output = "paste"
            "#,
        );
        match e.plan("ai:clip", &[]).unwrap() {
            Plan::Chat {
                prompt,
                needs_selection,
                temperature,
                max_tokens,
                output,
                ..
            } => {
                assert!(!needs_selection);
                assert_eq!(temperature, Some(0.3));
                assert_eq!(max_tokens, Some(200));
                assert_eq!(output, AiOutput::Paste);
                let p = prompt.unwrap();
                assert!(needs_clipboard(&p));
                assert_eq!(
                    fill_placeholders(&p, "", "x = 1", "Xcode"),
                    "Tidy this from Xcode:\nx = 1"
                );
            }
            p => panic!("unexpected {p:?}"),
        }
        assert_eq!(e.keymap.iter().find(|k| k.name == "actions").unwrap().key, "k");
    }

    #[test]
    fn quit_rows_are_sorted_and_resolve() {
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        let apps = vec![
            AppEntry {
                name: "Zed".into(),
                path: "/Applications/Zed.app".into(),
            },
            AppEntry {
                name: "Arc".into(),
                path: "/Applications/Arc.app".into(),
            },
        ];
        let e = Engine::assemble(cfg, "/cfg".into(), apps, vec![], vec![], vec![]);
        let rows: Vec<String> = e
            .search("quit", 20)
            .into_iter()
            .filter(|c| c.id.starts_with("quit:"))
            .map(|c| c.title)
            .collect();
        assert_eq!(rows, vec!["Quit Arc", "Quit Zed"]);
        assert_eq!(
            e.plan("quit:app:/Applications/Arc.app", &[]).unwrap(),
            Plan::QuitApp("Arc".into())
        );
    }

    fn mcp_extras() -> Extras {
        let mut mcp_tools = crate::mcp::ToolCache::new();
        mcp_tools.insert(
            "gh".into(),
            vec![
                crate::mcp::McpTool {
                    server: "gh".into(),
                    name: "create_issue".into(),
                    title: None,
                    description: "Create a GitHub issue".into(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {"repo": {"type": "string"}, "title": {"type": "string"}},
                        "required": ["repo", "title"]
                    }),
                    read_only: false,
                },
                crate::mcp::McpTool {
                    server: "gh".into(),
                    name: "get_me".into(),
                    title: None,
                    description: "".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    read_only: true,
                },
            ],
        );
        // The same server also exposes a document and a prompt template.
        let mut mcp_extras = crate::mcp::ExtrasCache::default();
        mcp_extras.resources.insert(
            "gh".into(),
            vec![crate::mcp::McpResource {
                server: "gh".into(),
                uri: "issues://opencast/open".into(),
                name: "Open issues".into(),
                description: "Every issue still open in opencast".into(),
                mime: "text/markdown".into(),
            }],
        );
        mcp_extras.prompts.insert(
            "gh".into(),
            vec![crate::mcp::McpPrompt {
                server: "gh".into(),
                name: "triage_issue".into(),
                title: Some("Triage an issue".into()),
                description: "Summarizes and labels one issue".into(),
                arguments: vec![crate::mcp::McpPromptArg {
                    name: "number".into(),
                    description: "issue number".into(),
                    required: true,
                }],
            }],
        );
        Extras {
            shortcuts: vec!["Send Arrival Message".into(), "Resize Image".into()],
            mcp_tools,
            mcp_extras,
            bookmarks: vec![crate::bookmarks::Bookmark {
                title: "Grafana Prod".into(),
                url: "https://grafana.example.com".into(),
                source: "Chrome".into(),
                folder: String::new(),
            }],
            history: vec![crate::history::Visit {
                title: "Rust Lang".into(),
                url: "https://www.rust-lang.org/".into(),
                source: "Chrome".into(),
            }],
            recents: vec![crate::recents::Visit {
                path: "/Users/me/Documents/notes.md".into(),
                name: "notes.md".into(),
                last_used: None,
            }],
            rates: Some(crate::currency::Rates {
                base: "USD".into(),
                rates: [("EUR".to_string(), 0.5)].into_iter().collect(),
                fetched: 1,
                date: String::new(),
            }),
        }
    }

    #[test]
    fn accepting_commands_and_action_ranking() {
        let e = engine_with_extras(
            r#"
            [[ai_commands]]
            id = "summarize-url"
            title = "Summarize Page"
            prompt = "Summarize {input}"
            accepts = ["url"]
            "#,
        );
        let text: Vec<String> = e.accepting("text").into_iter().map(|c| c.id).collect();
        assert!(
            text.contains(&"ai:fix-grammar".to_string()),
            "{{selection}} prompts accept text"
        );
        assert!(!text.contains(&"ai:summarize-url".to_string()));
        assert_eq!(e.accepting("url")[0].id, "ai:summarize-url");
        // Ranking writes usage.json: use a throwaway config dir.
        let dir = tempfile::tempdir().unwrap();
        let e = Engine::load_with_apps(dir.path(), vec![]);
        let ids: Vec<String> = ["ua:copy", "ua:open", "ua:reveal"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(e.rank_ids("ua:file", &ids), ids, "no history keeps the default order");
        e.bump_usage("ua:reveal", "ua:file");
        e.bump_usage("ua:reveal", "ua:file");
        assert_eq!(e.rank_ids("ua:file", &ids)[0], "ua:reveal");
    }

    #[test]
    fn conversions_panes_bookmarks_and_define() {
        let e = engine_with_extras("");
        let top = &e.search("10 usd to eur", 5)[0];
        assert_eq!(top.title, "5.00 EUR");
        assert_eq!(e.plan(&top.id, &[]).unwrap(), Plan::Copy("5.00".into()));
        assert_eq!(e.search("bluetooth", 5)[0].id, "pane:bluetooth");
        assert_eq!(
            e.plan("pane:bluetooth", &[]).unwrap(),
            Plan::OpenUrl("x-apple.systempreferences:com.apple.BluetoothSettings".into())
        );
        let bm = e.search("bm graf", 10);
        assert_eq!(bm[0].id, "bm:https://grafana.example.com");
        assert!(e.search("grafana", 10).iter().any(|c| c.id.starts_with("bm:")));
        let hist = e.search("hist rust", 10);
        assert_eq!(hist[0].id, "hist:https://www.rust-lang.org/");
        assert!(hist[0].subtitle.as_ref().unwrap().starts_with("History · Chrome"));
        assert_eq!(
            e.plan(&hist[0].id, &[]).unwrap(),
            Plan::OpenUrl("https://www.rust-lang.org/".into())
        );
        let d = &e.search("define serendipity", 5)[0];
        assert_eq!(
            e.plan(&d.id, &[]).unwrap(),
            Plan::UiArgs {
                action: "define",
                args: vec!["serendipity".into()]
            }
        );
        if std::path::Path::new("/usr/share/zoneinfo/Asia/Tokyo").exists() {
            assert!(e.search("time in tokyo", 5)[0].title.contains("Tokyo"));
        }
    }

    #[test]
    fn an_empty_bookmark_cache_says_why() {
        let mut e = engine_with(vec![], "");
        e.bookmark_scan = Default::default();
        match e.plan("builtin:bookmarks", &[]).unwrap() {
            Plan::ShowText { title, text } => {
                assert_eq!(title, "Bookmarks");
                assert!(text.contains("No browser bookmark store"), "{text}");
            }
            other => panic!("{other:?}"),
        }
        // Stores that exist but cannot be opened are a permission, not a bug.
        e.bookmark_scan.blocked = vec!["Chrome".into(), "Safari".into()];
        match e.plan("builtin:bookmarks", &[]).unwrap() {
            Plan::ShowText { text, .. } => assert!(
                text.contains("Full Disk Access") && text.contains("Chrome") && text.contains("Safari"),
                "{text}"
            ),
            other => panic!("{other:?}"),
        }
        // Cached bookmarks keep the old behaviour.
        assert_eq!(
            engine_with_extras("").plan("builtin:bookmarks", &[]).unwrap(),
            Plan::FillQuery("bm ".into())
        );
    }

    fn engine_with_extras(extra_toml: &str) -> Engine {
        let mut cfg = Config::parse(&format!("{DEFAULT_CONFIG}\n{extra_toml}")).unwrap();
        cfg.general.currency = true;
        Engine::assemble_with(cfg, "/cfg".into(), vec![], vec![], vec![], vec![], mcp_extras())
    }

    const GH: &str = r#"
        [[mcp_servers]]
        id = "gh"
        command = "npx"
        args = ["-y", "@modelcontextprotocol/server-github"]
        auto_approve = ["get_*"]
    "#;

    #[test]
    fn shortcuts_become_commands() {
        let e = engine_with_extras("");
        let hit = &e.search("arrival", 5)[0];
        assert_eq!(hit.id, "sc:Send Arrival Message");
        assert_eq!(hit.kind, CommandKind::Shortcut);
        assert_eq!(
            e.plan("sc:Resize Image", &["/tmp/a.png".into()]).unwrap(),
            Plan::RunShortcut {
                name: "Resize Image".into(),
                input: Some("/tmp/a.png".into())
            }
        );
        // Turned off in config: gone.
        let cfg = Config::parse(&DEFAULT_CONFIG.replacen("shortcuts = true", "shortcuts = false", 1)).unwrap();
        let off = Engine::assemble_with(cfg, "/cfg".into(), vec![], vec![], vec![], vec![], mcp_extras());
        assert!(off.registry.get("sc:Resize Image").is_none());
    }

    #[test]
    fn mcp_tools_become_commands_and_chat_tools() {
        // Tools of servers that are not configured are ignored.
        assert!(engine_with_extras("").registry.get("mcp:gh:create_issue").is_none());
        let e = engine_with_extras(GH);
        let c = e.registry.get("mcp:gh:create_issue").unwrap();
        assert_eq!(c.title, "Create Issue");
        assert_eq!(c.kind, CommandKind::Mcp);
        assert_eq!(c.arguments.len(), 2);
        assert_eq!(c.arguments[0].placeholder, "repo");
        assert_eq!(e.search("create issue", 5)[0].id, "mcp:gh:create_issue");
        match e.plan("mcp:gh:create_issue", &["me/x".into(), "Bug".into()]).unwrap() {
            Plan::McpCall {
                server,
                tool,
                arguments,
            } => {
                assert_eq!(server, "gh");
                assert_eq!(tool, "create_issue");
                assert_eq!(arguments["title"], "Bug");
            }
            p => panic!("{p:?}"),
        }
        assert!(e.plan("mcp:gh:create_issue", &["me/x".into()]).is_err());
        let tools = e.chat_tools();
        assert_eq!(tools.len(), 2);
        let me = tools.iter().find(|t| t.tool == "get_me").unwrap();
        assert!(me.auto_approve);
        assert_eq!(me.spec.name, "gh__get_me");
        assert!(!tools.iter().find(|t| t.tool == "create_issue").unwrap().auto_approve);
        assert!(e
            .preview("mcp:gh:create_issue")
            .unwrap()
            .text
            .unwrap()
            .contains("`repo`"));
    }

    #[test]
    fn mcp_resources_and_prompts_become_commands() {
        let e = engine_with_extras(GH);

        // A resource is a row you can press Return on: it reads the document.
        let res_id = "mcp:gh:res:issues://opencast/open";
        let c = e.registry.get(res_id).unwrap();
        assert_eq!(c.title, "Open issues");
        assert_eq!(c.kind, CommandKind::McpResource);
        assert_eq!(c.icon.as_deref(), Some("🔌"));
        // Same glyph as the tools, so the subtitle carries the kind.
        assert_eq!(
            c.subtitle.as_deref(),
            Some("gh · text/markdown · Every issue still open in opencast")
        );
        assert_eq!(e.search("open issues", 5)[0].id, res_id);
        assert_eq!(
            e.plan(res_id, &[]).unwrap(),
            Plan::McpReadResource {
                server: "gh".into(),
                uri: "issues://opencast/open".into(),
            }
        );
        assert!(e.preview(res_id).unwrap().facts.iter().any(|(k, _)| k == "URI"));

        // A prompt is a row that opens a chat with the server's own wording.
        let prompt_id = "mcp:gh:prompt:triage_issue";
        let c = e.registry.get(prompt_id).unwrap();
        assert_eq!(c.title, "Triage an issue");
        assert_eq!(c.kind, CommandKind::McpPrompt);
        assert_eq!(c.icon.as_deref(), Some("🔌"));
        assert_eq!(
            c.subtitle.as_deref(),
            Some("gh · prompt · Summarizes and labels one issue")
        );
        assert_eq!(c.arguments[0].placeholder, "number");
        match e.plan(prompt_id, &["42".into()]).unwrap() {
            Plan::McpGetPrompt {
                server,
                name,
                arguments,
            } => {
                assert_eq!(server, "gh");
                assert_eq!(name, "triage_issue");
                assert_eq!(arguments["number"], "42");
            }
            p => panic!("{p:?}"),
        }
        // A missing required argument explains itself.
        let err = e.plan(prompt_id, &[]).unwrap_err().to_string();
        assert!(err.contains("needs 1 argument(s): number"), "{err}");
        assert!(e
            .preview(prompt_id)
            .unwrap()
            .text
            .unwrap()
            .contains("`number`, required"));

        // A server not in config contributes nothing, cached or not.
        assert!(engine_with_extras("").registry.get(res_id).is_none());
    }

    #[test]
    fn saved_chats_search_resume_and_delete() {
        use crate::providers::{Message, Role};
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        let e = Engine::assemble(cfg, dir.path().to_path_buf(), vec![], vec![], vec![], vec![]);
        e.save_chat(crate::chats::SavedChat {
            id: "chat:1".into(),
            title: String::new(),
            updated: 1,
            provider: Some("ollama".into()),
            model: Some("llama3.2".into()),
            system: None,
            messages: vec![
                Message::new(Role::User, "explain borrow checker"),
                Message::new(Role::Assistant, "…"),
            ],
        });
        let rows = e.search("chats borrow", 10);
        assert_eq!(rows[0].id, "chat:1");
        assert_eq!(rows[0].title, "explain borrow checker");
        match e.plan("chat:1", &[]).unwrap() {
            Plan::ResumeChat(c) => assert_eq!(c.model.as_deref(), Some("llama3.2")),
            p => panic!("{p:?}"),
        }
        assert!(e.preview("chat:1").unwrap().text.unwrap().contains("**You**"));
        assert_eq!(
            e.plan("builtin:chat-history", &[]).unwrap(),
            Plan::FillQuery("chats".into())
        );
        assert!(e.delete_chat("chat:1"));
        assert!(e.plan("chat:1", &[]).is_err());
    }

    #[test]
    fn ui_builtins_and_snippet_expansion() {
        let e = engine_with(
            vec![],
            r#"
            [[snippets]]
            keyword = ";addr"
            text = "1 Main St"
            [[snippets]]
            keyword = "plain"
            text = "never auto"
            [[snippets]]
            keyword = "forced"
            text = "yes"
            auto_expand = true
            "#,
        );
        assert_eq!(e.plan("builtin:menu-items", &[]).unwrap(), Plan::Ui("menu-items"));
        assert_eq!(e.plan("builtin:settings", &[]).unwrap(), Plan::Ui("settings"));
        assert_eq!(e.plan("builtin:dictate", &[]).unwrap(), Plan::Ui("dictate"));
        assert_eq!(e.plan("builtin:schedule", &[]).unwrap(), Plan::Ui("schedule"));
        assert_eq!(e.plan("builtin:appearance", &[]).unwrap(), Plan::Ui("appearance-menu"));
        assert_eq!(
            e.plan("builtin:panel-position", &[]).unwrap(),
            Plan::Ui("panel-position-menu")
        );
        assert_eq!(e.plan("builtin:stay-awake", &[]).unwrap(), Plan::Ui("stay-awake-menu"));
        assert_eq!(e.search("OpenCast Keys", 3)[0].id, "builtin:shortcuts");
        assert_eq!(e.search("App Menu Shortcuts", 3)[0].id, "builtin:cheat-sheet");
        assert!(e
            .search("Appearance: Dark", 5)
            .iter()
            .all(|c| c.id != "builtin:theme-dark"));
        assert_eq!(e.search("Appearance", 5)[0].id, "builtin:appearance");
        let exp = e.expanding_snippets();
        let keys: Vec<&str> = exp.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&";addr") && keys.contains(&"forced"));
        assert!(!keys.contains(&"plain") && !keys.contains(&"sig"));
        assert_eq!(e.preview("snippet:;addr").unwrap().text.as_deref(), Some("1 Main St"));
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
