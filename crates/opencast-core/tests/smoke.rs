//! Smoke test: a fresh install (example config, example scripts) with a few
//! hundred fake apps. Every command must plan, every previewable row must
//! preview, and search must stay fast. Runs anywhere (`cargo test`), and on
//! the Mac via `./smoke.command`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use opencast_core::apps::AppEntry;
use opencast_core::engine::Engine;

fn fake_apps(n: usize) -> Vec<AppEntry> {
    let words = [
        "Visual", "Studio", "Code", "Slack", "Safari", "Notes", "Music", "Terminal", "Finder", "Mail", "Calendar",
        "Photos", "Preview", "Xcode", "Docker", "Figma", "Zoom", "Arc", "Obsidian", "Spotify",
    ];
    (0..n)
        .map(|i| {
            let name = format!("{} {} {i}", words[i % words.len()], words[(i * 7 + 3) % words.len()]);
            AppEntry {
                path: PathBuf::from(format!("/Applications/{name}.app")),
                name,
            }
        })
        .chain(
            ["Visual Studio Code", "System Settings", "Safari"].map(|name| AppEntry {
                path: PathBuf::from(format!("/Applications/{name}.app")),
                name: name.to_string(),
            }),
        )
        .collect()
}

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let e = Engine::load_with_apps(dir.path(), fake_apps(400));
    let took = started.elapsed();
    assert!(took < Duration::from_secs(2), "engine load took {took:?}");
    assert!(e.warnings.is_empty(), "fresh install has warnings: {:?}", e.warnings);
    (dir, e)
}

#[test]
fn every_command_plans() {
    let (_dir, e) = engine();
    assert!(e.registry.len() > 50, "only {} commands", e.registry.len());
    let mut failures = Vec::new();
    for c in e.registry.iter() {
        // Fill every argument so commands that need input can plan.
        let args: Vec<String> = c.arguments.iter().map(|_| "smoke".to_string()).collect();
        if let Err(err) = e.plan(&c.id, &args) {
            failures.push(format!("{} ({}): {err:#}", c.id, c.title));
        }
    }
    assert!(
        failures.is_empty(),
        "commands that do not plan:\n{}",
        failures.join("\n")
    );
}

#[test]
fn search_is_fast_and_sane() {
    let (_dir, e) = engine();
    // Warm up once.
    e.search("a", 30);
    let queries = [
        "",
        "s",
        "sl",
        "sla",
        "slack",
        "vsc",
        "visual studio",
        "term",
        "clip",
        "note",
        "notes",
        "chats",
        "emoji smile",
        "2+2*3",
        "quit",
        "left half",
        "move left",
        "theme",
        "settings",
        "zzzzzz",
        "a b c d e f",
    ];
    let mut worst = (Duration::ZERO, "");
    for q in queries {
        let t = Instant::now();
        let rows = e.search(q, 30);
        let took = t.elapsed();
        if took > worst.0 {
            worst = (took, q);
        }
        assert!(rows.len() <= 60, "`{q}` returned {} rows", rows.len());
        let mut ids: Vec<&str> = rows.iter().map(|c| c.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), rows.len(), "`{q}` returned duplicate rows");
    }
    // Debug builds are ~10x slower than release; this still catches O(n²) mistakes.
    assert!(
        worst.0 < Duration::from_millis(150),
        "search `{}` took {:?}",
        worst.1,
        worst.0
    );
    assert_eq!(e.search("2+2*3", 5)[0].title, "8");
    assert!(e.search("vsc", 5).iter().any(|c| c.title.starts_with("Visual Studio")));
}

#[test]
fn previews_never_panic() {
    let (_dir, e) = engine();
    let note = e.add_note("Smoke", "body");
    let mut ids: Vec<String> = e.registry.iter().map(|c| c.id.clone()).collect();
    ids.push(note.id);
    ids.push("nope:missing".into());
    for id in ids {
        let _ = e.preview(&id);
    }
}

#[test]
fn config_writes_stay_small() {
    let dir = tempfile::tempdir().unwrap();
    let _ = Engine::load_with_apps(dir.path(), vec![]);
    let path = dir.path().join("config.toml");
    let before = std::fs::metadata(&path).unwrap().len();
    for i in 0..30 {
        let theme = ["dark", "light", "system"][i % 3];
        opencast_core::config::set_general_key(dir.path(), "theme", theme).unwrap();
        opencast_core::config::set_general_key(dir.path(), "presentation", "spotlight").unwrap();
    }
    let after = std::fs::metadata(&path).unwrap().len();
    assert!(after < before + 200, "config grew from {before} to {after} bytes");
    let e = Engine::load_with_apps(dir.path(), vec![]);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    assert_eq!(e.config.general.presentation, "spotlight");
}
