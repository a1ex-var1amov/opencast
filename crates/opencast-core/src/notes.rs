//! Local quick notes (`notes.json` in the config dir).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commands::{Command, CommandKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    notes: Vec<Note>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("notes.json")
}

fn load_store(dir: &Path) -> Store {
    let Ok(raw) = fs::read_to_string(path(dir)) else {
        return Store::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_store(dir: &Path, store: &Store) {
    let _ = fs::create_dir_all(dir);
    if let Ok(raw) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path(dir), raw);
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn add(dir: &Path, title: &str, body: &str) -> Note {
    let mut store = load_store(dir);
    let note = Note {
        id: format!("note:{}", now()),
        title: title.trim().to_string(),
        body: body.to_string(),
        updated: now(),
    };
    store.notes.insert(0, note.clone());
    save_store(dir, &store);
    note
}

pub fn get(dir: &Path, id: &str) -> Option<Note> {
    load_store(dir).notes.into_iter().find(|n| n.id == id)
}

/// Rows when query looks like notes search.
pub fn matching(dir: &Path, query: &str, limit: usize) -> Vec<Command> {
    let q = query.trim().to_lowercase();
    let needle = if q == "note" || q == "notes" {
        ""
    } else if let Some(rest) = q.strip_prefix("note ") {
        rest.trim()
    } else if let Some(rest) = q.strip_prefix("notes ") {
        rest.trim()
    } else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for n in load_store(dir).notes {
        if out.len() >= limit {
            break;
        }
        if !needle.is_empty() {
            let hay = format!("{} {}", n.title, n.body).to_lowercase();
            if !hay.contains(needle) {
                continue;
            }
        }
        let preview: String = n.body.chars().take(60).collect();
        out.push(Command {
            id: n.id.clone(),
            title: if n.title.is_empty() {
                preview.clone()
            } else {
                n.title.clone()
            },
            subtitle: Some(if n.title.is_empty() {
                "Note · Return to paste".into()
            } else {
                format!("Note · {preview}")
            }),
            icon: Some("📝".into()),
            kind: CommandKind::Builtin,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["note".into(), "notes".into()],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_match() {
        let dir = tempfile::tempdir().unwrap();
        add(dir.path(), "Ship", "parity slice");
        let rows = matching(dir.path(), "note ship", 5);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].title.contains("Ship"));
    }
}
