//! Google Gemini API (`streamGenerateContent` with `alt=sse`).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{pump, sse::SseEvent, ChatEvent, ChatRequest, Endpoint, Provider, Role, Step};
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
        .map(|m| {
            let role = if m.role == Role::Assistant { "model" } else { "user" };
            json!({"role": role, "parts": [{"text": m.content}]})
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
    b
}

pub fn parse(ev: &SseEvent) -> Step {
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
    let text: String = v["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                // Skip "thought" parts from thinking models.
                .filter(|p| p["thought"] != true)
                .filter_map(|p| p["text"].as_str())
                .collect()
        })
        .unwrap_or_default();
    Step::Continue(Some(text))
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
        pump(resp, &tx, parse).await
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
