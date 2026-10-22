//! Sync the config across Macs by keeping the shareable files in a synced
//! folder (iCloud Drive, Dropbox, a git checkout…) and symlinking them back.
//!
//! Only files that make sense on every Mac move: `config.toml`, `scripts/`,
//! `notes.json`, `layouts.json`, `pinned.json`, `themes/`. Clipboard history,
//! usage stats, chats and caches stay local — they are private or
//! machine-specific, and two Macs writing them at once would conflict.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const SHARED: &[&str] = &[
    "config.toml",
    "scripts",
    "notes.json",
    "layouts.json",
    "pinned.json",
    "themes",
];

/// `~/Library/Mobile Documents/com~apple~CloudDocs/OpenCast`.
pub fn icloud_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Library/Mobile Documents/com~apple~CloudDocs/OpenCast"))
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Moved into the synced folder (this Mac had them).
    pub moved: Vec<String>,
    /// Already in the synced folder (another Mac set it up): local copy kept as `*.local-backup`.
    pub adopted: Vec<String>,
}

/// Link the shared files of `config_dir` into `target`. Safe to run twice and
/// on a second Mac: an existing synced copy wins, the local one is backed up.
pub fn enable(config_dir: &Path, target: &Path) -> Result<Report> {
    std::fs::create_dir_all(target).with_context(|| format!("creating {}", target.display()))?;
    let mut report = Report::default();
    for name in SHARED {
        let local = config_dir.join(name);
        let remote = target.join(name);
        // Already linked?
        if let Ok(link) = std::fs::read_link(&local) {
            if link == remote {
                continue;
            }
        }
        let local_exists = local.symlink_metadata().is_ok();
        let remote_exists = remote.symlink_metadata().is_ok();
        match (local_exists, remote_exists) {
            (false, false) => continue,
            (true, false) => {
                move_path(&local, &remote)?;
                report.moved.push(name.to_string());
            }
            (true, true) => {
                let backup = config_dir.join(format!("{name}.local-backup"));
                if backup.symlink_metadata().is_ok() {
                    bail!("{} exists; remove it and retry", backup.display());
                }
                std::fs::rename(&local, &backup).with_context(|| format!("backing up {}", local.display()))?;
                report.adopted.push(name.to_string());
            }
            (false, true) => report.adopted.push(name.to_string()),
        }
        symlink(&remote, &local)?;
    }
    Ok(report)
}

/// True when `config.toml` is a link into a synced folder.
pub fn is_enabled(config_dir: &Path) -> bool {
    std::fs::read_link(config_dir.join("config.toml")).is_ok()
}

fn move_path(from: &Path, to: &Path) -> Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // Different volume: copy, then remove.
    copy_recursive(from, to)?;
    if from.is_dir() {
        std::fs::remove_dir_all(from)?;
    } else {
        std::fs::remove_file(from)?;
    }
    Ok(())
}

fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_recursive(&e.path(), &to.join(e.file_name()))?;
        }
    } else {
        std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).with_context(|| format!("linking {}", link.display()))
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> Result<()> {
    bail!("sync needs symlinks (macOS/Linux)")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn first_mac_moves_second_mac_adopts() {
        let mac1 = tempfile::tempdir().unwrap();
        let mac2 = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        std::fs::write(mac1.path().join("config.toml"), "# mac1").unwrap();
        std::fs::create_dir(mac1.path().join("scripts")).unwrap();
        std::fs::write(mac1.path().join("scripts/a.sh"), "x").unwrap();
        std::fs::write(mac1.path().join("clipboard.json"), "[]").unwrap();

        let r = enable(mac1.path(), cloud.path()).unwrap();
        assert_eq!(r.moved, vec!["config.toml", "scripts"]);
        assert!(is_enabled(mac1.path()));
        assert_eq!(
            std::fs::read_to_string(mac1.path().join("config.toml")).unwrap(),
            "# mac1"
        );
        assert!(mac1.path().join("scripts/a.sh").exists());
        // Private files stay put.
        assert!(!cloud.path().join("clipboard.json").exists());
        // Idempotent.
        assert_eq!(enable(mac1.path(), cloud.path()).unwrap(), Report::default());

        std::fs::write(mac2.path().join("config.toml"), "# mac2").unwrap();
        let r = enable(mac2.path(), cloud.path()).unwrap();
        assert!(r.adopted.contains(&"config.toml".to_string()));
        assert_eq!(
            std::fs::read_to_string(mac2.path().join("config.toml")).unwrap(),
            "# mac1"
        );
        assert_eq!(
            std::fs::read_to_string(mac2.path().join("config.toml.local-backup")).unwrap(),
            "# mac2"
        );
        // Writes go through the link to the synced copy.
        std::fs::write(mac2.path().join("config.toml"), "# edited").unwrap();
        assert_eq!(
            std::fs::read_to_string(mac1.path().join("config.toml")).unwrap(),
            "# edited"
        );
    }
}
