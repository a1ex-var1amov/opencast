//! Pinned command ids (`pinned.json` in the config dir). Newest pin first.
//!
//! The engine keeps the list in memory (see `Engine::toggle_pin`); this is
//! the on-disk store.

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

pub fn load_list(dir: &Path) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(path(dir)) else {
        return Vec::new();
    };
    serde_json::from_str::<Store>(&raw).map(|s| s.ids).unwrap_or_default()
}

pub fn save_list(dir: &Path, ids: &[String]) {
    let _ = fs::create_dir_all(dir);
    let store = Store { ids: ids.to_vec() };
    if let Ok(raw) = serde_json::to_string_pretty(&store) {
        let _ = fs::write(path(dir), raw);
    }
}

pub fn load(dir: &Path) -> HashSet<String> {
    load_list(dir).into_iter().collect()
}

pub fn pinnable(id: &str) -> bool {
    crate::usage::tracked(id)
}

/// Toggle in memory. Returns whether the id is pinned after the toggle.
pub fn toggle_in(ids: &mut Vec<String>, id: &str) -> bool {
    if !pinnable(id) {
        return false;
    }
    if let Some(i) = ids.iter().position(|x| x == id) {
        ids.remove(i);
        false
    } else {
        ids.insert(0, id.to_string());
        true
    }
}

/// Toggle pin on disk. Returns whether the id is pinned after the toggle.
pub fn toggle(dir: &Path, id: &str) -> bool {
    let mut ids = load_list(dir);
    let pinned = toggle_in(&mut ids, id);
    if pinnable(id) {
        save_list(dir, &ids);
    }
    pinned
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
        assert!(!toggle(dir.path(), "calc:1"));
    }
}
