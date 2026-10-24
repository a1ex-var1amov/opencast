//! Spotlight file search via `mdfind`.
//!
//! Called from `searchFiles` after the Swift UI has painted in-menu hits.
//! Soft expand is debounced; hard prefixes (`file …`) still use this path.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Absolute paths matching `query`. Name search by default; content when
/// `query` starts with `content:` / `in:`.
pub fn search(query: &str, limit: usize) -> Vec<PathBuf> {
    let q = query.trim();
    if q.len() < 2 || limit == 0 {
        return Vec::new();
    }
    let (content, needle) = if let Some(rest) = q.strip_prefix("content:") {
        (true, rest.trim())
    } else if let Some(rest) = q.strip_prefix("in:") {
        (true, rest.trim())
    } else {
        (false, q)
    };
    if needle.len() < 2 {
        return Vec::new();
    }

    if content {
        content_search(needle, limit)
    } else {
        name_search(needle, limit)
    }
}

fn name_search(q: &str, limit: usize) -> Vec<PathBuf> {
    let mut cmd = Command::new("mdfind");
    cmd.args(["-name", q]);
    if let Some(home) = std::env::var_os("HOME") {
        cmd.arg("-onlyin").arg(home);
    }
    run_mdfind(cmd, limit)
}

fn content_search(q: &str, limit: usize) -> Vec<PathBuf> {
    let safe = q.replace('\\', "\\\\").replace('"', "\\\"");
    let pred = format!("kMDItemTextContent == \"*{safe}*\"cd");
    let mut cmd = Command::new("mdfind");
    cmd.arg(&pred);
    if let Some(home) = std::env::var_os("HOME") {
        cmd.arg("-onlyin").arg(home);
    }
    run_mdfind(cmd, limit)
}

/// Cap wall time so Spotlight cannot stall launcher search.
fn run_mdfind(mut cmd: Command, limit: usize) -> Vec<PathBuf> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Vec::new();
    };
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let deadline = Duration::from_millis(400);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let buf = reader.join().unwrap_or_default();
                if status.success() {
                    return filter_paths(&buf, limit);
                }
                return Vec::new();
            }
            Ok(None) if start.elapsed() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Vec::new();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => {
                let _ = child.kill();
                let _ = reader.join();
                return Vec::new();
            }
        }
    }
}

fn filter_paths(stdout: &[u8], limit: usize) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    // Cap before metadata(): mdfind can return hundreds of hits; we only keep `limit`.
    let cap = limit.saturating_mul(4).max(limit);
    let mut rows: Vec<(bool, PathBuf)> = String::from_utf8_lossy(stdout)
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .filter(|p| keep_spotlight_hit(p))
        .filter(|p| {
            home.as_ref()
                .is_none_or(|h| p.starts_with(h) || p.starts_with("/Applications") || p.starts_with("/Users"))
        })
        .take(cap)
        .map(|p| {
            let is_dir = std::fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
            (is_dir, p)
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter().take(limit).map(|(_, p)| p).collect()
}

/// Drop `.app` bundles; keep normal files and folders (including extensionless dirs).
fn keep_spotlight_hit(p: &std::path::Path) -> bool {
    if p.extension().and_then(|e| e.to_str()) != Some("app") {
        return true;
    }
    !std::fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn filter_keeps_directories_and_prefers_them() {
        // Workspace/temp only — writing under $HOME fails in sandboxed CI.
        let base = std::env::temp_dir().join(format!("opencast_files_mac_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let folder = base.join("SpotlightFolder");
        let file = base.join("notes.txt");
        let app = base.join("Fake.app");
        fs::create_dir(&folder).unwrap();
        fs::write(&file, "x").unwrap();
        fs::create_dir(&app).unwrap();
        assert!(keep_spotlight_hit(&folder), "normal folder kept");
        assert!(keep_spotlight_hit(&file), "file kept");
        assert!(!keep_spotlight_hit(&app), ".app bundle dropped");
        let mut rows = vec![
            (fs::metadata(&file).unwrap().is_dir(), file.clone()),
            (fs::metadata(&folder).unwrap().is_dir(), folder.clone()),
        ];
        rows.sort_by(|a, b| b.0.cmp(&a.0));
        assert!(rows[0].1.ends_with("SpotlightFolder"), "folders sort first");
        let _ = fs::remove_dir_all(&base);
    }
}
