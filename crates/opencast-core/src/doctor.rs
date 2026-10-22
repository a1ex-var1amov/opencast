//! "OpenCast Doctor": a health report for a real config folder. Catches the
//! things tests on a fresh install cannot: a runaway file, repeated text,
//! scripts that will not run, providers without keys, slow search with the
//! user's own apps and history.
//!
//! Run it from the launcher (OpenCast Doctor) or a terminal:
//! `cargo run --release -p opencast-core --example doctor`.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::engine::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Error,
}

impl Level {
    fn mark(self) -> &'static str {
        match self {
            Level::Ok => "✓",
            Level::Warn => "⚠︎",
            Level::Error => "✗",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub search_p50: Duration,
    pub search_p95: Duration,
    pub search_worst: (Duration, String),
}

impl Report {
    fn push(&mut self, level: Level, text: impl Into<String>) {
        self.findings.push(Finding {
            level,
            text: text.into(),
        });
    }

    pub fn worst(&self) -> Level {
        self.findings.iter().map(|f| f.level).max().unwrap_or(Level::Ok)
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let (e, w) = (
            self.findings.iter().filter(|f| f.level == Level::Error).count(),
            self.findings.iter().filter(|f| f.level == Level::Warn).count(),
        );
        out.push_str(&match (e, w) {
            (0, 0) => "All good.\n\n".to_string(),
            _ => format!("{e} problem(s), {w} warning(s).\n\n"),
        });
        // Problems first.
        let mut sorted: Vec<&Finding> = self.findings.iter().collect();
        sorted.sort_by_key(|f| std::cmp::Reverse(f.level));
        for f in sorted {
            out.push_str(&format!("{} {}\n", f.level.mark(), f.text));
        }
        out
    }
}

/// Sidecar files and the size above which something is wrong.
const FILES: &[(&str, u64, &str)] = &[
    ("config.toml", 256 * 1024, "a config is a few KB"),
    ("usage.json", 4 * 1024 * 1024, "usage history"),
    ("notes.json", 20 * 1024 * 1024, "notes"),
    ("chats.json", 50 * 1024 * 1024, "saved chats (Delete old ones with ⌘K)"),
    ("clipboard.json", 50 * 1024 * 1024, "clipboard history"),
    ("pinned.json", 1024 * 1024, "pins"),
    ("layouts.json", 4 * 1024 * 1024, "window layouts"),
    ("mcp-tools.json", 10 * 1024 * 1024, "MCP tool cache"),
];

fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} B"),
    }
}

/// The most repeated non-blank line and its count (repeated text = a writer bug).
pub fn most_repeated_line(text: &str) -> Option<(String, usize)> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for line in text.lines().map(str::trim).filter(|l| l.len() > 8) {
        *counts.entry(line).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(l, n)| (l.to_string(), n))
}

/// Check the files in `dir` without loading the engine (safe on huge files).
pub fn check_files(dir: &Path, report: &mut Report) {
    for (name, limit, what) in FILES {
        let path = dir.join(name);
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let size = meta.len();
        if size > crate::config::MAX_CONFIG_BYTES && *name == "config.toml" {
            report.push(
                Level::Error,
                format!(
                    "{name} is {} — OpenCast will not read it. It probably has repeated text; \
                     keep one copy of each section.",
                    human(size)
                ),
            );
            continue;
        }
        if size > *limit {
            report.push(
                Level::Warn,
                format!("{name} is {} — unusually large for {what}", human(size)),
            );
        }
        if name.ends_with(".toml") && size <= crate::config::MAX_CONFIG_BYTES {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some((line, n)) = most_repeated_line(&text) {
                    if n >= 20 {
                        report.push(Level::Error, format!("{name} repeats the line “{line}” {n} times"));
                    }
                }
            }
        }
    }
    if crate::sync::is_enabled(dir) {
        report.push(Level::Ok, "config syncs through a linked folder (iCloud Drive)");
    }
}

/// Full report: files, engine warnings, scripts, providers, search speed.
pub fn run(engine: &Engine, load_time: Option<Duration>) -> Report {
    let mut report = Report::default();
    check_files(&engine.config_dir, &mut report);

    if let Some(t) = load_time {
        let level = if t > Duration::from_secs(2) {
            Level::Warn
        } else {
            Level::Ok
        };
        report.push(level, format!("engine loads in {} ms", t.as_millis()));
    }
    for w in &engine.warnings {
        report.push(Level::Warn, w.clone());
    }
    report.push(
        Level::Ok,
        format!("{} commands, {} apps", engine.registry.len(), engine.app_paths().len()),
    );

    // Providers: a default that exists, and a key for each remote one.
    let providers = engine.router.list();
    if providers.is_empty() {
        report.push(
            Level::Warn,
            "no AI providers configured — AI Chat and AI commands will not work",
        );
    }
    for p in &engine.config.providers {
        let local = p
            .base_url
            .as_deref()
            .is_some_and(|u| u.contains("localhost") || u.contains("127.0.0.1"));
        if !local && p.api_key_env.is_none() && p.api_key_cmd.is_none() {
            report.push(
                Level::Warn,
                format!("provider `{}` has no api_key_env or api_key_cmd", p.id),
            );
        }
        if let Some(env) = &p.api_key_env {
            if std::env::var(env).map(|v| v.is_empty()).unwrap_or(true) {
                report.push(
                    Level::Warn,
                    format!(
                        "provider `{}`: ${env} is not set for apps — use api_key_cmd with the Keychain (Settings → AI)",
                        p.id
                    ),
                );
            }
        }
    }

    // Extensions.
    let exts = crate::extensions::list(&engine.scripts_dir());
    if !exts.is_empty() {
        report.push(Level::Ok, format!("{} extension(s) installed", exts.len()));
    }

    // MCP servers without a cached tool list never connected.
    let cache = crate::mcp::load_cache(&engine.config_dir);
    for s in engine.mcp_servers() {
        if !cache.contains_key(&s.id) {
            report.push(
                Level::Warn,
                format!(
                    "MCP server `{}` has no tools yet — run Refresh MCP Tools to see why",
                    s.id
                ),
            );
        }
    }

    // Search speed with this user's apps, history and notes.
    let queries = [
        "",
        "a",
        "s",
        "sa",
        "saf",
        "term",
        "vsc",
        "note",
        "chats",
        "clip",
        "emoji",
        "2+2",
        "left half",
        "settings",
        "zzzzzz",
        "open config",
        "quit",
    ];
    engine.search("a", 30); // warm
    let mut times: Vec<(Duration, &str)> = queries
        .iter()
        .map(|q| {
            let t = Instant::now();
            engine.search(q, 30);
            (t.elapsed(), *q)
        })
        .collect();
    times.sort();
    let pct = |p: f64| times[((times.len() - 1) as f64 * p).round() as usize].0;
    report.search_p50 = pct(0.5);
    report.search_p95 = pct(0.95);
    let worst = times.last().copied().unwrap_or_default();
    report.search_worst = (worst.0, worst.1.to_string());
    let level = if report.search_p95 > Duration::from_millis(30) {
        Level::Warn
    } else {
        Level::Ok
    };
    report.push(
        level,
        format!(
            "search p50 {:.1} ms · p95 {:.1} ms · slowest “{}” {:.1} ms",
            report.search_p50.as_secs_f64() * 1000.0,
            report.search_p95.as_secs_f64() * 1000.0,
            worst.1,
            worst.0.as_secs_f64() * 1000.0
        ),
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_runaway_and_repeated_config() {
        let dir = tempfile::tempdir().unwrap();
        let header = "# OpenCast configuration\n# Location: somewhere long enough\n\n";
        std::fs::write(
            dir.path().join("config.toml"),
            header.repeat(50) + "[general]\nhotkey = \"Alt+Space\"\n",
        )
        .unwrap();
        let mut r = Report::default();
        check_files(dir.path(), &mut r);
        assert_eq!(r.worst(), Level::Error, "{r:?}");
        assert!(r.to_text().contains("repeats the line"));
    }

    #[test]
    fn fresh_install_is_healthy() {
        let dir = tempfile::tempdir().unwrap();
        let e = Engine::load_with_apps(dir.path(), vec![]);
        let r = run(&e, Some(Duration::from_millis(5)));
        assert!(r.worst() < Level::Error, "{}", r.to_text());
        assert!(r.to_text().contains("search p50"));
    }
}
