//! The command registry and fuzzy search over it.
//!
//! Search runs on every keystroke, so the registry precomputes everything it
//! can (lowercased titles and keywords, an id index) and does no I/O.

use std::collections::{HashMap, HashSet};

use crate::usage::Usage;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandKind {
    Builtin,
    Ai,
    Script,
    Quicklink,
    App,
    Calculator,
    Window,
    Layout,
    File,
    Snippet,
    Clipboard,
    /// A Shortcuts.app shortcut.
    Shortcut,
    /// A tool from an MCP server.
    Mcp,
    /// A document an MCP server exposes (`resources/list`).
    McpResource,
    /// A prompt template an MCP server exposes (`prompts/list`).
    McpPrompt,
}

impl CommandKind {
    /// Tie-breaker when scores are equal, and order for an empty query.
    fn rank(self) -> u8 {
        match self {
            Self::Calculator => 0,
            Self::Clipboard => 1,
            Self::App => 2,
            Self::Ai => 3,
            Self::Snippet => 4,
            Self::Script => 5,
            Self::Quicklink => 6,
            Self::File => 7,
            Self::Shortcut => 6,
            Self::Mcp => 7,
            // Same tier as the tools they come from: rare, but real commands.
            Self::McpResource => 7,
            Self::McpPrompt => 7,
            Self::Window => 8,
            Self::Layout => 9,
            Self::Builtin => 10,
        }
    }

    /// Category names Raycast-style search promotes when the query looks like a type.
    fn category_aliases(self) -> &'static [&'static str] {
        match self {
            Self::Window => &["window", "windows", "snap", "tile", "wm", "rectangle"],
            Self::Layout => &["layout", "layouts", "workspace", "arrangement"],
            Self::Ai => &["ai", "llm"],
            Self::App => &["app", "apps", "application", "applications"],
            Self::Script => &["script", "scripts"],
            Self::Quicklink => &["quicklink", "quicklinks"],
            Self::Builtin => &["builtin"],
            Self::Calculator => &["calc", "calculator"],
            Self::File => &["file", "files", "find", "document", "documents"],
            Self::Snippet => &["snippet", "snippets", "expand"],
            Self::Clipboard => &["clip", "clipboard", "paste", "cb"],
            Self::Shortcut => &["shortcut", "shortcuts"],
            Self::Mcp => &["mcp", "tool", "tools"],
            Self::McpResource => &["mcp", "resource", "resources", "file", "document"],
            Self::McpPrompt => &["mcp", "prompt", "prompts", "template"],
        }
    }

    /// Large boost when `ql` (already lowercased + trimmed) looks like a type name.
    fn category_boost(self, ql: &str) -> i64 {
        if ql.len() < 2 {
            return 0;
        }
        for alias in self.category_aliases() {
            // "windo" → window; require the alias to grow from the query (not "sett" → settings-on-builtin).
            if alias.starts_with(ql) {
                return 400;
            }
            // Full alias typed, maybe with more: "window left"
            if ql.len() >= alias.len() && ql.starts_with(alias) {
                return 400;
            }
        }
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Argument {
    pub placeholder: String,
    pub optional: bool,
    pub secure: bool,
}

/// A ⌘-menu action the panel offers on a command row. Integrations attach these
/// to the app row they drive, so the Spotify row carries Play / Pause and Now
/// Playing instead of making the user hunt for them in the command list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub kind: CommandKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<Argument>,
    /// Global hotkey bound to this command, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(skip)]
    pub keywords: Vec<String>,
}

/// Per-command tweaks from `[[commands]]` in the config.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Override {
    /// Lowercased. An exact alias match outranks everything.
    pub aliases: Vec<String>,
    pub boost: i64,
    pub hidden: bool,
}

/// Precomputed search material for one command.
struct Entry {
    title_lower: String,
    /// First letters of the title's words (`Visual Studio Code` → `vsc`, `iTerm` → `it`).
    initials: String,
    keywords_lower: Vec<String>,
    aliases: Vec<String>,
    boost: i64,
    hidden: bool,
}

pub struct Registry {
    commands: Vec<Command>,
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    matcher: SkimMatcherV2,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("commands", &self.commands.len())
            .finish()
    }
}

impl Registry {
    pub fn new(commands: Vec<Command>) -> Self {
        Self::with_overrides(commands, &HashMap::new())
    }

    pub fn with_overrides(commands: Vec<Command>, overrides: &HashMap<String, Override>) -> Self {
        let entries = commands
            .iter()
            .map(|c| {
                let o = overrides.get(&c.id).cloned().unwrap_or_default();
                Entry {
                    title_lower: c.title.to_lowercase(),
                    initials: initials(&c.title),
                    keywords_lower: c.keywords.iter().map(|k| k.to_lowercase()).collect(),
                    aliases: o.aliases,
                    boost: o.boost,
                    hidden: o.hidden,
                }
            })
            .collect();
        let index = commands.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
        Self {
            commands,
            entries,
            index,
            matcher: SkimMatcherV2::default().ignore_case(),
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.index.get(id).map(|&i| &self.commands[i])
    }

    pub fn is_hidden(&self, id: &str) -> bool {
        self.index.get(id).is_some_and(|&i| self.entries[i].hidden)
    }

    /// Resolve a command id *or* a case-insensitive title to an id.
    pub fn resolve(&self, id_or_title: &str) -> Option<&Command> {
        let wanted = id_or_title.trim();
        if let Some(c) = self.get(wanted) {
            return Some(c);
        }
        let lower = wanted.to_lowercase();
        self.entries
            .iter()
            .position(|e| e.title_lower == lower)
            .map(|i| &self.commands[i])
    }

    #[allow(clippy::too_many_arguments)]
    fn score(
        &self,
        i: usize,
        q: &str,
        ql: &str,
        usage: &Usage,
        learned: &HashMap<String, i64>,
        pins: &HashSet<String>,
        now: u64,
    ) -> Option<i64> {
        let c = &self.commands[i];
        let e = &self.entries[i];
        if e.hidden {
            return None;
        }
        // Aliases are the user's own shorthand: exact beats everything, prefix is strong.
        let mut alias_bonus = 0;
        for a in &e.aliases {
            if a == ql {
                alias_bonus = alias_bonus.max(1000);
            } else if a.starts_with(ql) {
                alias_bonus = alias_bonus.max(300);
            }
        }
        let cat = c.kind.category_boost(ql);
        // `vsc` → Visual Studio Code, `ss` → System Settings: word initials.
        let acronym = if ql.len() >= 2 && e.initials.len() >= 2 && !ql.contains(' ') {
            if e.initials == ql {
                350
            } else if e.initials.starts_with(ql) {
                220
            } else {
                0
            }
        } else {
            0
        };
        let mut best = self.matcher.fuzzy_match(&c.title, q).map(|s| {
            // Strongly prefer prefix and word-start matches, like Raycast.
            let bonus = if e.title_lower.starts_with(ql) {
                200
            } else if e.title_lower.split_whitespace().any(|w| w.starts_with(ql)) {
                100
            } else {
                0
            };
            s + bonus
        });
        for (k, kl) in c.keywords.iter().zip(&e.keywords_lower) {
            if let Some(s) = self.matcher.fuzzy_match(k, q) {
                let s = s * 3 / 4 + if kl.starts_with(ql) { 60 } else { 0 };
                best = Some(best.map_or(s, |b| b.max(s)));
            }
        }
        let learned = learned.get(&c.id).copied().unwrap_or(0);
        if best.is_none() && (cat > 0 || alias_bonus > 0 || acronym > 0 || learned > 0) {
            best = Some(0);
        }
        best.map(|s| {
            s + cat
                + alias_bonus
                + acronym
                + learned
                + e.boost
                + usage.frecency(&c.id, now)
                + crate::pins::boost(pins, &c.id)
        })
    }

    /// Ranked results. An empty query lists commands (not apps) first, but caps
    /// how many rows of one kind can fill the first screen (see `kind_cap`).
    pub fn search(&self, query: &str, limit: usize) -> Vec<Command> {
        self.search_with_usage(query, limit, &Usage::default(), &HashSet::new())
    }

    pub fn search_with_usage(&self, query: &str, limit: usize, usage: &Usage, pins: &HashSet<String>) -> Vec<Command> {
        let now = crate::usage::now();
        let q = query.trim();
        let ql = q.to_lowercase();
        let mut out: Vec<Command> = Vec::new();

        if let Some(result) = crate::calc::eval(q) {
            out.push(Command {
                id: format!("calc:{result}"),
                title: result,
                subtitle: Some(format!("= {q} · Enter to copy")),
                icon: Some("🧮".into()),
                kind: CommandKind::Calculator,
                arguments: vec![],
                hotkey: None,
                keywords: vec![],
            });
        }

        if q.is_empty() {
            let mut all: Vec<usize> = (0..self.commands.len()).filter(|&i| !self.entries[i].hidden).collect();
            all.sort_by_key(|&i| {
                let c = &self.commands[i];
                (
                    !pins.contains(&c.id),
                    c.id != "builtin:chat",
                    c.kind == CommandKind::App,
                    std::cmp::Reverse(usage.frecency(&c.id, now) + self.entries[i].boost),
                    c.kind.rank(),
                )
            });
            // Commands first, but no single kind may own the whole first screen:
            // there are ~50 window commands, which would otherwise push every app
            // past the limit on a fresh install. Rows the user pinned, boosted or
            // actually picked keep their place; the rest wait until you type.
            let mut counts = [0u16; 16];
            let mut picked: Vec<usize> = Vec::new();
            let mut apps: Vec<usize> = Vec::new();
            let mut later: Vec<usize> = Vec::new();
            for i in all {
                let c = &self.commands[i];
                if c.kind == CommandKind::App {
                    apps.push(i);
                    continue;
                }
                let slot = c.kind.rank() as usize;
                let known = pins.contains(&c.id) || self.entries[i].boost > 0 || usage.frecency(&c.id, now) > 0;
                if known || counts[slot] < Self::kind_cap(c.kind) {
                    counts[slot] += 1;
                    picked.push(i);
                } else {
                    later.push(i);
                }
            }
            picked.extend(apps);
            picked.extend(later);
            out.extend(picked.into_iter().take(limit).map(|i| self.commands[i].clone()));
            return out;
        }

        let learned = usage.learned_all(&ql);
        let mut scored: Vec<(i64, usize)> = (0..self.commands.len())
            .filter_map(|i| self.score(i, q, &ql, usage, &learned, pins, now).map(|s| (s, i)))
            .collect();
        scored.sort_by(|a, b| {
            let (ca, cb) = (&self.commands[a.1], &self.commands[b.1]);
            b.0.cmp(&a.0)
                .then(ca.kind.rank().cmp(&cb.kind.rank()))
                .then(ca.title.len().cmp(&cb.title.len()))
        });
        out.extend(scored.into_iter().take(limit).map(|(_, i)| self.commands[i].clone()));
        out
    }

    /// How many rows of one kind an empty query may show before the rest wait
    /// until the user types, pins or uses them. The launcher ships ~45 built-ins
    /// and ~50 window commands: uncapped, they fill the list and no app is ever
    /// visible on the first screen.
    fn kind_cap(kind: CommandKind) -> u16 {
        match kind {
            CommandKind::Window => 12,
            CommandKind::Builtin => 16,
            _ => u16::MAX,
        }
    }
}

/// First letter of each word, splitting on non-alphanumerics and camelCase.
fn initials(title: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for ch in title.chars() {
        let starts_word = match prev {
            None => ch.is_alphanumeric(),
            Some(p) => ch.is_alphanumeric() && (!p.is_alphanumeric() || (p.is_lowercase() && ch.is_uppercase())),
        };
        if starts_word {
            out.extend(ch.to_lowercase());
        }
        prev = Some(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str, title: &str, kind: CommandKind, kw: &[&str]) -> Command {
        Command {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            icon: None,
            kind,
            arguments: vec![],
            hotkey: None,
            keywords: kw.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn cmds() -> Vec<Command> {
        vec![
            cmd("app:safari", "Safari", CommandKind::App, &[]),
            cmd("app:settings", "System Settings", CommandKind::App, &[]),
            cmd("app:slack", "Slack", CommandKind::App, &[]),
            cmd("app:window-app", "Window Manager Helper", CommandKind::App, &[]),
            cmd("ai:fix", "Fix Spelling and Grammar", CommandKind::Ai, &["proofread"]),
            cmd("builtin:reload", "Reload Commands", CommandKind::Builtin, &[]),
            cmd(
                "window:left-half",
                "Left Half",
                CommandKind::Window,
                &["window", "snap"],
            ),
            cmd("window:maximize", "Maximize", CommandKind::Window, &["window", "snap"]),
            cmd(
                "layout:remember",
                "Remember Layout",
                CommandKind::Layout,
                &["layout", "workspace"],
            ),
        ]
    }

    fn reg() -> Registry {
        Registry::new(cmds())
    }

    #[test]
    fn prefix_beats_scattered_match() {
        let r = reg();
        let res = r.search("sl", 10);
        assert_eq!(res[0].id, "app:slack");
    }

    #[test]
    fn word_start_match() {
        let res = reg().search("sett", 10);
        assert_eq!(res[0].id, "app:settings");
    }

    #[test]
    fn keywords_match() {
        let res = reg().search("proof", 10);
        assert_eq!(res[0].id, "ai:fix");
    }

    #[test]
    fn empty_query_lists_commands_before_apps() {
        let res = reg().search("", 10);
        assert_eq!(res[0].kind, CommandKind::Ai);
        assert_eq!(res.last().unwrap().kind, CommandKind::App);
    }

    #[test]
    fn empty_query_never_fills_the_screen_with_one_kind() {
        // A real install has ~50 window commands: they must not crowd apps out of
        // the first screen, and the rest of them appear when you type "left half".
        let mut many = cmds();
        for n in 0..48 {
            many.push(cmd(
                &format!("window:extra-{n}"),
                &format!("Extra Snap {n}"),
                CommandKind::Window,
                &["window"],
            ));
        }
        for n in 0..30 {
            many.push(cmd(
                &format!("builtin:extra-{n}"),
                &format!("Extra Tool {n}"),
                CommandKind::Builtin,
                &[],
            ));
        }
        let r = Registry::new(many);
        let res = r.search("", 60);
        let first_app = res.iter().position(|c| c.kind == CommandKind::App).unwrap();
        let windows_before_apps = res[..first_app]
            .iter()
            .filter(|c| c.kind == CommandKind::Window)
            .count();
        let builtins_before_apps = res[..first_app]
            .iter()
            .filter(|c| c.kind == CommandKind::Builtin)
            .count();
        assert!(
            windows_before_apps <= 12 && builtins_before_apps <= 16,
            "{windows_before_apps} window and {builtins_before_apps} built-in rows come before any app"
        );
        assert!(first_app < 35, "apps must reach the first screen (got row {first_app})");
        assert_eq!(res[0].kind, CommandKind::Ai, "commands still lead");
        // The commands that waited are still found when you type.
        assert!(r.search("extra snap 47", 10).iter().any(|c| c.id == "window:extra-47"));
        assert!(r.search("extra tool 29", 10).iter().any(|c| c.id == "builtin:extra-29"));
    }

    #[test]
    fn used_window_commands_are_not_capped() {
        let mut many = cmds();
        for n in 0..48 {
            many.push(cmd(
                &format!("window:extra-{n}"),
                &format!("Extra Snap {n}"),
                CommandKind::Window,
                &["window"],
            ));
        }
        let r = Registry::new(many);
        let mut u = Usage::default();
        for n in 0..30 {
            u.record(&format!("window:extra-{n}"), "snap", crate::usage::now());
        }
        let res = r.search_with_usage("", 60, &u, &HashSet::new());
        assert!(res.iter().filter(|c| c.kind == CommandKind::Window).count() > 20);
    }

    #[test]
    fn calculator_row_comes_first() {
        let res = reg().search("12*3", 10);
        assert_eq!(res[0].kind, CommandKind::Calculator);
        assert_eq!(res[0].title, "36");
    }

    #[test]
    fn category_prefix_promotes_window_commands() {
        let res = reg().search("windo", 10);
        assert!(res.len() >= 2);
        assert_eq!(res[0].kind, CommandKind::Window);
        assert_eq!(res[1].kind, CommandKind::Window);
        let win = res.iter().position(|c| c.kind == CommandKind::Window).unwrap();
        let decoy = res.iter().position(|c| c.id == "app:window-app").unwrap();
        assert!(win < decoy);
    }

    #[test]
    fn category_layout_beats_unrelated_apps() {
        let res = reg().search("layout", 5);
        assert_eq!(res[0].kind, CommandKind::Layout);
    }

    #[test]
    fn category_ai_prefix() {
        let res = reg().search("ai", 5);
        assert_eq!(res[0].kind, CommandKind::Ai);
    }

    #[test]
    fn initials_split_words_and_camel_case() {
        assert_eq!(initials("Visual Studio Code"), "vsc");
        assert_eq!(initials("iTerm"), "it");
        assert_eq!(initials("Fix Spelling and Grammar"), "fsag");
        assert_eq!(initials("Left-Half (window)"), "lhw");
    }

    #[test]
    fn acronyms_find_multi_word_titles() {
        let r = Registry::new(vec![
            cmd("app:vsc", "Visual Studio Code", CommandKind::App, &[]),
            cmd("app:vlc", "VLC", CommandKind::App, &[]),
            cmd("app:settings", "System Settings", CommandKind::App, &[]),
            cmd("app:safari", "Safari", CommandKind::App, &[]),
        ]);
        assert_eq!(r.search("vsc", 5)[0].id, "app:vsc");
        assert_eq!(r.search("ss", 5)[0].id, "app:settings");
    }

    #[test]
    fn learned_picks_reorder_results() {
        let r = reg();
        // Without history, "s" puts Slack/Safari/Settings in some order; teach it Settings.
        let mut u = Usage::default();
        u.record("app:settings", "s", crate::usage::now());
        let res = r.search_with_usage("s", 5, &u, &HashSet::new());
        assert_eq!(res[0].id, "app:settings");
        // And a learned pick surfaces even a weak fuzzy match.
        let mut u = Usage::default();
        u.record("builtin:reload", "zz", crate::usage::now());
        let res = r.search_with_usage("zz", 5, &u, &HashSet::new());
        assert_eq!(res[0].id, "builtin:reload");
    }

    #[test]
    fn get_and_resolve_use_the_index() {
        let r = reg();
        assert_eq!(r.get("app:slack").unwrap().title, "Slack");
        assert_eq!(r.resolve("left half").unwrap().id, "window:left-half");
        assert_eq!(r.resolve("window:maximize").unwrap().id, "window:maximize");
        assert!(r.resolve("nope").is_none());
    }

    #[test]
    fn aliases_boost_and_hidden() {
        let mut o = HashMap::new();
        o.insert(
            "app:settings".to_string(),
            Override {
                aliases: vec!["ss".into()],
                boost: 0,
                hidden: false,
            },
        );
        o.insert(
            "app:window-app".to_string(),
            Override {
                aliases: vec![],
                boost: 0,
                hidden: true,
            },
        );
        o.insert(
            "window:maximize".to_string(),
            Override {
                aliases: vec![],
                boost: 250,
                hidden: false,
            },
        );
        let r = Registry::with_overrides(cmds(), &o);
        // "ss" would otherwise fuzzy-match nothing well; the alias wins outright.
        assert_eq!(r.search("ss", 5)[0].id, "app:settings");
        // Hidden commands never appear, even on an empty query.
        assert!(r.search("", 50).iter().all(|c| c.id != "app:window-app"));
        assert!(r.search("window", 50).iter().all(|c| c.id != "app:window-app"));
        assert!(r.is_hidden("app:window-app"));
        // Boost reorders equals: both window commands match "window"; maximize is boosted.
        assert_eq!(r.search("window", 5)[0].id, "window:maximize");
    }
}
