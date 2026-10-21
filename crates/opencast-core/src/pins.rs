//! Pinned command ids (`pinned.json` in the config dir).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Store {
    ids: Vec<String>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("pinned.json")
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

pub fn load(dir: &Path) -> HashSet<String> {
    load_store(dir).ids.into_iter().collect()
}

/// Toggle pin. Returns whether the id is pinned after the toggle.
pub fn toggle(dir: &Path, id: &str) -> bool {
    if id.is_empty() || id.starts_with("calc:") || id.starts_with("clip:") || id.starts_with("emoji:") {
        return false;
    }
    let mut store = load_store(dir);
    if let Some(i) = store.ids.iter().position(|x| x == id) {
        store.ids.remove(i);
        save_store(dir, &store);
        false
    } else {
        store.ids.insert(0, id.to_string());
        save_store(dir, &store);
        true
    }
}

pub fn boost(pins: &HashSet<String>, id: &str) -> i64 {
    if pins.contains(id) {
        500
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        assert!(toggle(dir.path(), "window:left-half"));
        assert!(load(dir.path()).contains("window:left-half"));
        assert!(!toggle(dir.path(), "window:left-half"));
        assert!(!load(dir.path()).contains("window:left-half"));
    }
}
