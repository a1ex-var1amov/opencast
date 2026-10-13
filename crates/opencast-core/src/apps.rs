//! Installed application discovery and launching.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    /// `.app` bundle on macOS, `.desktop` file on Linux.
    pub path: PathBuf,
}

/// Scan the platform's application folders.
pub fn scan() -> Vec<AppEntry> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut apps = if cfg!(target_os = "macos") {
        scan_mac(&[
            PathBuf::from("/Applications"),
            PathBuf::from("/Applications/Utilities"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
            PathBuf::from("/System/Library/CoreServices/Applications"),
            home.join("Applications"),
        ])
    } else if cfg!(target_os = "linux") {
        scan_linux(&[
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            home.join(".local/share/flatpak/exports/share/applications"),
            home.join(".local/share/applications"),
        ])
    } else {
        Vec::new()
    };
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// `.app` bundles directly inside each folder; first name wins.
pub fn scan_mac(dirs: &[PathBuf]) -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("app") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            if seen.insert(name.clone()) {
                out.push(AppEntry { name, path });
            }
        }
    }
    out
}

/// `.desktop` entries; later folders (user-local) override earlier ones.
pub fn scan_linux(dirs: &[PathBuf]) -> Vec<AppEntry> {
    let mut by_id: std::collections::BTreeMap<String, AppEntry> = Default::default();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let id = e.file_name().to_string_lossy().to_string();
            match parse_desktop_entry(&src) {
                Some(name) => {
                    by_id.insert(id, AppEntry { name, path });
                }
                None => {
                    by_id.remove(&id);
                }
            }
        }
    }
    by_id.into_values().collect()
}

/// Returns the display name of a launchable `.desktop` entry, or `None` if
/// it is hidden or not an application.
pub fn parse_desktop_entry(src: &str) -> Option<String> {
    let mut in_main = false;
    let mut name = None;
    let mut is_app = false;
    for line in src.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "Name" if name.is_none() => name = Some(v.trim().to_string()),
            "Type" => is_app = v.trim() == "Application",
            "NoDisplay" | "Hidden" if v.trim() == "true" => return None,
            _ => {}
        }
    }
    if is_app {
        name.filter(|n| !n.is_empty())
    } else {
        None
    }
}

/// Launch an app found by [`scan`].
pub fn launch(path: &Path) -> Result<()> {
    if cfg!(target_os = "linux") && path.extension().and_then(|x| x.to_str()) == Some("desktop") {
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        Process::new("gtk-launch")
            .arg(id)
            .spawn()
            .context("launching with gtk-launch")?;
        return Ok(());
    }
    open_path(&path.to_string_lossy())
}

/// Open a file, folder or URL with the system handler.
pub fn open_path(target: &str) -> Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Process::new("open");
        c.arg(target);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Process::new("cmd");
        c.args(["/C", "start", "", target]);
        c
    } else {
        let mut c = Process::new("xdg-open");
        c.arg(target);
        c
    };
    cmd.spawn().with_context(|| format!("opening {target}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_parsing() {
        let ok = "[Desktop Entry]\nType=Application\nName=Firefox\nName[de]=Feuerfuchs\nExec=firefox %u\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(parse_desktop_entry(ok).as_deref(), Some("Firefox"));
        assert_eq!(
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n"),
            None
        );
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nType=Link\nName=X\n"), None);
    }

    #[test]
    fn mac_scan_dedupes_by_name() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::create_dir(a.path().join("Safari.app")).unwrap();
        std::fs::create_dir(b.path().join("Safari.app")).unwrap();
        std::fs::create_dir(b.path().join("Notes.app")).unwrap();
        std::fs::create_dir(b.path().join("NotAnApp")).unwrap();
        let apps = scan_mac(&[a.path().into(), b.path().into()]);
        assert_eq!(apps.len(), 2);
        assert_eq!(
            apps.iter().find(|x| x.name == "Safari").unwrap().path,
            a.path().join("Safari.app")
        );
    }
}
