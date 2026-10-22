//! OpenAI-compatible `/chat/completions` (OpenAI, Ollama, LM Studio,
//! OpenRouter, Groq, Together, vLLM, LiteLLM, Azure-style gateways...).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    pump, sse::SseEvent, usable_images, ChatEvent, ChatRequest, Endpoint, Provider, Role, Step, ToolAccumulator,
};
use crate::config::ProviderKind;

pub struct OpenAi(pub Endpoint);

pub fn url(base: &str) -> String {
    format!("{base}/chat/completions")
}

pub fn body(req: &ChatRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| match m.role {
            Role::Assistant if !m.tool_calls.is_empty() => json!({
                "role": "assistant",
                "content": if m.content.is_empty() { Value::Null } else { json!(m.content) },
                "tool_calls": m.tool_calls.iter().map(|c| json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.name, "arguments": c.arguments.to_string() },
                })).collect::<Vec<_>>(),
            }),
            Role::Tool => json!({
                "role": "tool",
                "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                "content": m.content,
            }),
            // A turn with images becomes content parts (a data URL per image).
            _ if m.has_images() => json!({
                "role": m.role,
                "content": std::iter::once(json!({ "type": "text", "text": m.content }))
                    .chain(usable_images(m).map(|i| json!({
                        "type": "image_url",
                        "image_url": { "url": format!("data:{};base64,{}", i.mime, i.b64) },
                    })))
                    .collect::<Vec<_>>(),
            }),
            _ => json!({"role": m.role, "content": m.content}),
        })
        .collect();
    let mut b = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
    });
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    if let Some(m) = req.max_tokens {
        b["max_tokens"] = json!(m);
    }
    if !req.tools.is_empty() {
        b["tools"] = json!(req
            .tools
            .iter()
            .map(|t| json!({
                "type": "function",
                "function": { "name": t.name, "description": t.description, "parameters": t.parameters },
            }))
            .collect::<Vec<_>>());
    }
    b
}

/// Stateful parser: text deltas stream straight through; tool-call fragments
/// are collected and released when the turn ends.
#[derive(Default)]
pub struct Parser {
    tools: ToolAccumulator,
}

impl Parser {
    pub fn parse(&mut self, ev: &SseEvent) -> Step {
        let data = ev.data.trim();
        if data == "[DONE]" {
            return self.finish();
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return Step::Continue(None);
        };
        if let Some(e) = v.get("error") {
            let msg = e
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| e.to_string());
            return Step::Error(msg);
        }
        let choice = &v["choices"][0];
        let delta = &choice["delta"];
        if let Some(calls) = delta["tool_calls"].as_array() {
            for (n, c) in calls.iter().enumerate() {
                let index = c["index"].as_u64().map(|i| i as usize).unwrap_or(n);
                self.tools.update(
                    index,
                    c["id"].as_str(),
                    c["function"]["name"].as_str(),
                    c["function"]["arguments"].as_str(),
                );
            }
        }
        // Some servers (Ollama, older gateways) never send [DONE] after tool calls.
        if choice["finish_reason"] == "tool_calls" && !self.tools.is_empty() {
            return Step::ToolCalls(self.tools.take());
        }
        // Reasoning models on some gateways stream thoughts separately; we only
        // show the answer.
        Step::Continue(delta["content"].as_str().map(str::to_string))
    }

    fn finish(&mut self) -> Step {
        if self.tools.is_empty() {
            Step::Done
        } else {
            Step::ToolCalls(self.tools.take())
        }
    }
}

/// Stateless parse for callers that never send tools.
pub fn parse(ev: &SseEvent) -> Step {
    Parser::default().parse(ev)
}

#[async_trait]
impl Provider for OpenAi {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Openai
    }
    fn default_model(&self) -> &str {
        &self.0.model
    }
    fn models(&self) -> Vec<String> {
        self.0.models.clone()
    }

    fn supports_vision(&self, model: &str) -> bool {
        self.0.vision_for(model)
    }

    async fn stream(&self, req: ChatRequest, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        let req = self.0.apply_defaults(req);
        let mut rb = self.0.post(&url(&self.0.base_url)).json(&body(&req));
        if let Some(k) = &self.0.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await?;
        let mut parser = Parser::default();
        pump(resp, &tx, |ev| parser.parse(ev)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Image, Message, Role, ToolCall, ToolSpec};

    #[test]
    fn body_shape() {
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![Message::new(Role::System, "s"), Message::new(Role::User, "hi")],
            temperature: Some(0.2),
            max_tokens: None,
            tools: vec![],
        };
        let b = body(&req);
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["stream"], true);
        assert!(b.get("max_tokens").is_none());
        assert!(b.get("tools").is_none());
        assert!((b["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn body_with_tools_and_tool_turns() {
        let call = ToolCall {
            id: "c1".into(),
            name: "gh__search".into(),
            arguments: json!({"q": "x"}),
        };
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![
                Message::new(Role::User, "find x"),
                Message::assistant_tool_calls("", vec![call.clone()]),
                Message::tool_result(&call, "found"),
            ],
            tools: vec![ToolSpec {
                name: "gh__search".into(),
                description: "search".into(),
                parameters: json!({"type": "object"}),
            }],
            ..Default::default()
        };
        let b = body(&req);
        assert_eq!(b["tools"][0]["function"]["name"], "gh__search");
        assert_eq!(
            b["messages"][1]["tool_calls"][0]["function"]["arguments"],
            r#"{"q":"x"}"#
        );
        assert!(b["messages"][1]["content"].is_null());
        assert_eq!(b["messages"][2]["role"], "tool");
        assert_eq!(b["messages"][2]["tool_call_id"], "c1");
    }

    #[test]
    fn parse_events() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        assert_eq!(
            parse(&ev(r#"{"choices":[{"delta":{"content":"Hel"}}]}"#)),
            Step::Continue(Some("Hel".into()))
        );
        assert_eq!(
            parse(&ev(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#)),
            Step::Continue(None)
        );
        assert_eq!(parse(&ev("[DONE]")), Step::Done);
        assert_eq!(parse(&ev(r#"{"error":{"message":"bad"}}"#)), Step::Error("bad".into()));
    }

    #[test]
    fn parse_streamed_tool_calls() {
        let ev = |d: &str| SseEvent {
            event: None,
            data: d.into(),
        };
        let mut p = Parser::default();
        let frames = [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"gh__search","arguments":""}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q\":"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"rust\"}"}}]}}]}"#,
        ];
        for f in frames {
            assert_eq!(p.parse(&ev(f)), Step::Continue(None));
        }
        match p.parse(&ev(r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#)) {
            Step::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call_a");
                assert_eq!(calls[0].name, "gh__search");
                assert_eq!(calls[0].arguments, json!({"q": "rust"}));
            }
            s => panic!("{s:?}"),
        }
        // Nothing left over for [DONE].
        assert_eq!(p.parse(&ev("[DONE]")), Step::Done);
    }

    #[test]
    fn images_travel_as_data_urls() {
        let req = ChatRequest {
            model: "gpt-4o".into(),
            messages: vec![
                Message::new(Role::User, "what is in this chart?").with_images(vec![Image {
                    mime: "image/png".into(),
                    b64: "aW1hZ2U=".into(),
                }]),
            ],
            ..Default::default()
        };
        let parts = body(&req)["messages"][0]["content"].as_array().unwrap().clone();
        assert_eq!(parts.len(), 2, "text part + image part");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,aW1hZ2U=");
        // Text-only turns stay plain strings: older proxies choke on content arrays.
        let plain = body(&ChatRequest {
            model: "m".into(),
            messages: vec![Message::new(Role::User, "hi")],
            ..Default::default()
        });
        assert!(plain["messages"][0]["content"].is_string());
    }
}
