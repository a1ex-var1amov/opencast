//! OpenAI-compatible `/chat/completions` (OpenAI, Ollama, LM Studio,
//! OpenRouter, Groq, Together, vLLM, LiteLLM, Azure-style gateways...).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{pump, sse::SseEvent, ChatEvent, ChatRequest, Endpoint, Provider, Step};
use crate::config::ProviderKind;

pub struct OpenAi(pub Endpoint);

pub fn url(base: &str) -> String {
    format!("{base}/chat/completions")
}

pub fn body(req: &ChatRequest) -> Value {
    let mut b = json!({
        "model": req.model,
        "messages": req.messages.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        "stream": true,
    });
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    if let Some(m) = req.max_tokens {
        b["max_tokens"] = json!(m);
    }
    b
}

pub fn parse(ev: &SseEvent) -> Step {
    let data = ev.data.trim();
    if data == "[DONE]" {
        return Step::Done;
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
    let delta = &v["choices"][0]["delta"];
    // Reasoning models on some gateways stream thoughts separately; we only
    // show the answer.
    Step::Continue(delta["content"].as_str().map(str::to_string))
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

    async fn stream(&self, req: ChatRequest, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        let req = self.0.apply_defaults(req);
        let mut rb = self.0.post(&url(&self.0.base_url)).json(&body(&req));
        if let Some(k) = &self.0.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await?;
        pump(resp, &tx, parse).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Message, Role};

    #[test]
    fn body_shape() {
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![Message::new(Role::System, "s"), Message::new(Role::User, "hi")],
            temperature: Some(0.2),
            max_tokens: None,
        };
        let b = body(&req);
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["stream"], true);
        assert!(b.get("max_tokens").is_none());
        assert!((b["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
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
}
