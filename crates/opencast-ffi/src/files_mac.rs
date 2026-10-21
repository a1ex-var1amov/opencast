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
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .filter(|p| p.extension().and_then(|e| e.to_str()) != Some("app"))
        .filter(|p| {
            home.as_ref()
                .is_none_or(|h| p.starts_with(h) || p.starts_with("/Applications") || p.starts_with("/Users"))
        })
        .take(limit)
        .collect()
}
