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

/// Hint from a macOS scanner (Spotlight + Info.plist). Filtered by [`filter_mac_apps`].
#[derive(Debug, Clone)]
pub struct MacAppHint {
    pub path: PathBuf,
    pub display_name: Option<String>,
    pub bundle_name: Option<String>,
    pub background_only: bool,
}

/// True when an earlier path component is already a `.app` (helper inside another app).
pub fn is_nested_app(path: &Path) -> bool {
    let mut comps = path.components().peekable();
    while let Some(c) = comps.next() {
        let s = c.as_os_str().to_string_lossy();
        if s.ends_with(".app") && comps.peek().is_some() {
            return true;
        }
    }
    false
}

/// Keep launchable top-level apps. Dedupes by path. Sorted by name.
pub fn filter_mac_apps(hints: Vec<MacAppHint>) -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for h in hints {
        if h.background_only || is_nested_app(&h.path) {
            continue;
        }
        if h.path.extension().and_then(|x| x.to_str()) != Some("app") {
            continue;
        }
        let name = h
            .display_name
            .filter(|s| !s.is_empty())
            .or_else(|| h.bundle_name.filter(|s| !s.is_empty()))
            .or_else(|| h.path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .unwrap_or_default();
        if name.is_empty() || !seen.insert(h.path.clone()) {
            continue;
        }
        out.push(AppEntry { name, path: h.path });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// `CFBundleDisplayName`, `CFBundleName`, `LSBackgroundOnly` from an Info.plist XML body.
pub fn parse_info_plist(src: &str) -> (Option<String>, Option<String>, bool) {
    (
        plist_string(src, "CFBundleDisplayName"),
        plist_string(src, "CFBundleName"),
        plist_bool(src, "LSBackgroundOnly").unwrap_or(false),
    )
}

fn plist_string(src: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let i = src.find(&needle)?;
    let after = &src[i + needle.len()..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")? + start;
    let s = after[start..end].trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn plist_bool(src: &str, key: &str) -> Option<bool> {
    let needle = format!("<key>{key}</key>");
    let i = src.find(&needle)?;
    let after = src[i + needle.len()..].trim_start();
    if after.starts_with("<true") {
        Some(true)
    } else if after.starts_with("<false") {
        Some(false)
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

    #[test]
    fn nested_app_and_background_are_dropped() {
        assert!(is_nested_app(Path::new("/Applications/Foo.app/Contents/Helpers/Bar.app")));
        assert!(!is_nested_app(Path::new("/Applications/Foo.app")));
        let apps = filter_mac_apps(vec![
            MacAppHint {
                path: PathBuf::from("/Applications/Foo.app/Contents/Helpers/Bar.app"),
                display_name: Some("Bar".into()),
                bundle_name: None,
                background_only: false,
            },
            MacAppHint {
                path: PathBuf::from("/Applications/Agent.app"),
                display_name: Some("Agent".into()),
                bundle_name: None,
                background_only: true,
            },
            MacAppHint {
                path: PathBuf::from("/Applications/Notes.app"),
                display_name: None,
                bundle_name: Some("Notes".into()),
                background_only: false,
            },
            MacAppHint {
                path: PathBuf::from("/Applications/Mail.app"),
                display_name: Some("Mail".into()),
                bundle_name: Some("Ignored".into()),
                background_only: false,
            },
        ]);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Mail");
        assert_eq!(apps[1].name, "Notes");
    }

    #[test]
    fn info_plist_names_and_background() {
        let src = r#"<?xml version="1.0"?>
<dict>
  <key>CFBundleDisplayName</key>
  <string>Display</string>
  <key>CFBundleName</key>
  <string>Bundle</string>
  <key>LSBackgroundOnly</key>
  <true/>
</dict>"#;
        let (d, b, bg) = parse_info_plist(src);
        assert_eq!(d.as_deref(), Some("Display"));
        assert_eq!(b.as_deref(), Some("Bundle"));
        assert!(bg);
    }
}
