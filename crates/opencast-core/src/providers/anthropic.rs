//! Anthropic Messages API (`/v1/messages`, streaming).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{pump, sse::SseEvent, ChatEvent, ChatRequest, Endpoint, Provider, Role, Step, ToolAccumulator};
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
    // Content blocks per turn. Tool results travel as `user` turns; consecutive
    // same-role turns are merged because the API wants strict alternation.
    let mut turns: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for m in req.messages.iter().filter(|m| m.role != Role::System) {
        let (role, blocks): (&'static str, Vec<Value>) = match m.role {
            Role::Assistant => {
                let mut b = Vec::new();
                if !m.content.is_empty() {
                    b.push(json!({"type": "text", "text": m.content}));
                }
                for c in &m.tool_calls {
                    b.push(json!({"type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments}));
                }
                ("assistant", b)
            }
            Role::Tool => (
                "user",
                vec![json!({
                    "type": "tool_result",
                    "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": m.content,
                })],
            ),
            _ => ("user", vec![json!({"type": "text", "text": m.content})]),
        };
        if blocks.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some((r, b)) if *r == role => b.extend(blocks),
            _ => turns.push((role, blocks)),
        }
    }
    let messages: Vec<Value> = turns
        .into_iter()
        .map(|(role, blocks)| {
            // Plain single-text turns stay strings (smaller, and what older proxies expect).
            let all_text = blocks.iter().all(|b| b["type"] == "text");
            if all_text {
                let text = blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                json!({"role": role, "content": text})
            } else {
                json!({"role": role, "content": blocks})
            }
        })
        .collect();
    let mut b = json!({
        "model": req.model,
        "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "messages": messages,
    });
    if !system.is_empty() {
        b["system"] = json!(system.join("\n\n"));
    }
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    if !req.tools.is_empty() {
        b["tools"] = json!(req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
            .collect::<Vec<_>>());
    }
    b
}

/// Stateful parser: `tool_use` blocks arrive as a start event plus
/// `input_json_delta` fragments, keyed by content-block index.
#[derive(Default)]
pub struct Parser {
    tools: ToolAccumulator,
    /// content-block index → tool slot
    slots: std::collections::HashMap<u64, usize>,
}

impl Parser {
    pub fn parse(&mut self, ev: &SseEvent) -> Step {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
            return Step::Continue(None);
        };
        match v["type"].as_str().or(ev.event.as_deref()) {
            Some("content_block_start") if v["content_block"]["type"] == "tool_use" => {
                let slot = self.slots.len();
                self.slots.insert(v["index"].as_u64().unwrap_or(0), slot);
                let block = &v["content_block"];
                self.tools
                    .update(slot, block["id"].as_str(), block["name"].as_str(), None);
                Step::Continue(None)
            }
            Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
                Step::Continue(v["delta"]["text"].as_str().map(str::to_string))
            }
            Some("content_block_delta") if v["delta"]["type"] == "input_json_delta" => {
                if let Some(&slot) = self.slots.get(&v["index"].as_u64().unwrap_or(0)) {
                    self.tools.update(slot, None, None, v["delta"]["partial_json"].as_str());
                }
                Step::Continue(None)
            }
            Some("message_stop") => {
                if self.tools.is_empty() {
                    Step::Done
                } else {
                    Step::ToolCalls(self.tools.take())
                }
            }
            Some("error") => Step::Error(
                v["error"]["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string()),
            ),
            _ => Step::Continue(None),
        }
    }
}

pub fn parse(ev: &SseEvent) -> Step {
    Parser::default().parse(ev)
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
        let mut parser = Parser::default();
        pump(resp, &tx, |ev| parser.parse(ev)).await
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
    fn tool_turns_become_blocks() {
        use crate::providers::{ToolCall, ToolSpec};
        let call = ToolCall {
            id: "tu1".into(),
            name: "fs__read".into(),
            arguments: json!({"path": "/x"}),
        };
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![
                Message::new(Role::User, "read /x"),
                Message::assistant_tool_calls("Reading.", vec![call.clone()]),
                Message::tool_result(&call, "contents"),
            ],
            tools: vec![ToolSpec {
                name: "fs__read".into(),
                description: "read".into(),
                parameters: json!({"type": "object"}),
            }],
            ..Default::default()
        };
        let b = body(&req);
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(b["messages"][0]["content"], "read /x");
        assert_eq!(b["messages"][1]["content"][0]["text"], "Reading.");
        assert_eq!(b["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(b["messages"][2]["role"], "user");
        assert_eq!(b["messages"][2]["content"][0]["tool_use_id"], "tu1");
    }

    #[test]
    fn parse_streamed_tool_use() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        let mut p = Parser::default();
        for f in [
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu9","name":"fs__read","input":{}}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"/x\"}"}}"#,
        ] {
            assert_eq!(p.parse(&ev(f)), Step::Continue(None));
        }
        match p.parse(&ev(r#"{"type":"message_stop"}"#)) {
            Step::ToolCalls(c) => {
                assert_eq!(c[0].id, "tu9");
                assert_eq!(c[0].arguments, json!({"path": "/x"}));
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
