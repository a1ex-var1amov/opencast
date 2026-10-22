//! Saved window layouts: "remember" the current window arrangement under a
//! name and restore it later. The platform shell captures and applies the
//! actual window rectangles; this module owns the on-disk store
//! (`<config dir>/layouts.json`).
//!
//! A layout is keyed by name *and* display topology: saving "Work" at the
//! desk and again on the laptop keeps both, and restoring picks the variant
//! that matches the screens in front of you (falling back to the newest).

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
    /// Bundle id when the shell knows it (more reliable than the name for launching).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub windows: Vec<SavedWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    /// Unix seconds when the layout was saved.
    #[serde(default)]
    pub saved_at: u64,
    /// Display topology this was saved on; see [`display_signature`].
    #[serde(default)]
    pub displays: String,
    pub apps: Vec<AppWindows>,
}

impl Layout {
    pub fn window_count(&self) -> usize {
        self.apps.iter().map(|a| a.windows.len()).sum()
    }
}

/// A stable id for a set of screens: sorted `x,y,w,h` tuples joined by `;`.
/// Rounded to whole points so a sub-pixel scale change does not split layouts.
pub fn display_signature(frames: &[(f64, f64, f64, f64)]) -> String {
    let mut parts: Vec<String> = frames
        .iter()
        .map(|(x, y, w, h)| format!("{},{},{},{}", x.round(), y.round(), w.round(), h.round()))
        .collect();
    parts.sort();
    parts.join(";")
}

/// Keyed by slug; each slug holds one variant per display signature.
pub type Store = BTreeMap<String, Vec<Layout>>;

/// Pre-topology file format: one layout per slug.
type LegacyStore = BTreeMap<String, Layout>;

fn file(dir: &Path) -> PathBuf {
    dir.join("layouts.json")
}

pub fn load(dir: &Path) -> Store {
    let Ok(src) = std::fs::read_to_string(file(dir)) else {
        return Store::new();
    };
    if let Ok(store) = serde_json::from_str::<Store>(&src) {
        return store;
    }
    serde_json::from_str::<LegacyStore>(&src)
        .map(|old| old.into_iter().map(|(k, l)| (k, vec![l])).collect())
        .unwrap_or_default()
}

fn write(dir: &Path, store: &Store) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = file(dir);
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
}

fn newest(variants: &[Layout]) -> Option<&Layout> {
    variants.iter().max_by_key(|l| l.saved_at)
}

/// `(slug, name, window count)` for every saved layout, sorted by slug.
/// The count is from the newest variant.
pub fn list(dir: &Path) -> Vec<(String, String, usize)> {
    load(dir)
        .into_iter()
        .filter_map(|(slug, variants)| {
            let l = newest(&variants)?;
            Some((slug, l.name.clone(), l.window_count()))
        })
        .collect()
}

/// Newest variant, whatever the displays.
pub fn get(dir: &Path, layout_slug: &str) -> Option<Layout> {
    let store = load(dir);
    newest(store.get(layout_slug)?).cloned()
}

/// The variant saved on `displays` if there is one, else the newest.
pub fn get_for(dir: &Path, layout_slug: &str, displays: &str) -> Option<Layout> {
    let store = load(dir);
    let variants = store.get(layout_slug)?;
    variants
        .iter()
        .find(|l| l.displays == displays)
        .or_else(|| newest(variants))
        .cloned()
}

/// Save a layout under `name` for `displays`, replacing the variant saved on
/// the same displays and keeping the others. Returns its slug.
pub fn save(dir: &Path, name: &str, displays: &str, apps: Vec<AppWindows>) -> Result<String> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "the layout needs a name");
    let key = slug(name);
    anyhow::ensure!(!key.is_empty(), "the layout needs a name with letters or digits");
    let mut store = load(dir);
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let variants = store.entry(key.clone()).or_default();
    variants.retain(|l| l.displays != displays);
    variants.push(Layout {
        name: name.to_string(),
        saved_at,
        displays: displays.to_string(),
        apps,
    });
    write(dir, &store)?;
    Ok(key)
}

/// Delete by name or slug (every variant). Returns true if something was removed.
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
                bundle_id: Some("com.apple.Safari".into()),
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
                bundle_id: None,
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

    const DESK: &str = "0,0,2560,1440;2560,0,1920,1080";
    const LAPTOP: &str = "0,0,1512,982";

    #[test]
    fn save_list_get_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let key = save(dir.path(), "  Work Setup ", DESK, sample()).unwrap();
        assert_eq!(key, "work-setup");

        let listed = list(dir.path());
        assert_eq!(listed, vec![("work-setup".into(), "Work Setup".into(), 3)]);

        let l = get(dir.path(), "work-setup").unwrap();
        assert_eq!(l.apps, sample());
        assert!(l.saved_at > 0);
        assert_eq!(l.displays, DESK);

        // Same displays: overwrite keeps one variant.
        save(dir.path(), "work setup", DESK, vec![]).unwrap();
        assert_eq!(load(dir.path())["work-setup"].len(), 1);
        assert_eq!(list(dir.path())[0].2, 0);

        assert!(delete(dir.path(), "Work Setup").unwrap());
        assert!(!delete(dir.path(), "work-setup").unwrap());
        assert!(list(dir.path()).is_empty());
    }

    #[test]
    fn variants_per_display_topology() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), "Work", DESK, sample()).unwrap();
        // Same second: `max_by_key` keeps the last of equal maxima, i.e. the latest save.
        save(dir.path(), "Work", LAPTOP, vec![]).unwrap();
        assert_eq!(load(dir.path())["work"].len(), 2);
        assert_eq!(list(dir.path()).len(), 1);
        assert_eq!(get_for(dir.path(), "work", DESK).unwrap().window_count(), 3);
        assert_eq!(get_for(dir.path(), "work", LAPTOP).unwrap().window_count(), 0);
        // Unknown displays: newest wins.
        assert_eq!(get_for(dir.path(), "work", "9,9,9,9").unwrap().window_count(), 0);
        assert!(delete(dir.path(), "work").unwrap());
        assert!(get_for(dir.path(), "work", DESK).is_none());
    }

    #[test]
    fn reads_the_legacy_single_variant_format() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("layouts.json"),
            r#"{"work":{"name":"Work","saved_at":5,"apps":[{"app":"Safari","windows":[{"x":0,"y":0,"w":1,"h":1}]}]}}"#,
        )
        .unwrap();
        let l = get(dir.path(), "work").unwrap();
        assert_eq!(l.name, "Work");
        assert_eq!(l.displays, "");
        assert_eq!(l.window_count(), 1);
        assert_eq!(l.apps[0].bundle_id, None);
    }

    #[test]
    fn signature_is_order_independent_and_rounded() {
        let a = display_signature(&[(2560.0, 0.0, 1920.0, 1080.0), (0.0, 0.0, 2560.0, 1440.0)]);
        let b = display_signature(&[(0.0, 0.0, 2560.0, 1440.0), (2560.0, 0.2, 1920.0, 1080.0)]);
        assert_eq!(a, b);
        assert_eq!(a, DESK);
    }

    #[test]
    fn empty_or_missing_store_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(dir.path()).is_empty());
        assert!(get(dir.path(), "nope").is_none());
        std::fs::write(dir.path().join("layouts.json"), "not json").unwrap();
        assert!(list(dir.path()).is_empty());
        assert!(save(dir.path(), "   ", DESK, vec![]).is_err());
    }
}
