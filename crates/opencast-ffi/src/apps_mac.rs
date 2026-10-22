//! macOS app discovery via Spotlight, with an on-disk cache so the launcher
//! opens before the (slow) scan finishes. Falls back to the directory scan.
//!
//! Display names come from one `mdls` call over every bundle: most
//! `Info.plist` files are binary, so reading them as text (the old way) fell
//! back to the file name and lost localized names.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use opencast_core::apps::{self, AppEntry, MacAppHint};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    apps: Vec<CachedApp>,
}

#[derive(Serialize, Deserialize, Clone)]
struct CachedApp {
    name: String,
    path: PathBuf,
}

const CACHE_VERSION: u32 = 1;

fn cache_path(config_dir: &Path) -> PathBuf {
    config_dir.join("apps.json")
}

/// Cached index if there is one (instant), else a full scan.
pub fn discover_cached(config_dir: &Path) -> Vec<AppEntry> {
    if let Some(apps) = load_cache(config_dir) {
        if !apps.is_empty() {
            return apps;
        }
    }
    let apps = discover();
    save_cache(config_dir, &apps);
    apps
}

/// Full scan; writes the cache. Returns true when the list changed.
pub fn refresh_cache(config_dir: &Path) -> bool {
    let fresh = discover();
    let same = load_cache(config_dir).is_some_and(|old| old == fresh);
    if !same {
        save_cache(config_dir, &fresh);
    }
    !same
}

fn load_cache(config_dir: &Path) -> Option<Vec<AppEntry>> {
    let raw = std::fs::read_to_string(cache_path(config_dir)).ok()?;
    let cache: Cache = serde_json::from_str(&raw).ok()?;
    if cache.version != CACHE_VERSION {
        return None;
    }
    Some(
        cache
            .apps
            .into_iter()
            .filter(|a| a.path.exists())
            .map(|a| AppEntry {
                name: a.name,
                path: a.path,
            })
            .collect(),
    )
}

fn save_cache(config_dir: &Path, apps: &[AppEntry]) {
    let cache = Cache {
        version: CACHE_VERSION,
        apps: apps
            .iter()
            .map(|a| CachedApp {
                name: a.name.clone(),
                path: a.path.clone(),
            })
            .collect(),
    };
    let _ = std::fs::create_dir_all(config_dir);
    if let Ok(raw) = serde_json::to_string(&cache) {
        let _ = std::fs::write(cache_path(config_dir), raw);
    }
}

/// Every Launch Services app Spotlight knows about, filtered in core.
pub fn discover() -> Vec<AppEntry> {
    match spotlight_hints() {
        Ok(hints) if !hints.is_empty() => {
            let apps = apps::filter_mac_apps(hints);
            if apps.is_empty() {
                apps::scan()
            } else {
                apps
            }
        }
        _ => apps::scan(),
    }
}

fn spotlight_hints() -> Result<Vec<MacAppHint>, ()> {
    let out = Command::new("mdfind")
        .arg("kMDItemContentTypeTree == \"com.apple.application-bundle\"")
        .output()
        .map_err(|_| ())?;
    if !out.status.success() {
        return Err(());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let paths: Vec<PathBuf> = text
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .filter(|p| !apps::is_nested_app(p))
        .collect();
    let names = display_names(&paths);
    let mut hints = Vec::new();
    for path in paths {
        let plist = path.join("Contents/Info.plist");
        // Text plists still give LSBackgroundOnly; binary ones read as empty.
        let src = std::fs::read_to_string(&plist).unwrap_or_default();
        let (display_name, bundle_name, background_only) = apps::parse_info_plist(&src);
        let md_name = names.get(&path).cloned().filter(|n| !n.is_empty());
        hints.push(MacAppHint {
            display_name: md_name.or(display_name),
            bundle_name,
            background_only,
            path,
        });
    }
    Ok(hints)
}

/// `kMDItemDisplayName` for many bundles in one `mdls` call. Values are
/// NUL-separated with `-raw`, `(null)` when missing.
fn display_names(paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut out = HashMap::new();
    if paths.is_empty() {
        return out;
    }
    // Keep argv well under ARG_MAX.
    for chunk in paths.chunks(400) {
        let res = Command::new("mdls")
            .args(["-name", "kMDItemDisplayName", "-raw"])
            .args(chunk)
            .output();
        let Ok(res) = res else { continue };
        let text = String::from_utf8_lossy(&res.stdout);
        for (path, value) in chunk.iter().zip(text.split('\0')) {
            let v = value.trim();
            if v.is_empty() || v == "(null)" {
                continue;
            }
            let v = v.strip_suffix(".app").unwrap_or(v);
            out.insert(path.clone(), v.to_string());
        }
    }
    out
}
