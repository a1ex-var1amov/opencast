//! How often each command id was run. Boosts search ranking.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Store {
    counts: HashMap<String, u32>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("usage.json")
}

fn load(dir: &Path) -> Store {
    let Ok(raw) = fs::read_to_string(path(dir)) else {
        return Store::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(dir: &Path, store: &Store) {
    let _ = fs::create_dir_all(dir);
    if let Ok(raw) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path(dir), raw);
    }
}

pub fn bump(dir: &Path, id: &str) {
    if id.is_empty()
        || id.starts_with("calc:")
        || id.starts_with("clip:")
        || id.starts_with("emoji:")
    {
        return;
    }
    let mut store = load(dir);
    *store.counts.entry(id.to_string()).or_insert(0) += 1;
    save(dir, &store);
}

pub fn counts(dir: &Path) -> HashMap<String, u32> {
    load(dir).counts
}

/// Cap so a single favorite cannot drown category/title matches.
pub fn boost(counts: &HashMap<String, u32>, id: &str) -> i64 {
    counts.get(id).map(|n| (*n).min(80) as i64 * 3).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_and_boost() {
        let dir = tempfile::tempdir().unwrap();
        bump(dir.path(), "window:left-half");
        bump(dir.path(), "window:left-half");
        let c = counts(dir.path());
        assert_eq!(c.get("window:left-half"), Some(&2));
        assert_eq!(boost(&c, "window:left-half"), 6);
    }
}
