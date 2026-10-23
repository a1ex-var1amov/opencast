//! Background refreshes of the caches search reads: browser bookmarks and
//! exchange rates. Run at startup (off the main thread); each returns whether
//! its cache changed so the caller can reload once.

use std::path::{Path, PathBuf};
use std::process::Command;

use opencast_core::bookmarks::{self, Bookmark};
use opencast_core::currency;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Binary plist → JSON text via `plutil` (nothing on failure, e.g. no Full Disk Access).
fn plist_json(path: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(path)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn refresh_bookmarks(config_dir: &Path) -> bool {
    let support = home().join("Library/Application Support");
    let scan = bookmarks::scan(&support, &home());
    if !scan.blocked.is_empty() {
        // Without Full Disk Access macOS answers "Operation not permitted";
        // the launcher tells the user rather than caching nothing quietly.
        eprintln!(
            "opencast: bookmarks: cannot read {} — needs Full Disk Access (System Settings \u{2192} Privacy & Security \u{2192} Full Disk Access)",
            scan.blocked.join(", ")
        );
    }
    let mut all: Vec<Bookmark> = Vec::new();
    for (name, file) in bookmarks::chromium_files(&support) {
        if let Ok(text) = std::fs::read_to_string(&file) {
            all.extend(bookmarks::parse_chromium(&text, &name));
        }
    }
    if let Ok(text) = std::fs::read_to_string(bookmarks::arc_store(&support)) {
        all.extend(bookmarks::parse_arc(&text));
    }
    // Safari's folder needs Full Disk Access; without it this is skipped, and
    // the scan above is what reports it.
    if let Some(json) = plist_json(&bookmarks::safari_store(&home())) {
        all.extend(bookmarks::parse_safari(&json));
    }
    bookmarks::save_cache(config_dir, &bookmarks::dedup(all))
}

/// Only when `currency = true`: the launcher makes no requests the user did not turn on.
pub async fn refresh_rates(config_dir: &Path, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    let now = opencast_core::usage::now();
    if !currency::is_stale(currency::load(config_dir).as_ref(), now) {
        return false;
    }
    match currency::fetch().await {
        Ok(r) => {
            currency::save(config_dir, &r);
            true
        }
        Err(e) => {
            eprintln!("opencast: exchange rates: {e:#}");
            false
        }
    }
}
