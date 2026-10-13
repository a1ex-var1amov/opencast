//! Anthropic Messages API (`/v1/messages`, streaming).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{pump, sse::SseEvent, ChatEvent, ChatRequest, Endpoint, Provider, Role, Step};
use crate::config::ProviderKind;

pub const API_VERSION: &str = "2023-06-01";
/// The Messages API requires max_tokens; used when neither the request nor
/// the provider config sets one.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

pub struct Anthropic(pub Endpoint);

pub fn url(base: &str) -> String {
    format!("{base}/v1/messages")
}

pub fn body(req: &ChatRequest) -> Value {
    let system: Vec<&str> = req
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    // Merge consecutive same-role turns: the API wants strict alternation.
    let mut turns: Vec<(Role, String)> = Vec::new();
    for m in req.messages.iter().filter(|m| m.role != Role::System) {
        match turns.last_mut() {
            Some((r, c)) if *r == m.role => {
                c.push_str("\n\n");
                c.push_str(&m.content);
            }
            _ => turns.push((m.role, m.content.clone())),
        }
    }
    let mut b = json!({
        "model": req.model,
        "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "messages": turns.iter().map(|(r, c)| json!({"role": r, "content": c})).collect::<Vec<_>>(),
    });
    if !system.is_empty() {
        b["system"] = json!(system.join("\n\n"));
    }
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    b
}

pub fn parse(ev: &SseEvent) -> Step {
    let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
        return Step::Continue(None);
    };
    match v["type"].as_str().or(ev.event.as_deref()) {
        Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
            Step::Continue(v["delta"]["text"].as_str().map(str::to_string))
        }
        Some("message_stop") => Step::Done,
        Some("error") => Step::Error(
            v["error"]["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string()),
        ),
        _ => Step::Continue(None),
    }
}

#[async_trait]
impl Provider for Anthropic {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }
    fn default_model(&self) -> &str {
        &self.0.model
    }
    fn models(&self) -> Vec<String> {
        self.0.models.clone()
    }

    async fn stream(&self, req: ChatRequest, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        let req = self.0.apply_defaults(req);
        let mut rb = self
            .0
            .post(&url(&self.0.base_url))
            .header("anthropic-version", API_VERSION)
            .json(&body(&req));
        if let Some(k) = &self.0.api_key {
            rb = rb.header("x-api-key", k);
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
    fn system_is_lifted_and_turns_merged() {
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![
                Message::new(Role::System, "be brief"),
                Message::new(Role::User, "a"),
                Message::new(Role::User, "b"),
                Message::new(Role::Assistant, "c"),
            ],
            ..Default::default()
        };
        let b = body(&req);
        assert_eq!(b["system"], "be brief");
        assert_eq!(b["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(b["messages"].as_array().unwrap().len(), 2);
        assert_eq!(b["messages"][0]["content"], "a\n\nb");
    }

    #[test]
    fn parse_events() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        assert_eq!(
            parse(&ev(
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#
            )),
            Step::Continue(Some("Hi".into()))
        );
        assert_eq!(parse(&ev(r#"{"type":"ping"}"#)), Step::Continue(None));
        assert_eq!(parse(&ev(r#"{"type":"message_stop"}"#)), Step::Done);
        assert_eq!(
            parse(&ev(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
            )),
            Step::Error("Overloaded".into())
        );
    }
}
