//! Model Context Protocol client.
//!
//! MCP servers give OpenCast thousands of integrations without an extension
//! store: their tools show up as launcher commands, and the AI chat can call
//! them. Two transports:
//! - **stdio** (`command` + `args`): newline-delimited JSON-RPC over the
//!   child's stdin/stdout — how `npx`, `uvx` and `docker run -i` servers work;
//! - **HTTP** (`url`): Streamable HTTP; a POST per request, the reply is JSON
//!   or a short SSE stream. Static headers only (no OAuth yet).
//!
//! Tool lists are cached in `<config dir>/mcp-tools.json` so the launcher can
//! list MCP commands at startup without starting every server.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// `[[mcp_servers]]` in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    /// Short id, used in command ids and tool names (`github`).
    pub id: String,
    /// stdio: program to run (`npx`, `uvx`, `/usr/local/bin/my-server`).
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment for the server. Values are literal.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Environment whose values come from a command (Keychain, 1Password…),
    /// e.g. `GITHUB_TOKEN = "security find-generic-password -s gh -w"`.
    #[serde(default)]
    pub env_cmd: HashMap<String, String>,
    /// HTTP: the server's MCP endpoint.
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Header values from a command, e.g. `Authorization = "echo Bearer $(op read …)"`.
    #[serde(default)]
    pub headers_cmd: HashMap<String, String>,
    /// Only these tools (exact names or `prefix*`). Empty = all.
    #[serde(default)]
    pub tools: Vec<String>,
    /// List the tools as launcher commands.
    #[serde(default = "yes")]
    pub in_launcher: bool,
    /// Offer the tools to the AI chat.
    #[serde(default = "yes")]
    pub in_chat: bool,
    /// Tools the AI may call without asking (`prefix*` allowed). `["*"]` = never ask.
    #[serde(default)]
    pub auto_approve: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn yes() -> bool {
    true
}

fn default_timeout() -> u64 {
    60
}

impl McpServerConfig {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("mcp server id `{}` must be letters, digits, - or _", self.id);
        }
        match (&self.command, &self.url) {
            (Some(_), None) | (None, Some(_)) => Ok(()),
            _ => bail!("mcp server `{}` needs exactly one of `command` or `url`", self.id),
        }
    }

    pub fn allows(&self, tool: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|p| glob(p, tool))
    }

    pub fn auto_approves(&self, tool: &str) -> bool {
        self.auto_approve.iter().any(|p| glob(p, tool))
    }
}

/// `*` = anything, `read_*` = prefix, otherwise exact.
fn glob(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub server: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "empty_schema", rename = "inputSchema")]
    pub input_schema: Value,
    /// Server says the tool does not change anything.
    #[serde(default)]
    pub read_only: bool,
}

fn empty_schema() -> Value {
    json!({"type": "object", "properties": {}})
}

impl McpTool {
    /// Launcher command id.
    pub fn command_id(&self) -> String {
        format!("mcp:{}:{}", self.server, self.name)
    }

    /// Name offered to the model: `server__tool`, sanitized to `[a-zA-Z0-9_-]{1,64}`.
    pub fn llm_name(&self) -> String {
        let raw = format!("{}__{}", self.server, self.name);
        let mut s: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        s.truncate(64);
        s
    }

    /// Human title: the server's `title`, else `create_issue` → `Create Issue`.
    pub fn display_title(&self) -> String {
        if let Some(t) = self.title.as_deref().filter(|t| !t.trim().is_empty()) {
            return t.trim().to_string();
        }
        self.name
            .split(['_', '-', '.'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Scalar parameters in launch order: required first. `(name, type, required, description)`.
    pub fn params(&self) -> Vec<(String, String, bool, String)> {
        let required: Vec<String> = self.input_schema["required"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        if let Some(props) = self.input_schema["properties"].as_object() {
            for (name, p) in props {
                let ty = p["type"].as_str().unwrap_or("string").to_string();
                let desc = p["description"].as_str().unwrap_or("").to_string();
                out.push((name.clone(), ty, required.contains(name), desc));
            }
        }
        out.sort_by_key(|(n, _, req, _)| (!*req, required.iter().position(|r| r == n).unwrap_or(usize::MAX)));
        out
    }

    /// True when every required parameter is a scalar and there are at most
    /// three of them (one argument field each in the launcher).
    pub fn simple_form(&self) -> bool {
        let params = self.params();
        let required: Vec<_> = params.iter().filter(|p| p.2).collect();
        required.len() <= 3
            && required
                .iter()
                .all(|p| matches!(p.1.as_str(), "string" | "number" | "integer" | "boolean"))
    }

    /// Arguments object from launcher field values (see [`Self::simple_form`]).
    /// Non-simple tools take one JSON field.
    pub fn arguments_from(&self, values: &[String]) -> Result<Value> {
        if !self.simple_form() {
            let text = values.first().map(String::as_str).unwrap_or("").trim();
            if text.is_empty() {
                return Ok(json!({}));
            }
            let v: Value = serde_json::from_str(text).context("arguments must be a JSON object")?;
            if !v.is_object() {
                bail!("arguments must be a JSON object");
            }
            return Ok(v);
        }
        let mut obj = serde_json::Map::new();
        for (i, (name, ty, required, _)) in self.params().into_iter().take(3).enumerate() {
            let value = values.get(i).map(String::as_str).unwrap_or("").trim();
            if value.is_empty() {
                if required {
                    bail!("`{name}` is required");
                }
                continue;
            }
            let v = match ty.as_str() {
                "number" => json!(value
                    .parse::<f64>()
                    .with_context(|| format!("`{name}` must be a number"))?),
                "integer" => json!(value
                    .parse::<i64>()
                    .with_context(|| format!("`{name}` must be a whole number"))?),
                "boolean" => json!(matches!(value.to_lowercase().as_str(), "true" | "yes" | "y" | "1")),
                _ => json!(value),
            };
            obj.insert(name, v);
        }
        Ok(Value::Object(obj))
    }
}

// ---- resources and prompts --------------------------------------------------

/// A document a server exposes (`resources/list`): a file, a page, a DB row.
/// Reading it is a launcher command, so the text lands in the panel instead of
/// needing a chat round trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResource {
    pub server: String,
    pub uri: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "mimeType")]
    pub mime: String,
}

impl McpResource {
    pub fn command_id(&self) -> String {
        format!("mcp:{}:res:{}", self.server, self.uri)
    }

    /// Title: the server's `name`, else the last path-ish segment of the URI.
    pub fn display_title(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        self.uri
            .rsplit(['/', ':'])
            .find(|s| !s.is_empty())
            .unwrap_or(&self.uri)
            .to_string()
    }
}

/// One argument of a server-side prompt template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpPromptArg {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// A prompt template a server exposes (`prompts/list`): the server fills it in,
/// so the same wording stays in the server's own hands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpPrompt {
    pub server: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub arguments: Vec<McpPromptArg>,
}

impl McpPrompt {
    pub fn command_id(&self) -> String {
        format!("mcp:{}:prompt:{}", self.server, self.name)
    }

    pub fn display_title(&self) -> String {
        if let Some(t) = self.title.as_deref().filter(|t| !t.trim().is_empty()) {
            return t.trim().to_string();
        }
        self.name
            .split(['_', '-', '.'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Positional launcher args → the `arguments` object the server expects.
    pub fn arguments_from(&self, values: &[String]) -> Result<Value> {
        let required: Vec<&McpPromptArg> = self.arguments.iter().filter(|a| a.required).collect();
        if values.len() < required.len() {
            bail!(
                "prompt `{}` needs {} argument(s): {}",
                self.name,
                required.len(),
                required.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
        let mut obj = serde_json::Map::new();
        for (arg, value) in self.arguments.iter().zip(values) {
            obj.insert(arg.name.clone(), json!(value));
        }
        Ok(Value::Object(obj))
    }
}

/// Resources and prompts, cached on disk like tools: the launcher lists commands
/// without connecting to a single server.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtrasCache {
    #[serde(default)]
    pub resources: HashMap<String, Vec<McpResource>>,
    #[serde(default)]
    pub prompts: HashMap<String, Vec<McpPrompt>>,
}

fn extras_path(dir: &Path) -> std::path::PathBuf {
    dir.join("mcp-extras.json")
}

pub fn load_extras(dir: &Path) -> ExtrasCache {
    std::fs::read_to_string(extras_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_extras(dir: &Path, extras: &ExtrasCache) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(s) = serde_json::to_string_pretty(extras) {
        let _ = std::fs::write(extras_path(dir), s);
    }
}

// ---- tool cache -------------------------------------------------------------

pub type ToolCache = HashMap<String, Vec<McpTool>>;

fn cache_path(dir: &Path) -> std::path::PathBuf {
    dir.join("mcp-tools.json")
}

pub fn load_cache(dir: &Path) -> ToolCache {
    std::fs::read_to_string(cache_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_cache(dir: &Path, cache: &ToolCache) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(s) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(cache_path(dir), s);
    }
}

// ---- tool results -----------------------------------------------------------

/// Flatten a `tools/call` result into text for the launcher or the model.
pub fn result_text(result: &Value) -> (String, bool) {
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let mut parts: Vec<String> = Vec::new();
    if let Some(content) = result["content"].as_array() {
        for c in content {
            match c["type"].as_str() {
                Some("text") => parts.push(c["text"].as_str().unwrap_or("").to_string()),
                Some("image") => parts.push(format!("[image {}]", c["mimeType"].as_str().unwrap_or(""))),
                Some("audio") => parts.push("[audio]".into()),
                Some("resource") => {
                    let r = &c["resource"];
                    parts.push(
                        r["text"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("[resource {}]", r["uri"].as_str().unwrap_or(""))),
                    )
                }
                Some("resource_link") => parts.push(format!(
                    "[{}]({})",
                    c["name"].as_str().unwrap_or("link"),
                    c["uri"].as_str().unwrap_or("")
                )),
                _ => {}
            }
        }
    }
    if parts.is_empty() {
        if let Some(sc) = result.get("structuredContent") {
            parts.push(serde_json::to_string_pretty(sc).unwrap_or_default());
        }
    }
    (parts.join("\n\n"), is_error)
}

// ---- client -----------------------------------------------------------------

enum Transport {
    Stdio {
        _child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    },
    Http {
        client: reqwest::Client,
        url: String,
        headers: Vec<(String, String)>,
        session: Option<String>,
    },
}

/// One live connection. Requests are serialized (one in flight).
pub struct McpClient {
    pub server: McpServerConfig,
    transport: Transport,
    next_id: u64,
}

fn run_cmd(cmd: &str) -> Result<String> {
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .with_context(|| format!("running `{cmd}`"))?;
    if !out.status.success() {
        bail!("`{cmd}` failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl McpClient {
    /// Start (stdio) or connect (HTTP), then run the `initialize` handshake.
    pub async fn connect(server: McpServerConfig) -> Result<Self> {
        server.validate()?;
        let transport = if let Some(cmd) = &server.command {
            let mut c = tokio::process::Command::new(cmd);
            c.args(&server.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            // GUI apps get a bare PATH; servers are usually launched via npx/uvx/docker.
            c.env("PATH", crate::scripts::path_with_extras());
            for (k, v) in &server.env {
                c.env(k, v);
            }
            for (k, cmd) in &server.env_cmd {
                c.env(
                    k,
                    run_cmd(cmd).with_context(|| format!("mcp `{}`: env {k}", server.id))?,
                );
            }
            let mut child = c
                .spawn()
                .with_context(|| format!("mcp `{}`: starting `{cmd}`", server.id))?;
            let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
            let stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?);
            Transport::Stdio {
                _child: child,
                stdin,
                stdout,
            }
        } else {
            let url = server.url.clone().unwrap_or_default();
            let mut headers: Vec<(String, String)> =
                server.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, cmd) in &server.headers_cmd {
                headers.push((
                    k.clone(),
                    run_cmd(cmd).with_context(|| format!("mcp `{}`: header {k}", server.id))?,
                ));
            }
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .context("http client")?;
            Transport::Http {
                client,
                url,
                headers,
                session: None,
            }
        };
        let mut me = Self {
            server,
            transport,
            next_id: 1,
        };
        let init = me
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "OpenCast", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await
            .with_context(|| format!("mcp `{}`: initialize", me.server.id))?;
        let _ = init;
        me.notify("notifications/initialized", json!({})).await?;
        Ok(me)
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.server.timeout_secs.max(1))
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        match &mut self.transport {
            Transport::Stdio { stdin, .. } => {
                let mut line = msg.to_string();
                line.push('\n');
                stdin.write_all(line.as_bytes()).await?;
                stdin.flush().await?;
            }
            Transport::Http {
                client,
                url,
                headers,
                session,
            } => {
                let mut rb = client
                    .post(url.as_str())
                    .header("accept", "application/json, text/event-stream")
                    .header("mcp-protocol-version", PROTOCOL_VERSION)
                    .json(&msg);
                for (k, v) in headers.iter() {
                    rb = rb.header(k, v);
                }
                if let Some(s) = session {
                    rb = rb.header("mcp-session-id", s.as_str());
                }
                let _ = rb.send().await?;
            }
        }
        Ok(())
    }

    /// One JSON-RPC request; returns `result` or the server's error.
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let timeout = self.timeout();
        let reply = tokio::time::timeout(timeout, self.exchange(id, msg))
            .await
            .map_err(|_| {
                anyhow!(
                    "mcp `{}`: `{method}` timed out after {}s",
                    self.server.id,
                    timeout.as_secs()
                )
            })??;
        if let Some(e) = reply.get("error") {
            bail!(
                "mcp `{}`: {}",
                self.server.id,
                e["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| e.to_string())
            );
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn exchange(&mut self, id: u64, msg: Value) -> Result<Value> {
        match &mut self.transport {
            Transport::Stdio { stdin, stdout, .. } => {
                let mut line = msg.to_string();
                line.push('\n');
                stdin.write_all(line.as_bytes()).await?;
                stdin.flush().await?;
                let mut buf = String::new();
                loop {
                    buf.clear();
                    let n = stdout.read_line(&mut buf).await?;
                    if n == 0 {
                        bail!("mcp server exited");
                    }
                    let Ok(v) = serde_json::from_str::<Value>(buf.trim()) else {
                        continue; // log noise on stdout
                    };
                    if v.get("id").and_then(Value::as_u64) == Some(id) && v.get("method").is_none() {
                        return Ok(v);
                    }
                    // A request from the server (sampling, roots…): we support none of them.
                    if let (Some(req_id), Some(_)) = (v.get("id"), v.get("method")) {
                        let reply = json!({"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": "not supported by OpenCast"}});
                        let mut l = reply.to_string();
                        l.push('\n');
                        stdin.write_all(l.as_bytes()).await?;
                        stdin.flush().await?;
                    }
                    // Notifications are ignored.
                }
            }
            Transport::Http {
                client,
                url,
                headers,
                session,
            } => {
                let mut rb = client
                    .post(url.as_str())
                    .header("accept", "application/json, text/event-stream")
                    .header("mcp-protocol-version", PROTOCOL_VERSION)
                    .json(&msg);
                for (k, v) in headers.iter() {
                    rb = rb.header(k, v);
                }
                if let Some(s) = session.as_deref() {
                    rb = rb.header("mcp-session-id", s);
                }
                let resp = rb.send().await?;
                if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
                    *session = Some(s.to_string());
                }
                let status = resp.status();
                let is_sse = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|ct| ct.starts_with("text/event-stream"));
                let body = resp.text().await?;
                if !status.is_success() {
                    bail!(
                        "HTTP {}: {}",
                        status.as_u16(),
                        body.chars().take(300).collect::<String>()
                    );
                }
                if is_sse {
                    let mut parser = crate::providers::sse::SseParser::new();
                    let mut events = parser.feed(body.as_bytes());
                    events.extend(parser.finish());
                    for ev in events {
                        if let Ok(v) = serde_json::from_str::<Value>(&ev.data) {
                            if v.get("id").and_then(Value::as_u64) == Some(id) {
                                return Ok(v);
                            }
                        }
                    }
                    bail!("no reply in the event stream");
                }
                serde_json::from_str(&body).context("server reply is not JSON")
            }
        }
    }

    /// All tools (follows pagination), filtered by the server's allowlist.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = self.request("tools/list", params).await?;
            for t in r["tools"].as_array().cloned().unwrap_or_default() {
                let name = t["name"].as_str().unwrap_or_default().to_string();
                if name.is_empty() || !self.server.allows(&name) {
                    continue;
                }
                out.push(McpTool {
                    server: self.server.id.clone(),
                    title: t["title"]
                        .as_str()
                        .or(t["annotations"]["title"].as_str())
                        .map(str::to_string),
                    description: t["description"].as_str().unwrap_or("").to_string(),
                    input_schema: if t["inputSchema"].is_object() {
                        t["inputSchema"].clone()
                    } else {
                        empty_schema()
                    },
                    read_only: t["annotations"]["readOnlyHint"].as_bool().unwrap_or(false),
                    name,
                });
            }
            cursor = r["nextCursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Call a tool; returns its text output. A tool-reported error is an `Err`.
    pub async fn call(&mut self, tool: &str, arguments: Value) -> Result<String> {
        let r = self
            .request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await?;
        let (text, is_error) = result_text(&r);
        if is_error {
            bail!(
                "{}",
                if text.is_empty() {
                    "the tool reported an error"
                } else {
                    &text
                }
            );
        }
        Ok(text)
    }

    /// Every resource the server exposes (follows pagination). A server without
    /// the resources capability is not an error — it just has none.
    pub async fn list_resources(&mut self) -> Result<Vec<McpResource>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = match self.request("resources/list", params).await {
                Ok(r) => r,
                Err(e) => return unsupported_to_empty(e, "resources"),
            };
            for it in r["resources"].as_array().cloned().unwrap_or_default() {
                let uri = it["uri"].as_str().unwrap_or_default().to_string();
                if uri.is_empty() {
                    continue;
                }
                out.push(McpResource {
                    server: self.server.id.clone(),
                    name: it["name"].as_str().unwrap_or("").to_string(),
                    description: it["description"].as_str().unwrap_or("").to_string(),
                    mime: it["mimeType"].as_str().unwrap_or("").to_string(),
                    uri,
                });
            }
            cursor = r["nextCursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Read one resource as text. Binary parts become a placeholder line, so a
    /// mixed result still reads.
    pub async fn read_resource(&mut self, uri: &str) -> Result<String> {
        let r = self.request("resources/read", json!({"uri": uri})).await?;
        let mut parts: Vec<String> = Vec::new();
        for c in r["contents"].as_array().cloned().unwrap_or_default() {
            if let Some(t) = c["text"].as_str() {
                parts.push(t.to_string());
            } else if let Some(b) = c["blob"].as_str() {
                let mime = c["mimeType"].as_str().unwrap_or("binary");
                parts.push(format!("[{mime} data, {} bytes — not text]", b.len()));
            }
        }
        if parts.is_empty() {
            bail!("mcp `{}`: resource `{uri}` returned nothing", self.server.id);
        }
        Ok(parts.join("\n\n"))
    }

    /// Every prompt template (follows pagination); empty when unsupported.
    pub async fn list_prompts(&mut self) -> Result<Vec<McpPrompt>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = match self.request("prompts/list", params).await {
                Ok(r) => r,
                Err(e) => return unsupported_to_empty(e, "prompts"),
            };
            for p in r["prompts"].as_array().cloned().unwrap_or_default() {
                let name = p["name"].as_str().unwrap_or_default().to_string();
                if name.is_empty() {
                    continue;
                }
                out.push(McpPrompt {
                    server: self.server.id.clone(),
                    title: p["title"].as_str().map(str::to_string),
                    description: p["description"].as_str().unwrap_or("").to_string(),
                    arguments: p["arguments"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|a| McpPromptArg {
                            name: a["name"].as_str().unwrap_or_default().to_string(),
                            description: a["description"].as_str().unwrap_or("").to_string(),
                            required: a["required"].as_bool().unwrap_or(false),
                        })
                        .filter(|a| !a.name.is_empty())
                        .collect(),
                    name,
                });
            }
            cursor = r["nextCursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Ask the server to fill in one of its prompt templates; returns the text.
    pub async fn get_prompt(&mut self, name: &str, arguments: Value) -> Result<String> {
        let r = self
            .request("prompts/get", json!({"name": name, "arguments": arguments}))
            .await?;
        let msgs: Vec<String> = r["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|m| m["content"]["text"].as_str().map(str::to_string))
            .collect();
        if msgs.is_empty() {
            bail!("mcp `{}`: prompt `{name}` returned no text", self.server.id);
        }
        Ok(msgs.join("\n\n"))
    }
}

/// A server that has not implemented a method answers -32601; that is "none", not
/// a failure worth a warning for every refresh.
fn unsupported_to_empty<T>(e: anyhow::Error, what: &str) -> Result<Vec<T>> {
    let m = e.to_string().to_lowercase();
    if m.contains("not found") || m.contains("not supported") || m.contains("unknown method") {
        return Ok(Vec::new());
    }
    Err(e.context(format!("mcp: listing {what}")))
}
/// Keeps one live client per server, started on first use.
#[derive(Default)]
pub struct McpHub {
    servers: HashMap<String, McpServerConfig>,
    clients: Mutex<HashMap<String, Arc<Mutex<McpClient>>>>,
}

impl McpHub {
    pub fn new(servers: &[McpServerConfig]) -> Self {
        Self {
            servers: servers.iter().map(|s| (s.id.clone(), s.clone())).collect(),
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub fn server(&self, id: &str) -> Option<&McpServerConfig> {
        self.servers.get(id)
    }

    pub fn server_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self.servers.keys().cloned().collect();
        v.sort();
        v
    }

    async fn client(&self, id: &str) -> Result<Arc<Mutex<McpClient>>> {
        let mut clients = self.clients.lock().await;
        if let Some(c) = clients.get(id) {
            return Ok(c.clone());
        }
        let cfg = self
            .servers
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("no mcp server `{id}` in config"))?;
        let c = Arc::new(Mutex::new(McpClient::connect(cfg).await?));
        clients.insert(id.to_string(), c.clone());
        Ok(c)
    }

    /// Drop a broken connection so the next call restarts the server.
    async fn forget(&self, id: &str) {
        self.clients.lock().await.remove(id);
    }

    pub async fn list_tools(&self, id: &str) -> Result<Vec<McpTool>> {
        let c = self.client(id).await?;
        let r = c.lock().await.list_tools().await;
        if r.is_err() {
            self.forget(id).await;
        }
        r
    }

    /// Fetch every server's tools, resources and prompts. Failures are reported,
    /// not fatal: one dead server must not hide the others' commands.
    pub async fn refresh_all(&self) -> (ToolCache, ExtrasCache, Vec<String>) {
        let mut cache = ToolCache::new();
        let mut extras = ExtrasCache::default();
        let mut errors = Vec::new();
        for id in self.server_ids() {
            match self.list_tools(&id).await {
                Ok(tools) => {
                    cache.insert(id.clone(), tools);
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
            // Resources and prompts are optional capabilities: a server that has
            // them gains commands, one that does not costs nothing.
            match self.list_resources(&id).await {
                Ok(r) => {
                    if !r.is_empty() {
                        extras.resources.insert(id.clone(), r);
                    }
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
            match self.list_prompts(&id).await {
                Ok(p) => {
                    if !p.is_empty() {
                        extras.prompts.insert(id.clone(), p);
                    }
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        (cache, extras, errors)
    }

    pub async fn list_resources(&self, id: &str) -> Result<Vec<McpResource>> {
        let c = self.client(id).await?;
        let r = c.lock().await.list_resources().await;
        if r.is_err() {
            self.forget(id).await;
        }
        r
    }

    pub async fn list_prompts(&self, id: &str) -> Result<Vec<McpPrompt>> {
        let c = self.client(id).await?;
        let r = c.lock().await.list_prompts().await;
        if r.is_err() {
            self.forget(id).await;
        }
        r
    }

    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<String> {
        let c = self.client(server).await?;
        let r = c.lock().await.read_resource(uri).await;
        self.reconnect_if_dead(server, &r).await;
        r
    }

    pub async fn get_prompt(&self, server: &str, name: &str, arguments: Value) -> Result<String> {
        let c = self.client(server).await?;
        let r = c.lock().await.get_prompt(name, arguments).await;
        self.reconnect_if_dead(server, &r).await;
        r
    }

    /// A transport failure (not a tool or server error) means the process died:
    /// drop the client so the next call starts a fresh one.
    async fn reconnect_if_dead(&self, server: &str, r: &Result<String>) {
        let Err(e) = r else { return };
        let msg = format!("{e:#}");
        if msg.contains("exited") || msg.contains("timed out") || msg.contains("Broken pipe") {
            self.forget(server).await;
        }
    }

    pub async fn call(&self, server: &str, tool: &str, arguments: Value) -> Result<String> {
        let c = self.client(server).await?;
        let r = c.lock().await.call(tool, arguments).await;
        self.reconnect_if_dead(server, &r).await;
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(schema: Value) -> McpTool {
        McpTool {
            server: "gh".into(),
            name: "create_issue".into(),
            title: None,
            description: "Create an issue".into(),
            input_schema: schema,
            read_only: false,
        }
    }

    #[test]
    fn names_titles_and_ids() {
        let t = tool(empty_schema());
        assert_eq!(t.command_id(), "mcp:gh:create_issue");
        assert_eq!(t.llm_name(), "gh__create_issue");
        assert_eq!(t.display_title(), "Create Issue");
        let mut odd = t.clone();
        odd.name = "files/read.text".into();
        assert_eq!(odd.llm_name(), "gh__files_read_text");
    }

    #[test]
    fn simple_forms_and_arguments() {
        let t = tool(json!({
            "type": "object",
            "properties": {
                "body": {"type": "string"},
                "repo": {"type": "string"},
                "title": {"type": "string"},
                "count": {"type": "integer"}
            },
            "required": ["repo", "title"]
        }));
        assert!(t.simple_form());
        let p = t.params();
        assert_eq!(p[0].0, "repo");
        assert_eq!(p[1].0, "title");
        let args = t.arguments_from(&["me/x".into(), "Bug".into(), "".into()]).unwrap();
        assert_eq!(args, json!({"repo": "me/x", "title": "Bug"}));
        assert!(t.arguments_from(&["".into(), "Bug".into()]).is_err());

        let complex = tool(json!({
            "type": "object",
            "properties": {"labels": {"type": "array"}},
            "required": ["labels"]
        }));
        assert!(!complex.simple_form());
        assert_eq!(
            complex.arguments_from(&[r#"{"labels":["a"]}"#.into()]).unwrap(),
            json!({"labels": ["a"]})
        );
        assert!(complex.arguments_from(&["[1]".into()]).is_err());
    }

    #[test]
    fn allowlists() {
        let s = McpServerConfig {
            id: "gh".into(),
            command: Some("x".into()),
            args: vec![],
            env: HashMap::new(),
            env_cmd: HashMap::new(),
            url: None,
            headers: HashMap::new(),
            headers_cmd: HashMap::new(),
            tools: vec!["get_*".into(), "create_issue".into()],
            in_launcher: true,
            in_chat: true,
            auto_approve: vec!["get_*".into()],
            timeout_secs: 5,
        };
        assert!(s.validate().is_ok());
        assert!(s.allows("get_me") && s.allows("create_issue") && !s.allows("delete_repo"));
        assert!(s.auto_approves("get_me") && !s.auto_approves("create_issue"));
        let mut bad = s.clone();
        bad.url = Some("http://x".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn flattens_results() {
        let (t, err) = result_text(&json!({
            "content": [{"type": "text", "text": "a"}, {"type": "image", "mimeType": "image/png", "data": ""}],
        }));
        assert_eq!(t, "a\n\n[image image/png]");
        assert!(!err);
        let (_, err) = result_text(&json!({"content": [], "isError": true}));
        assert!(err);
        let (t, _) = result_text(&json!({"content": [], "structuredContent": {"n": 1}}));
        assert!(t.contains("\"n\": 1"));
    }

    /// A tiny MCP server in shell + python-free sh: answers initialize, tools,
    /// resources and prompts.
    #[cfg(unix)]
    #[tokio::test]
    async fn stdio_roundtrip_against_a_fake_server() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("server.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"serverInfo\":{\"name\":\"fake\"}}}" ;;
    *'"method":"tools/list"'*) echo 'noise on stdout'; echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}},\"required\":[\"text\"]}},{\"name\":\"hidden\"}]}}" ;;
    *'"method":"tools/call"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"pong\"}]}}" ;;
    *'"method":"resources/list"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"resources\":[{\"uri\":\"notes://today\",\"name\":\"Today's note\",\"description\":\"The daily scratch note\",\"mimeType\":\"text/markdown\"}]}}" ;;
    *'"method":"resources/read"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"contents\":[{\"uri\":\"notes://today\",\"mimeType\":\"text/markdown\",\"text\":\"hello from the note\"},{\"uri\":\"notes://today\",\"mimeType\":\"image/png\",\"blob\":\"AAAA\"}]}}" ;;
    *'"method":"prompts/list"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"prompts\":[{\"name\":\"summarize_note\",\"title\":\"Summarize a note\",\"description\":\"Summarizes one note\",\"arguments\":[{\"name\":\"day\",\"description\":\"which day\",\"required\":true}]}]}}" ;;
    *'"method":"prompts/get"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"messages\":[{\"role\":\"user\",\"content\":{\"type\":\"text\",\"text\":\"Summarize the note for today\"}}]}}" ;;
  esac
done
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cfg = McpServerConfig {
            id: "fake".into(),
            command: Some(script.to_string_lossy().into_owned()),
            args: vec![],
            env: HashMap::new(),
            env_cmd: HashMap::new(),
            url: None,
            headers: HashMap::new(),
            headers_cmd: HashMap::new(),
            tools: vec!["echo".into()],
            in_launcher: true,
            in_chat: true,
            auto_approve: vec![],
            timeout_secs: 5,
        };
        let hub = McpHub::new(&[cfg]);
        let tools = hub.list_tools("fake").await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].params()[0].0, "text");
        let out = hub.call("fake", "echo", json!({"text": "ping"})).await.unwrap();
        assert_eq!(out, "pong");

        // Resources: listed, then read as text (a binary part becomes a note).
        let res = hub.list_resources("fake").await.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].uri, "notes://today");
        assert_eq!(res[0].mime, "text/markdown");
        let text = hub.read_resource("fake", "notes://today").await.unwrap();
        assert!(text.contains("hello from the note"), "{text}");
        assert!(text.contains("image/png data"), "{text}");

        // Prompts: listed with their arguments, then filled in by the server.
        let prompts = hub.list_prompts("fake").await.unwrap();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].display_title(), "Summarize a note");
        assert_eq!(prompts[0].arguments[0].name, "day");
        let args = prompts[0].arguments_from(&["today".to_string()]).unwrap();
        assert_eq!(args["day"], json!("today"));
        let filled = hub.get_prompt("fake", "summarize_note", args).await.unwrap();
        assert_eq!(filled, "Summarize the note for today");

        // refresh_all collects all three, and the extras land in their own cache.
        let (tools, extras, errors) = hub.refresh_all().await;
        assert_eq!(tools["fake"].len(), 1, "{errors:?}");
        assert_eq!(extras.resources["fake"].len(), 1);
        assert_eq!(extras.prompts["fake"].len(), 1);
    }

    #[test]
    fn a_server_without_the_capability_lists_nothing() {
        let e = anyhow!("mcp `gh`: jsonrpc error -32601: Method not found: resources/list");
        assert_eq!(unsupported_to_empty::<McpResource>(e, "resources").unwrap().len(), 0);
        // A real failure still surfaces.
        let e2 = anyhow!("mcp `gh`: timed out");
        assert!(unsupported_to_empty::<McpResource>(e2, "resources").is_err());
    }

    #[test]
    fn resources_and_prompts_have_ids_and_titles() {
        let r = McpResource {
            server: "notes".into(),
            uri: "notes://today".into(),
            name: "".into(),
            description: "Daily".into(),
            mime: "text/markdown".into(),
        };
        assert_eq!(r.command_id(), "mcp:notes:res:notes://today");
        assert_eq!(r.display_title(), "today");
        let named = McpResource {
            name: "Today's note".into(),
            ..r.clone()
        };
        assert_eq!(named.display_title(), "Today's note");

        let p = McpPrompt {
            server: "notes".into(),
            name: "summarize_note".into(),
            title: None,
            description: "".into(),
            arguments: vec![McpPromptArg {
                name: "day".into(),
                description: "".into(),
                required: true,
            }],
        };
        assert_eq!(p.command_id(), "mcp:notes:prompt:summarize_note");
        assert_eq!(p.display_title(), "Summarize Note");
        // Missing a required argument is explained, not guessed.
        let e = p.arguments_from(&[]).unwrap_err().to_string();
        assert!(e.contains("needs 1 argument(s): day"), "{e}");
        assert_eq!(p.arguments_from(&["fri".into()]).unwrap()["day"], json!("fri"));
    }

    #[test]
    fn extras_cache_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut extras = ExtrasCache::default();
        extras.resources.insert(
            "notes".into(),
            vec![McpResource {
                server: "notes".into(),
                uri: "notes://today".into(),
                name: "Today".into(),
                description: "".into(),
                mime: "text/markdown".into(),
            }],
        );
        save_extras(dir.path(), &extras);
        assert_eq!(load_extras(dir.path()), extras);
        // An older install without this file simply has none.
        assert_eq!(load_extras(&dir.path().join("nope")), ExtrasCache::default());
    }
}
