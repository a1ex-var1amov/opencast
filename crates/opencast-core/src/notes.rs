//! Local quick notes (`notes.json` in the config dir). Newest first.
//!
//! The engine keeps the notes in memory (see `Engine::add_note`); this is the
//! on-disk store plus the pure search over a slice.

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

pub fn load_all(dir: &Path) -> Vec<Note> {
    let Ok(raw) = fs::read_to_string(path(dir)) else {
        return Vec::new();
    };
    serde_json::from_str::<Store>(&raw).map(|s| s.notes).unwrap_or_default()
}

pub fn save_all(dir: &Path, notes: &[Note]) {
    let _ = fs::create_dir_all(dir);
    let store = Store { notes: notes.to_vec() };
    if let Ok(raw) = serde_json::to_string_pretty(&store) {
        let _ = fs::write(path(dir), raw);
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Prepend a note in memory. Ids are unique even within one second.
pub fn add_in(notes: &mut Vec<Note>, title: &str, body: &str) -> Note {
    let stamp = now();
    let mut id = format!("note:{stamp}");
    let mut n = 1;
    while notes.iter().any(|x| x.id == id) {
        id = format!("note:{stamp}-{n}");
        n += 1;
    }
    let note = Note {
        id,
        title: title.trim().to_string(),
        body: body.to_string(),
        updated: stamp,
    };
    notes.insert(0, note.clone());
    note
}

pub fn add(dir: &Path, title: &str, body: &str) -> Note {
    let mut notes = load_all(dir);
    let note = add_in(&mut notes, title, body);
    save_all(dir, &notes);
    note
}

pub fn get(dir: &Path, id: &str) -> Option<Note> {
    load_all(dir).into_iter().find(|n| n.id == id)
}

/// The search needle when `query` is a notes query (`note`, `notes x`), else None.
pub fn needle(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    if q == "note" || q == "notes" {
        return Some(String::new());
    }
    if let Some(rest) = q.strip_prefix("note ").or_else(|| q.strip_prefix("notes ")) {
        return Some(rest.trim().to_string());
    }
    None
}

/// Rows for `query`: every note for `note`/`notes`, notes containing the
/// rest for `note …`, and a few notes containing a plain query (2+ chars).
pub fn matching_in(notes: &[Note], query: &str, limit: usize) -> Vec<Command> {
    let (needle, limit) = match needle(query) {
        Some(n) => (n, limit),
        None => {
            let q = query.trim().to_lowercase();
            if q.chars().count() < 2 {
                return Vec::new();
            }
            (q, limit.min(5))
        }
    };
    let mut out = Vec::new();
    for n in notes {
        if out.len() >= limit {
            break;
        }
        if !needle.is_empty() {
            let hay = format!("{} {}", n.title, n.body).to_lowercase();
            if !hay.contains(&needle) {
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
                "Note".into()
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

pub fn matching(dir: &Path, query: &str, limit: usize) -> Vec<Command> {
    matching_in(&load_all(dir), query, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_match() {
        let dir = tempfile::tempdir().unwrap();
        add(dir.path(), "Ship", "parity slice");
        add(dir.path(), "Other", "thing");
        let rows = matching(dir.path(), "note ship", 5);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].title.contains("Ship"));
        assert_eq!(matching(dir.path(), "notes", 5).len(), 2);
        assert!(matching(dir.path(), "shipping", 5).is_empty());
        assert_eq!(matching(dir.path(), "ship", 5).len(), 1);
        assert!(matching(dir.path(), "s", 5).is_empty());
        let all = load_all(dir.path());
        assert_ne!(all[0].id, all[1].id);
    }
}
