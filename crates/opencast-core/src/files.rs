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
        if !path.exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let parent = path
            .parent()
            .map(|p| shorten_home(p))
            .unwrap_or_default();
        out.push(Command {
            id: format!("file:{}", path.display()),
            title: name,
            subtitle: Some(parent),
            icon: Some("📄".into()),
            kind: CommandKind::File,
            arguments: vec![],
            hotkey: None,
            keywords: vec!["file".into(), "find".into(), "document".into()],
        });
    }
    out
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
        assert!(cmds[0].id.starts_with("file:"));
    }
}
