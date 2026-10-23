//! File search helpers (paths from Spotlight / mdfind on macOS).

use std::path::{Path, PathBuf};

use crate::commands::{Command, CommandKind};

/// Turn absolute paths into searchable file commands.
pub fn to_commands(paths: impl IntoIterator<Item = PathBuf>, limit: usize) -> Vec<Command> {
    let mut out = Vec::new();
    for path in paths {
        if out.len() >= limit {
            break;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("app") {
            continue;
        }
        if !meta.is_dir() && !meta.is_file() {
            continue;
        }
        let is_dir = meta.is_dir();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(if is_dir { "folder" } else { "file" })
            .to_string();
        let title = if is_dir { format!("{name}/") } else { name };
        let parent = path.parent().map(shorten_home).unwrap_or_default();
        out.push(Command {
            id: format!("file:{}", path.display()),
            title,
            subtitle: Some(parent),
            icon: Some(if is_dir { "folder.fill" } else { "doc.fill" }.into()),
            kind: CommandKind::File,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["file".into(), "find".into(), "document".into()],
        });
    }
    out
}

/// Path mode: `~/Doc` lists `~/` filtered by `doc`; `~/Documents/` lists that folder.
/// Returns None when `query` is not a path. Folders first, then by fuzzy rank.
pub fn list_path(query: &str, show_hidden: bool, limit: usize) -> Option<Vec<Command>> {
    let q = query.trim();
    if !(q.starts_with('/') || q.starts_with("~/") || q == "~") {
        return None;
    }
    let expanded = if let Some(rest) = q.strip_prefix('~') {
        dirs::home_dir()?.to_string_lossy().into_owned() + rest
    } else {
        q.to_string()
    };
    let (dir, needle) = if q == "~" || expanded.ends_with('/') {
        (PathBuf::from(&expanded), String::new())
    } else {
        let p = PathBuf::from(&expanded);
        let needle = p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (
            p.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/")),
            needle,
        )
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Some(Vec::new());
    };
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    use fuzzy_matcher::FuzzyMatcher;
    let mut rows: Vec<(bool, i64, String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') && !needle.starts_with('.') {
                return None;
            }
            let score = if needle.is_empty() {
                0
            } else {
                let base = matcher.fuzzy_match(&name, &needle)?;
                base + if name.to_lowercase().starts_with(&needle) {
                    200
                } else {
                    0
                }
            };
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false) && !name.ends_with(".app");
            Some((is_dir, score, name, e.path()))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.0.cmp(&a.0))
            .then(a.2.to_lowercase().cmp(&b.2.to_lowercase()))
    });
    Some(
        rows.into_iter()
            .take(limit)
            .map(|(is_dir, _, name, path)| Command {
                id: format!("file:{}", path.display()),
                title: if is_dir { format!("{name}/") } else { name },
                subtitle: Some(shorten_home(&dir)),
                icon: Some(if is_dir { "folder.fill" } else { "doc.fill" }.into()),
                kind: CommandKind::File,
                arguments: vec![],
                hotkey: None,
                keywords: vec![],
            })
            .collect(),
    )
}

fn shorten_home(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_file_commands() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("notes.txt");
        std::fs::write(&f, "hi").unwrap();
        let cmds = to_commands(vec![f.clone()], 5);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].title, "notes.txt");
        assert_eq!(cmds[0].icon.as_deref(), Some("doc.fill"));
        assert!(cmds[0].id.starts_with("file:"));
    }

    #[test]
    fn builds_folder_commands() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Projects");
        std::fs::create_dir(&folder).unwrap();
        let cmds = to_commands(vec![folder.clone()], 5);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].title, "Projects/");
        assert_eq!(cmds[0].icon.as_deref(), Some("folder.fill"));
        let parent = shorten_home(dir.path());
        assert_eq!(cmds[0].subtitle.as_deref(), Some(parent.as_str()));
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn lists_and_filters_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Documents")).unwrap();
        std::fs::create_dir(dir.path().join("Downloads")).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        std::fs::write(dir.path().join(".secret"), "x").unwrap();
        let base = dir.path().to_string_lossy().to_string();
        let all = list_path(&format!("{base}/"), false, 50).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].title, "Documents/", "folders first");
        assert!(list_path(&format!("{base}/"), true, 50).unwrap().len() == 4);
        let d = list_path(&format!("{base}/do"), false, 50).unwrap();
        assert!(d.iter().all(|c| c.title.starts_with("Do")));
        assert!(list_path("hello", false, 50).is_none());
        assert!(list_path(&format!("{base}/missing/"), false, 50).unwrap().is_empty());
    }
}
