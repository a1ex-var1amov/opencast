#[cfg(test)]
use opencast_core::apps::AppEntry;
use opencast_core::engine::Engine;
use opencast_core::Config;
#[cfg(test)]
use std::path::PathBuf;

const SEARCH_LIMIT: usize = 60;

pub fn dispatch(engine: &Engine, request: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
    };
    let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
    let out = match op {
        // Instant: registry / emoji / notes / thin quicklink fallbacks only.
        "search" => {
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            let mut rows =
                serde_json::to_value(engine.search(q, SEARCH_LIMIT)).unwrap_or_else(|_| serde_json::json!([]));
            attach_actions(engine, &mut rows);
            serde_json::json!({ "commands": rows })
        }
        // Soft / forced Spotlight — never on the hot path; Swift debounces.
        "searchFiles" => {
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            let Some(needle) = file_needle(q) else {
                return serde_json::json!({ "commands": [] }).to_string();
            };
            let paths = crate::files_mac::search(&needle, 12);
            let commands = opencast_core::files::to_commands(paths, 12);
            serde_json::json!({ "commands": commands })
        }
        "settings" => {
            // Prefer the file so Appearance / Panel Position stick without Reload.
            let path = engine.config_dir.join("config.toml");
            let (theme, presentation) = Config::load_from(&path)
                .map(|c| (c.general.theme, c.general.presentation))
                .unwrap_or_else(|_| {
                    (
                        engine.config.general.theme.clone(),
                        engine.config.general.presentation.clone(),
                    )
                });
            serde_json::json!({
                "theme": theme,
                "presentation": presentation,
                "themeFile": engine.theme,
                "keymap": engine.keymap,
                "fallbackFiles": engine.fallback_files(),
                "fallbackThreshold": engine.config.general.fallback_threshold,
                "calendar": engine.config.general.calendar,
                "checkUpdates": engine.config.general.check_updates,
                "focus": {
                    "apps": engine.config.focus.apps,
                    "minutes": engine.config.focus.minutes,
                    "domains": engine.config.focus.domains,
                    "menuBar": engine.config.focus.menu_bar,
                },
                "shiftQuickLook": engine.config.general.shift_quick_look,
                "windowHud": engine.config.general.window_hud,
                "dragSnap": engine.config.general.drag_snap,
                "windowCycle": engine.config.general.window_cycle,
                "windowPortrait": engine.config.general.window_portrait,
                "confirmDestructive": engine.config.general.confirm_destructive,
            })
        }
        "warnings" => serde_json::json!({ "warnings": engine.warnings }),
        "apps" => serde_json::json!({ "paths": engine.app_paths() }),
        "preview" => {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            serde_json::json!({ "preview": engine.preview(id) })
        }
        "snippets" => {
            let list: Vec<serde_json::Value> = engine
                .expanding_snippets()
                .into_iter()
                .map(|(k, t)| serde_json::json!({ "keyword": k, "text": t }))
                .collect();
            serde_json::json!({ "snippets": list })
        }
        "providers" => serde_json::json!({ "providers": engine.router.list() }),
        "windowTarget" => {
            use opencast_core::window::{self, Rect};
            let rect = |x: Option<&serde_json::Value>| x.and_then(|r| serde_json::from_value::<Rect>(r.clone()).ok());
            let action = v.get("action").and_then(|x| x.as_str()).unwrap_or("");
            let cycle = v.get("cycle").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let screens: Vec<Rect> = v
                .get("screens")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|r| rect(Some(r))).collect())
                .unwrap_or_default();
            match rect(v.get("current")) {
                Some(cur) => {
                    let opts = window::Opts::from_config(&engine.config);
                    match window::target(action, &cur, &screens, cycle, &opts, &engine.config.window_commands) {
                        Some(r) => {
                            serde_json::json!({ "rect": r, "cycles": opencast_core::engine::WindowAction::from_key(action).map(window::cycle_len).unwrap_or(1) })
                        }
                        None => serde_json::json!({ "none": true }),
                    }
                }
                None => serde_json::json!({ "error": "missing current frame" }),
            }
        }
        "windowGlyphs" => {
            let ids: Vec<String> = v
                .get("actions")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let portrait = v.get("portrait").and_then(|x| x.as_bool()).unwrap_or(false)
                && opencast_core::window::Opts::from_config(&engine.config).portrait;
            let map: serde_json::Map<String, serde_json::Value> = ids
                .into_iter()
                .filter_map(|id| {
                    opencast_core::window::glyph(&id, &engine.config.window_commands, portrait)
                        .map(|r| (id, serde_json::json!(r)))
                })
                .collect();
            serde_json::json!({ "glyphs": map })
        }
        "accepting" => {
            let kind = v.get("type").and_then(|x| x.as_str()).unwrap_or("text");
            serde_json::json!({ "commands": engine.accepting(kind) })
        }
        "rankActions" => {
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            let ids: Vec<String> = v
                .get("ids")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            serde_json::json!({ "ids": engine.rank_ids(q, &ids) })
        }
        "bumpUsage" => {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            engine.bump_usage(id, q);
            serde_json::json!({ "ok": true })
        }
        "listPath" => {
            // Path mode reads one directory; off the search hot path (own op).
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            let hidden = v.get("hidden").and_then(|x| x.as_bool()).unwrap_or(false);
            match opencast_core::files::list_path(q, hidden, 200) {
                Some(rows) => serde_json::json!({ "commands": rows }),
                None => serde_json::json!({ "commands": [], "notPath": true }),
            }
        }
        "aiCommands" => {
            let list: Vec<&opencast_core::Command> = engine
                .registry
                .iter()
                .filter(|c| c.kind == opencast_core::CommandKind::Ai)
                .collect();
            serde_json::json!({ "commands": list })
        }
        _ => serde_json::json!({ "error": format!("unknown op `{op}`") }),
    };
    out.to_string()
}

/// Attach an integration's ⌘-menu actions to the row it drives, so the Spotify
/// app row offers Play / Pause and Now Playing without a trip through search.
fn attach_actions(engine: &Engine, rows: &mut serde_json::Value) {
    let Some(list) = rows.as_array_mut() else {
        return;
    };
    for row in list.iter_mut() {
        let Some(id) = row.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        if let Some(actions) = engine.actions_for(id) {
            if let Ok(v) = serde_json::to_value(actions) {
                row["actions"] = v;
            }
        }
    }
}

/// Needle for Spotlight. `file notes` → `notes`; `find.pdf` stays intact.
fn file_needle(query: &str) -> Option<String> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let ql = q.to_lowercase();
    if ql.starts_with("content:") || ql.starts_with("in:") {
        return Some(q.to_string());
    }
    for prefix in ["files ", "file ", "find ", "documents ", "document ", "docs "] {
        if let Some(rest) = ql.strip_prefix(prefix) {
            let rest = rest.trim();
            return if rest.len() >= 2 {
                // Preserve original casing of the remainder.
                Some(q[prefix.len()..].trim().to_string())
            } else {
                None
            };
        }
    }
    if matches!(
        ql.as_str(),
        "file" | "files" | "find" | "doc" | "docs" | "document" | "documents"
    ) {
        return None;
    }
    if q.len() >= 2 {
        Some(q.to_string())
    } else {
        None
    }
}

#[cfg(test)]
fn fixture() -> Engine {
    Engine::assemble(
        Config::default(),
        PathBuf::from("/tmp/opencast-test"),
        vec![AppEntry {
            name: "Safari".into(),
            path: PathBuf::from("/Applications/Safari.app"),
        }],
        vec![],
        vec![],
        vec![],
    )
}

#[test]
fn search_op_returns_commands() {
    let engine = fixture();
    let out = dispatch(&engine, r#"{"op":"search","query":""}"#);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let commands = v["commands"].as_array().expect("commands array");
    assert!(commands.iter().any(|c| c["title"] == "Safari"));
    let settings = dispatch(&engine, r#"{"op":"settings"}"#);
    let s: serde_json::Value = serde_json::from_str(&settings).unwrap();
    assert!(s["keymap"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["name"] == "actions" && k["key"] == "k"));
    assert_eq!(s["fallbackFiles"], true);
    assert!(s["themeFile"].is_null());
    assert!(commands
        .iter()
        .any(|c| c["id"] == "app:/Applications/Safari.app" && c["subtitle"].is_null()));
    let bad = dispatch(&engine, r#"{"op":"nope"}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&bad).unwrap()["error"].is_string());
}

#[test]
fn an_integration_puts_its_verbs_on_the_app_row() {
    let engine = Engine::assemble(
        Config::default(),
        PathBuf::from("/tmp/opencast-test"),
        vec![AppEntry {
            name: "Spotify".into(),
            path: PathBuf::from("/Applications/Spotify.app"),
        }],
        vec![],
        vec![],
        vec![],
    );
    let out = dispatch(&engine, r#"{"op":"search","query":"spotify"}"#);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let rows = v["commands"].as_array().unwrap();
    let row = rows
        .iter()
        .find(|c| c["id"] == "app:/Applications/Spotify.app")
        .expect("no Spotify app row");
    let ids: Vec<&str> = row["actions"]
        .as_array()
        .expect("the app row has no actions")
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"spotify-play"), "{ids:?}");
    assert!(ids.contains(&"spotify-search"), "{ids:?}");
    // Only the app row carries them: the command rows themselves do not.
    let plain = rows.iter().find(|c| c["id"] == "builtin:spotify-play").unwrap();
    assert!(plain.get("actions").is_none());
}

#[test]
fn file_needle_strips_prefix_keeps_filenames() {
    assert_eq!(file_needle("file notes").as_deref(), Some("notes"));
    assert_eq!(file_needle("find.pdf").as_deref(), Some("find.pdf"));
    assert_eq!(file_needle("file"), None);
    assert_eq!(file_needle("content:todo").as_deref(), Some("content:todo"));
}
