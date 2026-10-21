//! The command registry and fuzzy search over it.

use std::collections::HashMap;

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
        }
    }

    /// Large boost when `q` looks like a type name (prefix of a category alias).
    fn category_boost(self, q: &str) -> i64 {
        let ql = q.trim().to_lowercase();
        if ql.len() < 2 {
            return 0;
        }
        for alias in self.category_aliases() {
            // "windo" → window; require the alias to grow from the query (not "sett" → settings-on-builtin).
            if alias.starts_with(&ql) {
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

pub struct Registry {
    commands: Vec<Command>,
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
        Self {
            commands,
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
        self.commands.iter().find(|c| c.id == id)
    }

    fn score(
        &self,
        c: &Command,
        q: &str,
        usage: &HashMap<String, u32>,
        pins: &std::collections::HashSet<String>,
    ) -> Option<i64> {
        let title_l = c.title.to_lowercase();
        let ql = q.to_lowercase();
        let cat = c.kind.category_boost(q);
        let mut best = self.matcher.fuzzy_match(&c.title, q).map(|s| {
            // Strongly prefer prefix and word-start matches, like Raycast.
            let bonus = if title_l.starts_with(&ql) {
                200
            } else if title_l.split_whitespace().any(|w| w.starts_with(&ql)) {
                100
            } else {
                0
            };
            s + bonus
        });
        for k in &c.keywords {
            if let Some(s) = self.matcher.fuzzy_match(k, q) {
                let s = s * 3 / 4 + if k.to_lowercase().starts_with(&ql) { 60 } else { 0 };
                best = Some(best.map_or(s, |b| b.max(s)));
            }
        }
        // Category-only hit (e.g. "windo" → all window snaps) still ranks.
        if best.is_none() && cat > 0 {
            best = Some(0);
        }
        best.map(|s| {
            s + cat + crate::usage::boost(usage, &c.id) + crate::pins::boost(pins, &c.id)
        })
    }

    /// Ranked results. An empty query lists commands (not apps) first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Command> {
        self.search_with_usage(query, limit, &HashMap::new(), &std::collections::HashSet::new())
    }

    pub fn search_with_usage(
        &self,
        query: &str,
        limit: usize,
        usage: &HashMap<String, u32>,
        pins: &std::collections::HashSet<String>,
    ) -> Vec<Command> {
        let q = query.trim();
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
            let mut all: Vec<&Command> = self.commands.iter().collect();
            all.sort_by_key(|c| {
                (
                    !pins.contains(&c.id),
                    c.id != "builtin:chat",
                    c.kind == CommandKind::App,
                    std::cmp::Reverse(usage.get(&c.id).copied().unwrap_or(0)),
                    c.kind.rank(),
                )
            });
            out.extend(all.into_iter().take(limit).cloned());
            return out;
        }

        let mut scored: Vec<(i64, &Command)> = self
            .commands
            .iter()
            .filter_map(|c| self.score(c, q, usage, pins).map(|s| (s, c)))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1.kind.rank().cmp(&b.1.kind.rank()))
                .then(a.1.title.len().cmp(&b.1.title.len()))
        });
        out.extend(scored.into_iter().take(limit).map(|(_, c)| c.clone()));
        out
    }
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

    fn reg() -> Registry {
        Registry::new(vec![
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
            cmd(
                "window:maximize",
                "Maximize",
                CommandKind::Window,
                &["window", "snap"],
            ),
            cmd(
                "layout:remember",
                "Remember Layout",
                CommandKind::Layout,
                &["layout", "workspace"],
            ),
        ])
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
}
