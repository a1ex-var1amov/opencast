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
        "search" => {
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            serde_json::json!({ "commands": engine.search(q, SEARCH_LIMIT) })
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
    assert!(commands.iter().any(|c| c["id"] == "app:/Applications/Safari.app" && c["subtitle"].is_null()));
    let bad = dispatch(&engine, r#"{"op":"nope"}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&bad).unwrap()["error"].is_string());
}
