//! AI providers behind one streaming trait.
//!
//! Three wire adapters cover essentially every provider: OpenAI-compatible
//! (also Ollama, LM Studio, OpenRouter, Groq, vLLM, LiteLLM...), Anthropic
//! Messages, and Gemini.

pub mod anthropic;
pub mod gemini;
pub mod openai;
pub mod sse;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config::{Config, ProviderConfig, ProviderKind};
use sse::{SseEvent, SseParser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// What the UI receives. Provider-specific shapes never leave this module.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "text", rename_all = "lowercase")]
pub enum ChatEvent {
    Delta(String),
    Done,
    Error(String),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn default_model(&self) -> &str;
    fn models(&self) -> Vec<String>;
    /// Stream a completion into `tx`. Always ends with `Done` or `Error`
    /// unless the receiver is dropped.
    async fn stream(&self, req: ChatRequest, tx: mpsc::Sender<ChatEvent>) -> Result<()>;
}

/// Settings shared by every adapter.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub id: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub models: Vec<String>,
    pub headers: HashMap<String, String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub client: reqwest::Client,
}

impl Endpoint {
    pub fn from_config(cfg: &ProviderConfig, client: reqwest::Client) -> Result<Self> {
        let base_url = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| default_base_url(cfg.kind).to_string())
            .trim_end_matches('/')
            .to_string();
        Ok(Self {
            id: cfg.id.clone(),
            kind: cfg.kind,
            base_url,
            api_key: cfg.resolve_api_key()?,
            model: cfg.model.clone(),
            models: cfg.all_models(),
            headers: cfg.headers.clone(),
            max_tokens: cfg.max_tokens,
            temperature: cfg.temperature,
            client,
        })
    }

    /// Fill per-provider defaults the request did not set.
    pub fn apply_defaults(&self, mut req: ChatRequest) -> ChatRequest {
        if req.model.is_empty() {
            req.model = self.model.clone();
        }
        req.temperature = req.temperature.or(self.temperature);
        req.max_tokens = req.max_tokens.or(self.max_tokens);
        req
    }

    pub fn post(&self, url: &str) -> reqwest::RequestBuilder {
        let mut rb = self.client.post(url).header("accept", "text/event-stream");
        for (k, v) in &self.headers {
            rb = rb.header(k, v);
        }
        rb
    }
}

pub fn default_base_url(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Openai => "https://api.openai.com/v1",
        ProviderKind::Anthropic => "https://api.anthropic.com",
        ProviderKind::Gemini => "https://generativelanguage.googleapis.com",
    }
}

/// What an adapter makes of one SSE event.
#[derive(Debug, PartialEq)]
pub enum Step {
    Continue(Option<String>),
    Done,
    Error(String),
}

/// Shared response loop: HTTP errors, SSE framing, forwarding deltas.
pub(crate) async fn pump(
    resp: reqwest::Response,
    tx: &mpsc::Sender<ChatEvent>,
    mut on_event: impl FnMut(&SseEvent) -> Step,
) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let _ = tx.send(ChatEvent::Error(http_error(status.as_u16(), &body))).await;
        return Ok(());
    }
    let mut parser = SseParser::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(ChatEvent::Error(format!("connection lost: {e}"))).await;
                return Ok(());
            }
        };
        for ev in parser.feed(&chunk) {
            if forward(on_event(&ev), tx).await {
                return Ok(());
            }
        }
    }
    if let Some(ev) = parser.finish() {
        if forward(on_event(&ev), tx).await {
            return Ok(());
        }
    }
    let _ = tx.send(ChatEvent::Done).await;
    Ok(())
}

/// Returns true when the stream is finished.
async fn forward(step: Step, tx: &mpsc::Sender<ChatEvent>) -> bool {
    match step {
        Step::Continue(Some(t)) if !t.is_empty() => tx.send(ChatEvent::Delta(t)).await.is_err(),
        Step::Continue(_) => false,
        Step::Done => {
            let _ = tx.send(ChatEvent::Done).await;
            true
        }
        Step::Error(e) => {
            let _ = tx.send(ChatEvent::Error(e)).await;
            true
        }
    }
}

/// Turn an HTTP error body into one readable line.
pub fn http_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            let e = v.get("error").unwrap_or(&v);
            e.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .or_else(|| e.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    let hint = match status {
        401 | 403 => " — check the provider's API key",
        404 => " — check base_url and the model id",
        429 => " — rate limited or out of credit",
        _ => "",
    };
    format!("HTTP {status}: {}{hint}", detail.trim())
}

pub fn build(cfg: &ProviderConfig, client: reqwest::Client) -> Result<Arc<dyn Provider>> {
    let ep = Endpoint::from_config(cfg, client)?;
    Ok(match cfg.kind {
        ProviderKind::Openai => Arc::new(openai::OpenAi(ep)),
        ProviderKind::Anthropic => Arc::new(anthropic::Anthropic(ep)),
        ProviderKind::Gemini => Arc::new(gemini::Gemini(ep)),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub kind: ProviderKind,
    pub models: Vec<String>,
    pub default_model: String,
    pub is_default: bool,
}

#[derive(Default)]
pub struct Router {
    providers: Vec<Arc<dyn Provider>>,
    default: Option<String>,
}

impl Router {
    /// Build every configured provider. Ones that fail (e.g. a key command
    /// that errors) are skipped and reported as warnings.
    pub fn from_config(cfg: &Config) -> (Self, Vec<String>) {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("OpenCast/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        let mut providers = Vec::new();
        let mut warnings = Vec::new();
        for p in &cfg.providers {
            match build(p, client.clone()) {
                Ok(built) => {
                    let has_key = p.api_key_env.is_some() || p.api_key_cmd.is_some();
                    if !has_key && !p.is_local() {
                        warnings.push(format!(
                            "provider `{}` has no api_key_env or api_key_cmd (fine only if its server needs no key)",
                            p.id
                        ));
                    }
                    providers.push(built);
                }
                Err(e) => warnings.push(format!("{e:#}")),
            }
        }
        let default = cfg
            .default_provider_id()
            .filter(|d| providers.iter().any(|p| p.id() == *d))
            .map(str::to_string)
            .or_else(|| providers.first().map(|p| p.id().to_string()));
        (Self { providers, default }, warnings)
    }

    pub fn get(&self, id: Option<&str>) -> Option<Arc<dyn Provider>> {
        let id = id.or(self.default.as_deref())?;
        self.providers.iter().find(|p| p.id() == id).cloned()
    }

    pub fn list(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|p| ProviderInfo {
                id: p.id().to_string(),
                kind: p.kind(),
                models: p.models(),
                default_model: p.default_model().to_string(),
                is_default: self.default.as_deref() == Some(p.id()),
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

/// Collect a whole stream into a string (handy for tests and one-shot use).
pub async fn complete(p: &dyn Provider, req: ChatRequest) -> Result<String> {
    let (tx, mut rx) = mpsc::channel(64);
    let fut = p.stream(req, tx);
    let collect = async {
        let mut out = String::new();
        while let Some(ev) = rx.recv().await {
            match ev {
                ChatEvent::Delta(t) => out.push_str(&t),
                ChatEvent::Done => return Ok(out),
                ChatEvent::Error(e) => return Err(anyhow::anyhow!(e)),
            }
        }
        Ok(out)
    };
    let (r, out) = tokio::join!(fut, collect);
    r?;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_extracts_message() {
        let e = http_error(401, r#"{"error":{"message":"Invalid API key","type":"auth"}}"#);
        assert_eq!(e, "HTTP 401: Invalid API key — check the provider's API key");
        assert_eq!(http_error(500, "boom"), "HTTP 500: boom");
    }

    #[test]
    fn router_defaults_and_listing() {
        let cfg = Config::parse(
            r#"
            [general]
            default_provider = "b"
            [[providers]]
            id = "a"
            kind = "openai"
            base_url = "http://localhost:11434/v1/"
            model = "llama3.2"
            [[providers]]
            id = "b"
            kind = "anthropic"
            api_key_cmd = "echo k"
            model = "m1"
            models = ["m2"]
            "#,
        )
        .unwrap();
        let (r, warnings) = Router::from_config(&cfg);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(r.get(None).unwrap().id(), "b");
        assert_eq!(r.get(Some("a")).unwrap().default_model(), "llama3.2");
        let list = r.list();
        assert_eq!(list[1].models, vec!["m1", "m2"]);
        assert!(list[1].is_default);
    }
}
