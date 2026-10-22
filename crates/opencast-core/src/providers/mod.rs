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

/// How long an AI request may go without a single byte before it is dropped.
const STALL_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// A tool result (answering an assistant `tool_calls` entry).
    Tool,
}

/// A function call the model asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned id (Gemini has none; we make one up).
    pub id: String,
    pub name: String,
    /// Parsed JSON arguments (an object).
    pub arguments: serde_json::Value,
}

/// A tool offered to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// `^[a-zA-Z0-9_-]{1,64}$` — every provider accepts this.
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments object.
    pub parameters: serde_json::Value,
}

/// An image on a user turn, base64 without the `data:` wrapper. Sent only when
/// the provider says it can see (`Provider::supports_vision`); otherwise the
/// panel OCRs the image and sends text, which works with any model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    /// `image/png` | `image/jpeg` | `image/webp` | `image/gif`
    pub mime: String,
    pub b64: String,
}

/// Base64 ceiling per image (~9 MB of bytes). The panel downscreenshots and
/// re-encodes; this is the guard that keeps a 40 MB scan from being posted.
pub const MAX_IMAGE_B64: usize = 12 * 1024 * 1024;

/// Images worth sending: non-empty, under the ceiling.
pub fn usable_images(m: &Message) -> impl Iterator<Item = &Image> {
    m.images
        .iter()
        .filter(|i| !i.b64.is_empty() && i.b64.len() <= MAX_IMAGE_B64)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: String,
    /// Assistant turn that called tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Tool turn: which call this answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool turn: the tool's name (Gemini matches results by name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Images on this turn (usually a user turn). Empty keeps the wire shape plain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
            images: Vec::new(),
        }
    }

    pub fn assistant_tool_calls(content: impl Into<String>, calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: calls,
            ..Self::new(Role::Assistant, content)
        }
    }

    pub fn tool_result(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(call.id.clone()),
            name: Some(call.name.clone()),
            ..Self::new(Role::Tool, content)
        }
    }

    pub fn with_images(mut self, images: Vec<Image>) -> Self {
        self.images = images;
        self
    }

    pub fn has_images(&self) -> bool {
        !self.images.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Tools the model may call. Empty = plain chat.
    pub tools: Vec<ToolSpec>,
}

/// What the UI receives. Provider-specific shapes never leave this module.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "text", rename_all = "lowercase")]
pub enum ChatEvent {
    Delta(String),
    Done,
    Error(String),
    /// The model stopped to call tools; the caller runs them and continues.
    #[serde(rename = "toolcalls")]
    ToolCalls(Vec<ToolCall>),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn default_model(&self) -> &str;
    fn models(&self) -> Vec<String>;
    /// Whether image attachments can be sent for `model` (else the panel OCRs).
    fn supports_vision(&self, model: &str) -> bool {
        looks_vision(model)
    }
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
    /// `vision` in config: force images on/off instead of guessing from the name.
    pub vision: Option<bool>,
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
            vision: cfg.vision,
            client,
        })
    }

    /// Can this endpoint take image attachments for `model`?
    pub fn vision_for(&self, model: &str) -> bool {
        self.vision.unwrap_or_else(|| looks_vision(model))
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

/// Name-only guess at whether a model takes images, used unless config says
/// otherwise (`vision = true|false`). Deliberately conservative: a wrong "no"
/// costs a screenshot's layout (the panel OCRs instead), a wrong "yes" costs a
/// rejected request and an error the user has to read.
pub fn looks_vision(model: &str) -> bool {
    let m = model.to_lowercase();
    // Distinctive families first: everything current from these vendors sees.
    const YES: &[&str] = &[
        "vision",
        "-vl",
        "vl-",
        "pixtral",
        "llava",
        "internvl",
        "cogvlm",
        "nanvl",
        "gemini",
        "gemma",
        "claude",
        "gpt-4o",
        "gpt-4.1",
        "gpt-4-turbo",
        "gpt-4v",
        "o1",
        "o3",
        "o4",
        "kimi",
        "glm-4v",
        "qwen2-vl",
        "qwen2.5-vl",
        "qwen3-vl",
        "phi3-vision",
        "phi-3-vision",
        "minicpm-v",
        "molmo",
        "paligemma",
    ];
    YES.iter().any(|t| m.contains(t))
}

/// What an adapter makes of one SSE event.
#[derive(Debug, PartialEq)]
pub enum Step {
    Continue(Option<String>),
    Done,
    Error(String),
    /// Stream finished with tool calls instead of (or after) text.
    ToolCalls(Vec<ToolCall>),
}

/// Accumulates streamed tool-call fragments (OpenAI and Anthropic send the
/// arguments JSON in pieces).
#[derive(Debug, Default)]
pub struct ToolAccumulator {
    calls: Vec<(String, String, String)>, // id, name, arguments json text
}

impl ToolAccumulator {
    /// Start or extend call `index`.
    pub fn update(&mut self, index: usize, id: Option<&str>, name: Option<&str>, args: Option<&str>) {
        while self.calls.len() <= index {
            self.calls.push(Default::default());
        }
        let c = &mut self.calls[index];
        if let Some(id) = id.filter(|s| !s.is_empty()) {
            c.0 = id.to_string();
        }
        if let Some(n) = name.filter(|s| !s.is_empty()) {
            c.1.push_str(n);
        }
        if let Some(a) = args {
            c.2.push_str(a);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.calls.iter().all(|c| c.1.is_empty())
    }

    pub fn take(&mut self) -> Vec<ToolCall> {
        std::mem::take(&mut self.calls)
            .into_iter()
            .filter(|c| !c.1.is_empty())
            .enumerate()
            .map(|(i, (id, name, args))| ToolCall {
                id: if id.is_empty() { format!("call_{i}") } else { id },
                name,
                arguments: parse_args(&args),
            })
            .collect()
    }
}

/// Tool arguments as an object; empty or broken JSON becomes `{}`.
pub fn parse_args(text: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(text.trim()) {
        Ok(v @ serde_json::Value::Object(_)) => v,
        _ => serde_json::json!({}),
    }
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
        Step::ToolCalls(calls) => {
            let _ = tx.send(ChatEvent::ToolCalls(calls)).await;
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
    /// Images can be attached for the default model (false = the panel OCRs them).
    pub vision: bool,
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
            // Stall guard, not a cap on generation length: a request dies when no
            // byte arrives for this long (a hung tunnel, a wedged local server).
            // Long answers keep streaming as long as tokens keep coming.
            .read_timeout(Duration::from_secs(STALL_TIMEOUT_SECS))
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
                vision: p.supports_vision(p.default_model()),
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
                ChatEvent::ToolCalls(_) => return Ok(out),
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

    #[test]
    fn vision_is_guessed_from_the_model_name() {
        for m in [
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1-mini",
            "gpt-4-turbo",
            "claude-sonnet-4-5",
            "gemini-2.5-flash",
            "gemma3:12b",
            "llama3.2-vision:11b",
            "qwen2.5-vl:7b",
            "pixtral-large-latest",
            "minicpm-v",
        ] {
            assert!(looks_vision(m), "{m} should take images");
        }
        for m in [
            "llama3.1:8b",
            "llama3.3:70b",
            "deepseek-chat",
            "mistral-small-latest",
            "phi-3.5-mini",
            "gpt-3.5-turbo",
            "qwen2.5:14b",
        ] {
            assert!(!looks_vision(m), "{m} should not take images");
        }
    }

    #[test]
    fn config_can_force_vision() {
        let cfg = Config::parse(
            r#"
            [[providers]]
            id = "local"
            kind = "openai"
            base_url = "http://localhost:11434/v1"
            model = "llama3.1:8b"
            [[providers]]
            id = "forced"
            kind = "openai"
            base_url = "http://localhost:1234/v1"
            model = "mystery-model"
            vision = true
            "#,
        )
        .unwrap();
        let (r, warnings) = Router::from_config(&cfg);
        assert!(warnings.is_empty(), "{warnings:?}");
        let list = r.list();
        assert!(!list[0].vision, "a text-only local model is listed as no-images");
        assert!(list[1].vision, "vision = true overrides the name");
        // Unset config still guesses per model, so picking a vision model on the
        // same provider starts sending pixels.
        assert!(r.get(Some("local")).unwrap().supports_vision("gpt-4o"));
        assert!(!r.get(Some("local")).unwrap().supports_vision("llama3.1:8b"));
    }

    #[test]
    fn images_round_trip_through_the_panel_json() {
        let m = Message::new(Role::User, "what is in this shot?").with_images(vec![Image {
            mime: "image/png".into(),
            b64: "aGVsbG8=".into(),
        }]);
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"b64\":\"aGVsbG8=\""), "{j}");
        let back: Message = serde_json::from_str(&j).unwrap();
        assert_eq!(back, m);
        // A message without images keeps the old shape (no `images` key at all).
        assert!(!serde_json::to_string(&Message::new(Role::User, "hi"))
            .unwrap()
            .contains("images"));
    }

    #[test]
    fn oversized_or_empty_images_are_dropped() {
        let m = Message::new(Role::User, "x").with_images(vec![
            Image {
                mime: "image/png".into(),
                b64: String::new(),
            },
            Image {
                mime: "image/png".into(),
                b64: "x".repeat(MAX_IMAGE_B64 + 1),
            },
            Image {
                mime: "image/jpeg".into(),
                b64: "ok".into(),
            },
        ]);
        let kept: Vec<&Image> = usable_images(&m).collect();
        assert_eq!(kept.len(), 1, "only the sane one survives");
    }
}
