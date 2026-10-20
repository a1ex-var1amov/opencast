//! macOS app discovery via Spotlight. Falls back to the directory scan.

use opencast_core::apps::{self, AppEntry, MacAppHint};
use std::path::PathBuf;
use std::process::Command;

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
    let mut hints = Vec::new();
    for line in text.lines() {
        let path = PathBuf::from(line.trim());
        if path.as_os_str().is_empty() {
            continue;
        }
        let plist = path.join("Contents/Info.plist");
        let src = std::fs::read_to_string(&plist).unwrap_or_default();
        let (display_name, bundle_name, background_only) = apps::parse_info_plist(&src);
        hints.push(MacAppHint {
            path,
            display_name,
            bundle_name,
            background_only,
        });
    }
    Ok(hints)
}
