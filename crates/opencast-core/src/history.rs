//! Browser history: Chrome-family SQLite `History` files (copy then query —
//! browsers lock the live DB) plus Safari's `History.db` when readable.
//! The shell refreshes the cache (`history.json`) in the background; search
//! reads it from memory.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bookmarks::CHROMIUM;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    pub title: String,
    pub url: String,
    /// `Chrome`, `Safari`, `Brave`…
    pub source: String,
}

const CACHE: &str = "history.json";
/// Recent rows kept per browser store; enough for search without a huge cache.
const LIMIT: usize = 200;

pub fn load_cache(dir: &Path) -> Vec<Visit> {
    std::fs::read_to_string(dir.join(CACHE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the cache; true when it changed.
pub fn save_cache(dir: &Path, list: &[Visit]) -> bool {
    if load_cache(dir) == list {
        return false;
    }
    if let Ok(s) = serde_json::to_string(list) {
        let _ = std::fs::write(dir.join(CACHE), s);
    }
    true
}

/// Every Chrome-family `History` SQLite file (all profiles).
pub fn chromium_files(app_support: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (name, rel) in CHROMIUM {
        let base = app_support.join(rel);
        let Ok(entries) = std::fs::read_dir(&base) else {
            if base.join("History").is_file() {
                out.push((name.to_string(), base.join("History")));
            }
            continue;
        };
        for e in entries.flatten() {
            let p = e.path().join("History");
            if p.is_file() {
                out.push((name.to_string(), p));
            }
        }
        if base.join("History").is_file() {
            out.push((name.to_string(), base.join("History")));
        }
    }
    out
}

/// Safari history store — FDA-protected like its bookmarks.
pub fn safari_store(home: &Path) -> PathBuf {
    home.join("Library/Safari/History.db")
}

/// Rows from a Chrome-family History DB (after copy). Shared with tests.
pub fn parse_chromium_json(json: &str, source: &str) -> Vec<Visit> {
    parse_json_rows(json, source)
}

/// Rows from a Safari History.db query JSON.
pub fn parse_safari_json(json: &str) -> Vec<Visit> {
    parse_json_rows(json, "Safari")
}

fn parse_json_rows(json: &str, source: &str) -> Vec<Visit> {
    let Ok(rows) = serde_json::from_str::<Vec<Value>>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in rows {
        let url = row.get("url").and_then(Value::as_str).unwrap_or("");
        let title = row.get("title").and_then(Value::as_str).unwrap_or("");
        push(&mut out, title, url, source);
    }
    out
}

fn push(out: &mut Vec<Visit>, title: &str, url: &str, source: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    let title = if title.trim().is_empty() { url } else { title.trim() };
    out.push(Visit {
        title: title.to_string(),
        url: url.to_string(),
        source: source.to_string(),
    });
}

/// Drop duplicate URLs (first wins — callers should order newest first).
pub fn dedup(list: Vec<Visit>) -> Vec<Visit> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter().filter(|v| seen.insert(v.url.clone())).collect()
}

/// Copy the live DB (browsers hold a lock) and query via `sqlite3`.
// ponytail: sqlite3 CLI until we need rusqlite
fn query_copied(src: &Path, sql: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!(
        "opencast-hist-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join("History.db");
    if std::fs::copy(src, &dest).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    // WAL companions improve consistency when the browser is open; ignore misses.
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", src.display()));
        if side.is_file() {
            let name = format!("History.db{suffix}");
            let _ = std::fs::copy(&side, dir.join(name));
        }
    }
    let out = Command::new("sqlite3")
        .args(["-json", &dest.to_string_lossy(), sql])
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        .filter(|s| !s.trim().is_empty())
}

/// Recent URLs from one Chrome-family History file.
pub fn read_chromium(path: &Path, source: &str) -> Vec<Visit> {
    let sql = format!(
        "SELECT url, ifnull(title,'') AS title FROM urls \
         WHERE hidden = 0 AND (url LIKE 'http://%' OR url LIKE 'https://%') \
         ORDER BY last_visit_time DESC LIMIT {LIMIT}"
    );
    query_copied(path, &sql)
        .map(|j| parse_chromium_json(&j, source))
        .unwrap_or_default()
}

/// Recent Safari history when the DB is readable (TCC may block).
pub fn read_safari(path: &Path) -> Vec<Visit> {
    let sql = format!(
        "SELECT hi.url AS url, ifnull(hv.title,'') AS title \
         FROM history_visits hv \
         JOIN history_items hi ON hi.id = hv.history_item \
         WHERE hi.url LIKE 'http://%' OR hi.url LIKE 'https://%' \
         ORDER BY hv.visit_time DESC LIMIT {LIMIT}"
    );
    query_copied(path, &sql)
        .map(|j| parse_safari_json(&j))
        .unwrap_or_default()
}

/// Collect every readable store into one deduped list (newest-first per store).
pub fn collect(app_support: &Path, home: &Path) -> Vec<Visit> {
    let mut all = Vec::new();
    for (name, file) in chromium_files(app_support) {
        all.extend(read_chromium(&file, &name));
    }
    let safari = safari_store(home);
    if safari.is_file() {
        all.extend(read_safari(&safari));
    }
    dedup(all)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scan {
    pub found: Vec<String>,
    pub blocked: Vec<String>,
}

fn openable(path: &Path) -> bool {
    match std::fs::File::open(path) {
        Ok(_) => true,
        Err(e) => {
            e.kind() != std::io::ErrorKind::NotFound
                && e.kind() != std::io::ErrorKind::PermissionDenied
                && e.raw_os_error() != Some(1)
        }
    }
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Look at every history store without reading rows (safe on a hot path).
pub fn scan(app_support: &Path, home: &Path) -> Scan {
    let mut s = Scan::default();
    for (name, rel) in CHROMIUM {
        let base = app_support.join(rel);
        if !exists(&base) {
            continue;
        }
        if base.join("History").is_file() {
            s.found.push((*name).to_string());
            continue;
        }
        match std::fs::read_dir(&base) {
            Ok(entries) => {
                if entries.flatten().any(|e| e.path().join("History").is_file()) {
                    s.found.push((*name).to_string());
                }
            }
            Err(_) => s.blocked.push((*name).to_string()),
        }
    }
    let safari = safari_store(home);
    if exists(&safari) {
        if openable(&safari) {
            s.found.push("Safari".to_string());
        } else {
            s.blocked.push("Safari".to_string());
        }
    }
    s.found.sort();
    s.blocked.sort();
    s
}

pub fn scan_here() -> Scan {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    if cfg!(target_os = "macos") {
        scan(&home.join("Library/Application Support"), &home)
    } else {
        Scan::default()
    }
}

pub fn why_empty(scan: &Scan) -> String {
    let browsers = "Chrome, Brave, Edge, Vivaldi, Chromium, Opera and Safari";
    if !scan.blocked.is_empty() {
        let stores = scan.blocked.join(", ");
        let many = if scan.blocked.len() == 1 { "" } else { "s" };
        return format!(
            "OpenCast cannot read your {stores} history store{many}, so there is nothing to search. \
             Grant Full Disk Access: run the \u{201c}Privacy: Full Disk Access\u{201d} command, switch OpenCast \
             on in System Settings \u{2192} Privacy & Security \u{2192} Full Disk Access, then relaunch OpenCast. \
             (Safari's history lives in ~/Library/Safari, which macOS protects from every app.)"
        );
    }
    if scan.found.is_empty() {
        return format!(
            "No browser history store was found. OpenCast reads {browsers}; the cache is written to \
             history.json in the config folder when OpenCast starts."
        );
    }
    let stores = scan.found.join(", ");
    let many = if scan.found.len() == 1 { "" } else { "s" };
    format!("OpenCast read your {stores} history store{many} but found no recent web visits in them.")
}

/// `hist` / `history`. Search matches these; OpenCast Keys lists them.
pub const PREFIXES: &[(&str, &str)] = &[("history", "Browser history"), ("hist", "Browser history")];

/// `history …` / `hist …` lists history; a plain 3+ letter query shows a few.
pub fn needle(query: &str) -> (Option<String>, bool) {
    let q = query.trim().to_lowercase();
    for (prefix, _) in PREFIXES {
        if q == *prefix {
            return (Some(String::new()), true);
        }
        if let Some(rest) = q.strip_prefix(&format!("{prefix} ")) {
            return (Some(rest.trim().to_string()), true);
        }
    }
    if q.chars().count() >= 3 {
        (Some(q), false)
    } else {
        (None, false)
    }
}

pub fn matching(list: &[Visit], query: &str) -> Vec<crate::commands::Command> {
    let (Some(needle), explicit) = needle(query) else {
        return Vec::new();
    };
    let limit = if explicit { 40 } else { 4 };
    let words: Vec<&str> = needle.split_whitespace().collect();
    list.iter()
        .filter(|v| {
            let hay = format!("{} {}", v.title, v.url).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .take(limit)
        .map(|v| crate::commands::Command {
            id: format!("hist:{}", v.url),
            title: v.title.clone(),
            subtitle: Some(format!(
                "History · {} · {}",
                v.source,
                v.url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_start_matches("www.")
            )),
            icon: Some("🕘".into()),
            kind: crate::commands::CommandKind::Quicklink,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["history".into()],
        })
        .collect()
}

#[cfg(test)]
fn hide(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chromium_and_safari_json() {
        let chrome = r#"[{"url":"https://rust-lang.org/","title":"Rust"},{"url":"chrome://newtab/","title":"New Tab"},{"url":"https://example.com/","title":""}]"#;
        let list = parse_chromium_json(chrome, "Chrome");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "Rust");
        assert_eq!(list[0].source, "Chrome");
        assert_eq!(list[1].title, "https://example.com/");

        let safari = r#"[{"url":"https://apple.com/","title":"Apple"}]"#;
        let s = parse_safari_json(safari);
        assert_eq!(s[0].source, "Safari");
        assert_eq!(s[0].title, "Apple");
    }

    #[test]
    fn query_copied_fixture_sqlite() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("History");
        let status = Command::new("sqlite3")
            .arg(&db)
            .arg(
                "CREATE TABLE urls (url TEXT, title TEXT, last_visit_time INTEGER, hidden INTEGER DEFAULT 0);\
                 INSERT INTO urls VALUES ('https://grafana.example.com','Grafana', 9, 0);\
                 INSERT INTO urls VALUES ('https://kagi.com','Kagi', 8, 0);\
                 INSERT INTO urls VALUES ('chrome://settings','Settings', 7, 0);",
            )
            .status();
        let Ok(st) = status else {
            return; // no sqlite3 in PATH (unusual on macOS)
        };
        if !st.success() {
            return;
        }
        let rows = read_chromium(&db, "Chrome");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "https://grafana.example.com");
        assert_eq!(rows[0].title, "Grafana");
        assert_eq!(rows[1].title, "Kagi");
    }

    #[test]
    fn stores_are_found_or_blocked() {
        let root = tempfile::tempdir().unwrap();
        let support = root.path().join("AppSupport");
        let home = root.path().join("home");
        let profile = support.join("Google/Chrome/Default");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("History"), "sqlite").unwrap();
        let safari = home.join("Library/Safari");
        std::fs::create_dir_all(&safari).unwrap();
        let db = safari.join("History.db");
        std::fs::write(&db, "sqlite").unwrap();
        hide(&db);

        let scan = scan(&support, &home);
        assert_eq!(scan.found, vec!["Chrome".to_string()]);
        if std::fs::File::open(&db).is_err() {
            assert_eq!(scan.blocked, vec!["Safari".to_string()]);
            let why = why_empty(&scan);
            assert!(why.contains("Safari") && why.contains("Full Disk Access"), "{why}");
        }
    }

    #[test]
    fn search_modes() {
        let list = dedup(vec![
            Visit {
                title: "Grafana Prod".into(),
                url: "https://grafana.example.com".into(),
                source: "Chrome".into(),
            },
            Visit {
                title: "Grafana copy".into(),
                url: "https://grafana.example.com".into(),
                source: "Safari".into(),
            },
            Visit {
                title: "Kagi".into(),
                url: "https://kagi.com".into(),
                source: "Safari".into(),
            },
        ]);
        assert_eq!(list.len(), 2);
        let rows = matching(&list, "hist");
        assert_eq!(rows.len(), 2);
        assert!(rows[0].subtitle.as_ref().unwrap().starts_with("History · "));
        assert_eq!(matching(&list, "graf")[0].id, "hist:https://grafana.example.com");
        assert!(matching(&list, "gr").is_empty());
        assert_eq!(matching(&list, "hist kagi")[0].title, "Kagi");
    }

    #[test]
    fn no_stores_names_browsers() {
        let root = tempfile::tempdir().unwrap();
        let scan = scan(&root.path().join("nope"), &root.path().join("home"));
        assert!(scan.found.is_empty() && scan.blocked.is_empty());
        let why = why_empty(&scan);
        assert!(why.contains("Chrome") && why.contains("Safari"), "{why}");
    }
}
