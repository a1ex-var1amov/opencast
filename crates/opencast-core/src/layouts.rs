//! Saved window layouts: "remember" the current window arrangement under a
//! name and restore it later. The platform shell captures and applies the
//! actual window rectangles; this module owns the on-disk store
//! (`<config dir>/layouts.json`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::slug;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    #[serde(default)]
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppWindows {
    pub app: String,
    pub windows: Vec<SavedWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    /// Unix seconds when the layout was saved.
    #[serde(default)]
    pub saved_at: u64,
    pub apps: Vec<AppWindows>,
}

impl Layout {
    pub fn window_count(&self) -> usize {
        self.apps.iter().map(|a| a.windows.len()).sum()
    }
}

/// Keyed by slug so names can be edited freely later.
pub type Store = BTreeMap<String, Layout>;

fn file(dir: &Path) -> PathBuf {
    dir.join("layouts.json")
}

pub fn load(dir: &Path) -> Store {
    let Ok(src) = std::fs::read_to_string(file(dir)) else {
        return Store::new();
    };
    serde_json::from_str(&src).unwrap_or_default()
}

fn write(dir: &Path, store: &Store) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = file(dir);
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
}

/// `(slug, name, window count)` for every saved layout, sorted by slug.
pub fn list(dir: &Path) -> Vec<(String, String, usize)> {
    load(dir)
        .into_iter()
        .map(|(slug, l)| {
            let n = l.window_count();
            (slug, l.name, n)
        })
        .collect()
}

pub fn get(dir: &Path, layout_slug: &str) -> Option<Layout> {
    load(dir).remove(layout_slug)
}

/// Save (or overwrite) a layout under `name`. Returns its slug.
pub fn save(dir: &Path, name: &str, apps: Vec<AppWindows>) -> Result<String> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "the layout needs a name");
    let key = slug(name);
    anyhow::ensure!(!key.is_empty(), "the layout needs a name with letters or digits");
    let mut store = load(dir);
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    store.insert(
        key.clone(),
        Layout {
            name: name.to_string(),
            saved_at,
            apps,
        },
    );
    write(dir, &store)?;
    Ok(key)
}

/// Delete by name or slug. Returns true if something was removed.
pub fn delete(dir: &Path, name_or_slug: &str) -> Result<bool> {
    let key = slug(name_or_slug);
    let mut store = load(dir);
    let removed = store.remove(&key).is_some();
    if removed {
        write(dir, &store)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<AppWindows> {
        vec![
            AppWindows {
                app: "Safari".into(),
                windows: vec![SavedWindow {
                    title: "Docs".into(),
                    x: 0.0,
                    y: 25.0,
                    w: 800.0,
                    h: 600.0,
                }],
            },
            AppWindows {
                app: "Ghostty".into(),
                windows: vec![
                    SavedWindow {
                        title: "".into(),
                        x: 800.0,
                        y: 25.0,
                        w: 640.0,
                        h: 600.0,
                    },
                    SavedWindow {
                        title: "logs".into(),
                        x: 0.0,
                        y: 650.0,
                        w: 1440.0,
                        h: 250.0,
                    },
                ],
            },
        ]
    }

    #[test]
    fn save_list_get_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let key = save(dir.path(), "  Work Setup ", sample()).unwrap();
        assert_eq!(key, "work-setup");

        let listed = list(dir.path());
        assert_eq!(listed, vec![("work-setup".into(), "Work Setup".into(), 3)]);

        let l = get(dir.path(), "work-setup").unwrap();
        assert_eq!(l.apps, sample());
        assert!(l.saved_at > 0);

        // Overwrite keeps one entry.
        save(dir.path(), "work setup", vec![]).unwrap();
        assert_eq!(list(dir.path())[0].2, 0);

        assert!(delete(dir.path(), "Work Setup").unwrap());
        assert!(!delete(dir.path(), "work-setup").unwrap());
        assert!(list(dir.path()).is_empty());
    }

    #[test]
    fn empty_or_missing_store_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(dir.path()).is_empty());
        assert!(get(dir.path(), "nope").is_none());
        std::fs::write(dir.path().join("layouts.json"), "not json").unwrap();
        assert!(list(dir.path()).is_empty());
        assert!(save(dir.path(), "   ", vec![]).is_err());
    }
}
