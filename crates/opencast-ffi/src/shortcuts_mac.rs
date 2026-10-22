//! Shortcuts.app: list the user's shortcuts (cached in `shortcuts.json`, the
//! `shortcuts` tool takes ~0.5 s) and run them, with optional text input.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

fn cache_path(dir: &Path) -> std::path::PathBuf {
    dir.join("shortcuts.json")
}

pub fn list() -> Vec<String> {
    let Ok(out) = Command::new("/usr/bin/shortcuts").arg("list").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Rescan and write the cache. Returns true when the list changed.
pub fn refresh_cache(dir: &Path) -> bool {
    let fresh = list();
    let old: Vec<String> = std::fs::read_to_string(cache_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if fresh == old {
        return false;
    }
    if let Ok(s) = serde_json::to_string(&fresh) {
        let _ = std::fs::write(cache_path(dir), s);
    }
    true
}

/// Run a shortcut. Returns its text output (may be empty).
pub async fn run(name: &str, input: Option<&str>) -> Result<String> {
    let tmp = std::env::temp_dir();
    let stamp = format!("{}-{}", std::process::id(), opencast_core::usage::now());
    let out_path = tmp.join(format!("opencast-sc-out-{stamp}.txt"));
    let mut cmd = tokio::process::Command::new("/usr/bin/shortcuts");
    cmd.arg("run").arg(name).arg("--output-path").arg(&out_path);
    let in_path = tmp.join(format!("opencast-sc-in-{stamp}.txt"));
    if let Some(text) = input {
        std::fs::write(&in_path, text).context("writing shortcut input")?;
        cmd.arg("--input-path").arg(&in_path);
    }
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(120), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("`{name}` is still running after 2 minutes"))?
        .context("running shortcuts")?;
    let _ = std::fs::remove_file(&in_path);
    let result = std::fs::read(&out_path).ok();
    let _ = std::fs::remove_file(&out_path);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if err.is_empty() {
                format!("`{name}` failed")
            } else {
                err
            }
        );
    }
    Ok(result
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
        .trim()
        .to_string())
}
