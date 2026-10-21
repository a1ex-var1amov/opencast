use opencast_core::engine::Engine;
use opencast_core::Config;
#[cfg(test)]
use opencast_core::apps::AppEntry;
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
            serde_json::json!({ "commands": engine.search(q, SEARCH_LIMIT) })
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
            // Prefer the file so Appearance / Presentation commands stick without Reload.
            let path = engine.config_dir.join("config.toml");
            let (theme, presentation) = Config::load_from(&path)
                .map(|c| (c.general.theme, c.general.presentation))
                .unwrap_or_else(|_| {
                    (
                        engine.config.general.theme.clone(),
                        engine.config.general.presentation.clone(),
                    )
                });
            serde_json::json!({ "theme": theme, "presentation": presentation })
        }
        "warnings" => serde_json::json!({ "warnings": engine.warnings }),
        "providers" => serde_json::json!({ "providers": engine.router.list() }),
        _ => serde_json::json!({ "error": format!("unknown op `{op}`") }),
    };
    out.to_string()
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
    assert!(
        commands
            .iter()
            .any(|c| c["id"] == "app:/Applications/Safari.app" && c["subtitle"].is_null())
    );
    let bad = dispatch(&engine, r#"{"op":"nope"}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&bad).unwrap()["error"].is_string());
}

#[test]
fn file_needle_strips_prefix_keeps_filenames() {
    assert_eq!(file_needle("file notes").as_deref(), Some("notes"));
    assert_eq!(file_needle("find.pdf").as_deref(), Some("find.pdf"));
    assert_eq!(file_needle("file"), None);
    assert_eq!(file_needle("content:todo").as_deref(), Some("content:todo"));
}
