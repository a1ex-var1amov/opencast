//! Browser bookmarks: Chrome-family JSON files (Chrome, Brave, Edge, Arc,
//! Vivaldi, Chromium) parsed here; Safari's binary plist is converted to JSON
//! by the shell (`plutil`) and parsed here too. The shell refreshes the cache
//! (`bookmarks.json`) in the background; search reads it from memory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    /// `Chrome`, `Safari`, `Arc`…
    pub source: String,
    /// Folder path, e.g. `Bookmarks Bar › Work`.
    #[serde(default)]
    pub folder: String,
}

const CACHE: &str = "bookmarks.json";

pub fn load_cache(dir: &Path) -> Vec<Bookmark> {
    std::fs::read_to_string(dir.join(CACHE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the cache; true when it changed.
pub fn save_cache(dir: &Path, list: &[Bookmark]) -> bool {
    if load_cache(dir) == list {
        return false;
    }
    if let Ok(s) = serde_json::to_string(list) {
        let _ = std::fs::write(dir.join(CACHE), s);
    }
    true
}

/// Chrome-family browsers: (name, folder under ~/Library/Application Support).
pub const CHROMIUM: &[(&str, &str)] = &[
    ("Chrome", "Google/Chrome"),
    ("Brave", "BraveSoftware/Brave-Browser"),
    ("Edge", "Microsoft Edge"),
    ("Vivaldi", "Vivaldi"),
    ("Chromium", "Chromium"),
    ("Opera", "com.operasoftware.Opera"),
];

/// Every `Bookmarks` file of a Chrome-family browser (all profiles).
pub fn chromium_files(app_support: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (name, rel) in CHROMIUM {
        let base = app_support.join(rel);
        let Ok(entries) = std::fs::read_dir(&base) else {
            // Opera keeps the file at the top level.
            if base.join("Bookmarks").is_file() {
                out.push((name.to_string(), base.join("Bookmarks")));
            }
            continue;
        };
        for e in entries.flatten() {
            let p = e.path().join("Bookmarks");
            if p.is_file() {
                out.push((name.to_string(), p));
            }
        }
        if base.join("Bookmarks").is_file() {
            out.push((name.to_string(), base.join("Bookmarks")));
        }
    }
    out
}

pub fn parse_chromium(json: &str, source: &str) -> Vec<Bookmark> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(roots) = v.get("roots").and_then(Value::as_object) {
        for (_, root) in roots {
            walk_chromium(root, source, "", &mut out);
        }
    }
    out
}

fn walk_chromium(node: &Value, source: &str, folder: &str, out: &mut Vec<Bookmark>) {
    match node.get("type").and_then(Value::as_str) {
        Some("url") => {
            if let (Some(name), Some(url)) = (node["name"].as_str(), node["url"].as_str()) {
                push(out, name, url, source, folder);
            }
        }
        _ => {
            let name = node["name"].as_str().unwrap_or("");
            let path = join(folder, name);
            for c in node["children"].as_array().into_iter().flatten() {
                walk_chromium(c, source, &path, out);
            }
        }
    }
}

/// Safari `Bookmarks.plist` as JSON (`plutil -convert json`).
pub fn parse_safari(json: &str) -> Vec<Bookmark> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_safari(&v, "", &mut out);
    out
}

fn walk_safari(node: &Value, folder: &str, out: &mut Vec<Bookmark>) {
    match node.get("WebBookmarkType").and_then(Value::as_str) {
        Some("WebBookmarkTypeLeaf") => {
            let url = node["URLString"].as_str().unwrap_or("");
            let title = node["URIDictionary"]["title"].as_str().unwrap_or(url);
            push(out, title, url, "Safari", folder);
        }
        _ => {
            let title = node["Title"].as_str().unwrap_or("");
            // Reading List lives in Safari's tree too; keep it, labelled.
            let title = match title {
                "BookmarksBar" => "Favorites",
                "BookmarksMenu" => "Bookmarks Menu",
                "com.apple.ReadingList" => "Reading List",
                t => t,
            };
            let path = join(folder, title);
            for c in node["Children"].as_array().into_iter().flatten() {
                walk_safari(c, &path, out);
            }
        }
    }
}

/// Arc keeps pinned tabs/bookmarks in `StorableSidebar.json`.
pub fn parse_arc(json: &str) -> Vec<Bookmark> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<Bookmark>) {
        match v {
            Value::Object(o) => {
                if let Some(tab) = o.get("data").and_then(|d| d.get("tab")) {
                    if let Some(url) = tab["savedURL"].as_str() {
                        let title = o
                            .get("title")
                            .and_then(Value::as_str)
                            .or_else(|| tab["savedTitle"].as_str())
                            .unwrap_or(url);
                        push(out, title, url, "Arc", "");
                    }
                }
                for (_, x) in o {
                    walk(x, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(&v, &mut out);
    out
}

fn join(folder: &str, name: &str) -> String {
    match (folder.is_empty(), name.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => folder.to_string(),
        _ => format!("{folder} › {name}"),
    }
}

fn push(out: &mut Vec<Bookmark>, title: &str, url: &str, source: &str, folder: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return; // javascript: bookmarklets, chrome:// pages
    }
    let title = if title.trim().is_empty() { url } else { title.trim() };
    out.push(Bookmark {
        title: title.to_string(),
        url: url.to_string(),
        source: source.to_string(),
        folder: folder.to_string(),
    });
}

/// Drop duplicate URLs (first one wins), keep order.
pub fn dedup(list: Vec<Bookmark>) -> Vec<Bookmark> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter().filter(|b| seen.insert(b.url.clone())).collect()
}

/// `bm …` / `bookmarks …` lists bookmarks; a plain 3+ letter query shows a few.
pub fn needle(query: &str) -> (Option<String>, bool) {
    let q = query.trim().to_lowercase();
    for prefix in ["bookmarks", "bookmark", "bm"] {
        if q == prefix {
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

pub fn matching(list: &[Bookmark], query: &str) -> Vec<crate::commands::Command> {
    let (Some(needle), explicit) = needle(query) else {
        return Vec::new();
    };
    let limit = if explicit { 40 } else { 4 };
    let words: Vec<&str> = needle.split_whitespace().collect();
    list.iter()
        .filter(|b| {
            let hay = format!("{} {}", b.title, b.url).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .take(limit)
        .map(|b| crate::commands::Command {
            id: format!("bm:{}", b.url),
            title: b.title.clone(),
            subtitle: Some(format!(
                "{} · {}",
                b.source,
                b.url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_start_matches("www.")
            )),
            icon: Some("🔖".into()),
            kind: crate::commands::CommandKind::Quicklink,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["bookmark".into()],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chromium() {
        let json = r#"{"roots": {
            "bookmark_bar": {"type": "folder", "name": "Bookmarks bar", "children": [
                {"type": "url", "name": "Rust", "url": "https://www.rust-lang.org/"},
                {"type": "folder", "name": "Work", "children": [
                    {"type": "url", "name": "Grafana", "url": "https://grafana.example.com/d/abc"},
                    {"type": "url", "name": "Bookmarklet", "url": "javascript:alert(1)"}
                ]}
            ]},
            "other": {"type": "folder", "name": "Other", "children": []}
        }}"#;
        let list = parse_chromium(json, "Chrome");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].folder, "Bookmarks bar › Work");
        assert_eq!(list[1].source, "Chrome");
    }

    #[test]
    fn parses_safari_and_arc() {
        let safari = r#"{"Children": [{"Title": "BookmarksBar", "Children": [
            {"WebBookmarkType": "WebBookmarkTypeLeaf", "URLString": "https://apple.com", "URIDictionary": {"title": "Apple"}}
        ]}]}"#;
        let s = parse_safari(safari);
        assert_eq!(s[0].title, "Apple");
        assert_eq!(s[0].folder, "Favorites");
        let arc = r#"{"sidebar": {"containers": [{"items": [
            {"title": null, "data": {"tab": {"savedURL": "https://linear.app", "savedTitle": "Linear"}}}
        ]}]}}"#;
        let a = parse_arc(arc);
        assert_eq!(a[0].title, "Linear");
        assert_eq!(a[0].source, "Arc");
    }

    #[test]
    fn search_modes() {
        let list = dedup(vec![
            Bookmark {
                title: "Grafana Prod".into(),
                url: "https://grafana.example.com".into(),
                source: "Chrome".into(),
                folder: String::new(),
            },
            Bookmark {
                title: "Grafana copy".into(),
                url: "https://grafana.example.com".into(),
                source: "Safari".into(),
                folder: String::new(),
            },
            Bookmark {
                title: "Kagi".into(),
                url: "https://kagi.com".into(),
                source: "Safari".into(),
                folder: String::new(),
            },
        ]);
        assert_eq!(list.len(), 2);
        assert_eq!(matching(&list, "bm").len(), 2);
        assert_eq!(matching(&list, "graf")[0].id, "bm:https://grafana.example.com");
        assert!(matching(&list, "gr").is_empty());
        assert_eq!(matching(&list, "bm kagi")[0].title, "Kagi");
    }
}
