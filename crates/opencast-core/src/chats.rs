//! Saved AI chats (`chats.json` in the config dir), newest first.
//!
//! The panel saves a chat after each finished answer; `chats …` in the
//! launcher lists them and Return resumes one with its provider and model.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commands::{Command, CommandKind};
use crate::providers::Message;

/// Most chats kept; older ones are dropped.
pub const MAX_CHATS: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedChat {
    /// `chat:<uuid>` (the panel makes the id).
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub updated: u64,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system: Option<String>,
    pub messages: Vec<Message>,
}

impl SavedChat {
    /// First user line, trimmed, as a title when the panel did not set one.
    pub fn derive_title(messages: &[Message]) -> String {
        let first = messages
            .iter()
            .find(|m| m.role == crate::providers::Role::User)
            .map(|m| m.content.trim())
            .unwrap_or("");
        let line = first.lines().next().unwrap_or("").trim();
        let mut t: String = line.chars().take(70).collect();
        if line.chars().count() > 70 {
            t.push('…');
        }
        if t.is_empty() {
            "Untitled chat".into()
        } else {
            t
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    chats: Vec<SavedChat>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("chats.json")
}

pub fn load_all(dir: &Path) -> Vec<SavedChat> {
    fs::read_to_string(path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Store>(&s).ok())
        .map(|s| s.chats)
        .unwrap_or_default()
}

pub fn save_all(dir: &Path, chats: &[SavedChat]) {
    let _ = fs::create_dir_all(dir);
    let store = Store { chats: chats.to_vec() };
    if let Ok(s) = serde_json::to_string(&store) {
        let tmp = dir.join("chats.json.tmp");
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, path(dir));
        }
    }
}

/// Insert or replace by id, move to the front, cap the list.
pub fn upsert(chats: &mut Vec<SavedChat>, mut chat: SavedChat) {
    if chat.title.trim().is_empty() {
        chat.title = SavedChat::derive_title(&chat.messages);
    }
    chats.retain(|c| c.id != chat.id);
    chats.insert(0, chat);
    chats.truncate(MAX_CHATS);
}

pub fn remove(chats: &mut Vec<SavedChat>, id: &str) -> bool {
    let before = chats.len();
    chats.retain(|c| c.id != id);
    chats.len() != before
}

/// `chats` / `chat history` / `history`. Search matches these; OpenCast Keys lists them.
/// `history` is also browser history; the Keys list keeps the first one.
pub const PREFIXES: &[(&str, &str)] = &[
    ("chat history", "Saved chats"),
    ("chats", "Saved chats"),
    ("history", "Saved chats"),
];

/// The search needle when `query` asks for chats (`chats`, `chat history`, `chats rust`).
pub fn needle(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    for (prefix, _) in PREFIXES {
        if q == *prefix {
            return Some(String::new());
        }
        if let Some(rest) = q.strip_prefix(&format!("{prefix} ")) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

pub fn matching_in(chats: &[SavedChat], query: &str, limit: usize) -> Vec<Command> {
    let Some(needle) = needle(query) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for c in chats {
        if out.len() >= limit {
            break;
        }
        if !needle.is_empty() {
            let hit = c.title.to_lowercase().contains(&needle)
                || c.messages.iter().any(|m| m.content.to_lowercase().contains(&needle));
            if !hit {
                continue;
            }
        }
        let turns = c
            .messages
            .iter()
            .filter(|m| m.role == crate::providers::Role::User)
            .count();
        out.push(Command {
            id: c.id.clone(),
            title: c.title.clone(),
            subtitle: Some(format!(
                "Chat · {turns} message{}{}",
                if turns == 1 { "" } else { "s" },
                c.model.as_deref().map(|m| format!(" · {m}")).unwrap_or_default()
            )),
            icon: Some("💬".into()),
            kind: CommandKind::Builtin,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["chat".into(), "history".into()],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Role;

    fn chat(id: &str, text: &str, at: u64) -> SavedChat {
        SavedChat {
            id: id.into(),
            title: String::new(),
            updated: at,
            provider: None,
            model: Some("m".into()),
            system: None,
            messages: vec![Message::new(Role::User, text), Message::new(Role::Assistant, "ok")],
        }
    }

    #[test]
    fn upsert_orders_titles_and_caps() {
        let mut v = Vec::new();
        upsert(&mut v, chat("chat:a", "How do I rebase?\nmore", 1));
        upsert(&mut v, chat("chat:b", "Explain lifetimes", 2));
        upsert(&mut v, chat("chat:a", "How do I rebase?", 3));
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "chat:a");
        assert_eq!(v[0].title, "How do I rebase?");
        for i in 0..(MAX_CHATS + 5) {
            upsert(&mut v, chat(&format!("chat:{i}"), "x", i as u64));
        }
        assert_eq!(v.len(), MAX_CHATS);
        assert!(remove(&mut v, "chat:0") || !v.iter().any(|c| c.id == "chat:0"));
    }

    #[test]
    fn search_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vec::new();
        upsert(&mut v, chat("chat:a", "rust lifetimes", 1));
        upsert(&mut v, chat("chat:b", "git rebase", 2));
        save_all(dir.path(), &v);
        let loaded = load_all(dir.path());
        assert_eq!(loaded.len(), 2);
        assert_eq!(matching_in(&loaded, "chats", 10).len(), 2);
        let hits = matching_in(&loaded, "chats rebase", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "chat:b");
        assert!(matching_in(&loaded, "rebase", 10).is_empty());
        assert_eq!(needle("chat history"), Some(String::new()));
    }
}
