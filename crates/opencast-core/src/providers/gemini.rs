//! Google Gemini API (`streamGenerateContent` with `alt=sse`).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{pump, sse::SseEvent, ChatEvent, ChatRequest, Endpoint, Provider, Role, Step, ToolCall};
use crate::config::ProviderKind;

pub struct Gemini(pub Endpoint);

pub fn url(base: &str, model: &str) -> String {
    let model = model.strip_prefix("models/").unwrap_or(model);
    format!("{base}/v1beta/models/{model}:streamGenerateContent?alt=sse")
}

pub fn body(req: &ChatRequest) -> Value {
    let system: Vec<&str> = req
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    let contents: Vec<Value> = req
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| match m.role {
            Role::Assistant if !m.tool_calls.is_empty() => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({"text": m.content}));
                }
                for c in &m.tool_calls {
                    parts.push(json!({"functionCall": {"name": c.name, "args": c.arguments}}));
                }
                json!({"role": "model", "parts": parts})
            }
            Role::Tool => json!({
                "role": "user",
                "parts": [{"functionResponse": {
                    "name": m.name.clone().unwrap_or_default(),
                    "response": {"content": m.content},
                }}],
            }),
            _ => {
                let role = if m.role == Role::Assistant { "model" } else { "user" };
                json!({"role": role, "parts": [{"text": m.content}]})
            }
        })
        .collect();
    let mut b = json!({ "contents": contents });
    if !system.is_empty() {
        b["systemInstruction"] = json!({"parts": [{"text": system.join("\n\n")}]});
    }
    let mut generation = serde_json::Map::new();
    if let Some(t) = req.temperature {
        generation.insert("temperature".into(), json!(t));
    }
    if let Some(m) = req.max_tokens {
        generation.insert("maxOutputTokens".into(), json!(m));
    }
    if !generation.is_empty() {
        b["generationConfig"] = Value::Object(generation);
    }
    if !req.tools.is_empty() {
        b["tools"] = json!([{ "functionDeclarations": req.tools.iter().map(|t| json!({
            "name": t.name,
            "description": t.description,
            "parameters": gemini_schema(&t.parameters),
        })).collect::<Vec<_>>() }]);
    }
    b
}

/// Gemini accepts an OpenAPI subset of JSON Schema: drop the keywords it rejects.
pub fn gemini_schema(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                if matches!(
                    k.as_str(),
                    "$schema" | "additionalProperties" | "$id" | "$ref" | "default" | "examples"
                ) {
                    continue;
                }
                out.insert(k.clone(), gemini_schema(val));
            }
            // An object with no properties is rejected; give it an empty map.
            if out.get("type").and_then(Value::as_str) == Some("object") && !out.contains_key("properties") {
                out.insert("properties".into(), json!({}));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(gemini_schema).collect()),
        other => other.clone(),
    }
}

/// Gemini sends function calls whole (no fragments); collect them and release
/// when the candidate finishes.
#[derive(Default)]
pub struct Parser {
    calls: Vec<ToolCall>,
}

impl Parser {
    pub fn parse(&mut self, ev: &SseEvent) -> Step {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
            return Step::Continue(None);
        };
        if let Some(e) = v.get("error") {
            return Step::Error(
                e["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| e.to_string()),
            );
        }
        if let Some(reason) = v["promptFeedback"]["blockReason"].as_str() {
            return Step::Error(format!("blocked by Gemini safety filter: {reason}"));
        }
        let parts = v["candidates"][0]["content"]["parts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut text = String::new();
        for p in &parts {
            if let Some(fc) = p.get("functionCall") {
                let n = self.calls.len();
                self.calls.push(ToolCall {
                    id: format!("gemini_{n}"),
                    name: fc["name"].as_str().unwrap_or_default().to_string(),
                    arguments: if fc["args"].is_object() {
                        fc["args"].clone()
                    } else {
                        json!({})
                    },
                });
            } else if p["thought"] != true {
                // Skip "thought" parts from thinking models.
                if let Some(t) = p["text"].as_str() {
                    text.push_str(t);
                }
            }
        }
        if !self.calls.is_empty() && v["candidates"][0]["finishReason"].is_string() {
            return Step::ToolCalls(std::mem::take(&mut self.calls));
        }
        Step::Continue(Some(text))
    }
}

pub fn parse(ev: &SseEvent) -> Step {
    Parser::default().parse(ev)
}

#[async_trait]
impl Provider for Gemini {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }
    fn default_model(&self) -> &str {
        &self.0.model
    }
    fn models(&self) -> Vec<String> {
        self.0.models.clone()
    }

    async fn stream(&self, req: ChatRequest, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        let req = self.0.apply_defaults(req);
        let mut rb = self.0.post(&url(&self.0.base_url, &req.model)).json(&body(&req));
        if let Some(k) = &self.0.api_key {
            rb = rb.header("x-goog-api-key", k);
        }
        let resp = rb.send().await?;
        let mut parser = Parser::default();
        pump(resp, &tx, |ev| parser.parse(ev)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Message;

    #[test]
    fn url_and_body() {
        assert_eq!(
            url("https://g", "models/gemini-x"),
            "https://g/v1beta/models/gemini-x:streamGenerateContent?alt=sse"
        );
        let req = ChatRequest {
            model: "g".into(),
            messages: vec![
                Message::new(Role::System, "sys"),
                Message::new(Role::User, "q"),
                Message::new(Role::Assistant, "a"),
            ],
            max_tokens: Some(100),
            ..Default::default()
        };
        let b = body(&req);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 100);
    }

    #[test]
    fn tools_and_function_turns() {
        use crate::providers::ToolSpec;
        let call = ToolCall {
            id: "gemini_0".into(),
            name: "w__now".into(),
            arguments: json!({}),
        };
        let req = ChatRequest {
            model: "g".into(),
            messages: vec![
                Message::new(Role::User, "time?"),
                Message::assistant_tool_calls("", vec![call.clone()]),
                Message::tool_result(&call, "10:00"),
            ],
            tools: vec![ToolSpec {
                name: "w__now".into(),
                description: "now".into(),
                parameters: json!({"type": "object", "additionalProperties": false, "$schema": "x"}),
            }],
            ..Default::default()
        };
        let b = body(&req);
        let decl = &b["tools"][0]["functionDeclarations"][0];
        assert_eq!(decl["name"], "w__now");
        assert!(decl["parameters"].get("additionalProperties").is_none());
        assert_eq!(decl["parameters"]["properties"], json!({}));
        assert_eq!(b["contents"][1]["parts"][0]["functionCall"]["name"], "w__now");
        assert_eq!(
            b["contents"][2]["parts"][0]["functionResponse"]["response"]["content"],
            "10:00"
        );
    }

    #[test]
    fn parse_function_call() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        let mut p = Parser::default();
        match p.parse(&ev(
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"w__now","args":{"tz":"EST"}}}]},"finishReason":"STOP"}]}"#,
        )) {
            Step::ToolCalls(c) => {
                assert_eq!(c[0].name, "w__now");
                assert_eq!(c[0].arguments["tz"], "EST");
            }
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn parse_events() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        assert_eq!(
            parse(&ev(
                r#"{"candidates":[{"content":{"parts":[{"text":"thinking","thought":true},{"text":"Hi"},{"text":"!"}]}}]}"#
            )),
            Step::Continue(Some("Hi!".into()))
        );
        assert_eq!(
            parse(&ev(r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#)),
            Step::Error("blocked by Gemini safety filter: SAFETY".into())
        );
    }
}
