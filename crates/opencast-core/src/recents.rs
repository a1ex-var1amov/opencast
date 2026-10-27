//! Recent documents via Spotlight (`mdfind` + `kMDItemLastUsedDate`).
//! The shell refreshes the cache (`recents.json`) in the background; search
//! reads it from memory.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    pub path: String,
    pub name: String,
    /// Spotlight `kMDItemLastUsedDate` string when known (newest-first order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

const CACHE: &str = "recents.json";
/// Enough for search without a huge cache.
const LIMIT: usize = 50;

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

fn shorten_home(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}

/// Build a launcher row. Uses `file:` so existing open plan works.
pub fn to_command(v: &Visit) -> crate::commands::Command {
    let path = Path::new(&v.path);
    let parent = path.parent().map(shorten_home).unwrap_or_else(|| "~/".into());
    crate::commands::Command {
        id: format!("file:{}", v.path),
        title: v.name.clone(),
        subtitle: Some(format!("Recent · {parent}")),
        icon: Some("doc.fill".into()),
        kind: crate::commands::CommandKind::File,
        arguments: vec![],
        hotkey: None,
        keywords: vec!["recent".into(), "recents".into(), "document".into()],
    }
}

/// Parse `mdfind -attr kMDItemLastUsedDate` lines (shared with tests).
pub fn parse_mdfind_attr(text: &str) -> Vec<(String, String)> {
    let marker = "   kMDItemLastUsedDate = ";
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(i) = line.find(marker) else {
            continue;
        };
        let path = line[..i].to_string();
        let used = line[i + marker.len()..].trim().to_string();
        if path.is_empty() || used.is_empty() {
            continue;
        }
        out.push((path, used));
    }
    out
}

fn name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn under_library(path: &str) -> bool {
    path.contains("/Library/")
}

/// Drop duplicates and Library paths; keep newest-first order from callers.
pub fn dedup(list: Vec<Visit>) -> Vec<Visit> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter()
        .filter(|v| !under_library(&v.path) && seen.insert(v.path.clone()))
        .collect()
}

/// Rows from mdfind attr output, newest first, capped.
pub fn from_mdfind(text: &str, limit: usize) -> Vec<Visit> {
    let mut rows = parse_mdfind_attr(text);
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    dedup(
        rows.into_iter()
            .map(|(path, used)| Visit {
                name: name_of(&path),
                path,
                last_used: Some(used),
            })
            .collect(),
    )
    .into_iter()
    .take(limit)
    .collect()
}

// ponytail: mdfind CLI until we need MDQuery (same ceiling as history's sqlite3)
fn run_mdfind(home: &Path) -> Option<String> {
    let out = Command::new("mdfind")
        .args([
            "-attr",
            "kMDItemLastUsedDate",
            "kMDItemLastUsedDate >= $time.today(-60)",
            "-onlyin",
        ])
        .arg(home)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        .filter(|s| !s.trim().is_empty())
}

/// Collect recent documents under `$HOME` (macOS Spotlight). Empty elsewhere.
pub fn collect(home: &Path) -> Vec<Visit> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    run_mdfind(home).map(|t| from_mdfind(&t, LIMIT)).unwrap_or_default()
}

pub fn scan_here() -> Vec<Visit> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    collect(&home)
}

/// `recent` / `recents`. Search matches these; OpenCast Keys lists them.
pub const PREFIXES: &[(&str, &str)] = &[("recents", "Recent files"), ("recent", "Recent files")];

/// `recent …` / `recents …` lists them; a plain 3+ letter query shows a few.
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
            if words.is_empty() {
                return true;
            }
            let hay = format!("{} {}", v.name, v.path).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .take(limit)
        .map(to_command)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let list = vec![Visit {
            path: "/Users/me/Notes/todo.md".into(),
            name: "todo.md".into(),
            last_used: Some("2026-10-20 12:00:00 +0000".into()),
        }];
        assert!(save_cache(dir.path(), &list));
        assert_eq!(load_cache(dir.path()), list);
        assert!(!save_cache(dir.path(), &list));
    }

    #[test]
    fn parse_and_filter_library() {
        let text = "\
/Users/me/Library/Caches/x.txt   kMDItemLastUsedDate = 2026-10-22 10:00:00 +0000
/Users/me/Documents/report.pdf   kMDItemLastUsedDate = 2026-10-21 09:00:00 +0000
/Users/me/Desktop/notes.md   kMDItemLastUsedDate = 2026-10-22 11:00:00 +0000
";
        let list = from_mdfind(text, 10);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "notes.md");
        assert_eq!(list[1].path, "/Users/me/Documents/report.pdf");
        assert!(list[0].last_used.as_deref().unwrap().contains("2026-10-22"));
    }

    #[test]
    fn to_command_uses_file_id() {
        let v = Visit {
            path: "/Users/me/Documents/report.pdf".into(),
            name: "report.pdf".into(),
            last_used: None,
        };
        let c = to_command(&v);
        assert_eq!(c.id, "file:/Users/me/Documents/report.pdf");
        assert_eq!(c.kind, crate::commands::CommandKind::File);
        assert!(c.subtitle.as_ref().unwrap().starts_with("Recent · "));
    }

    #[test]
    fn search_modes() {
        let list = vec![
            Visit {
                path: "/Users/me/Documents/Grafana.pdf".into(),
                name: "Grafana.pdf".into(),
                last_used: None,
            },
            Visit {
                path: "/Users/me/Desktop/kagi-notes.txt".into(),
                name: "kagi-notes.txt".into(),
                last_used: None,
            },
        ];
        assert_eq!(matching(&list, "recent").len(), 2);
        assert_eq!(matching(&list, "recents graf")[0].title, "Grafana.pdf");
        assert_eq!(matching(&list, "graf")[0].id, "file:/Users/me/Documents/Grafana.pdf");
        assert!(matching(&list, "gr").is_empty());
    }
}
