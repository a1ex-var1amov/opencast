//! AI chat with tools: streams a provider, runs the MCP tools the model asks
//! for (after the user approves, unless the server auto-approves them), feeds
//! the results back, and repeats until the model answers.
//!
//! Also owns the process-wide MCP hub (one live connection per server).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use opencast_core::engine::ChatTool;
use opencast_core::mcp::{McpHub, McpServerConfig};
use opencast_core::providers::{ChatEvent, ChatRequest, Message, Provider, ToolCall};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use crate::host::{emit, runtime, with_engine};

/// Most model ↔ tool round trips per answer.
const MAX_ROUNDS: usize = 8;
/// Longest tool output handed back to the model.
const MAX_TOOL_OUTPUT: usize = 20_000;

static HUB: LazyLock<Mutex<(Vec<McpServerConfig>, Arc<McpHub>)>> =
    LazyLock::new(|| Mutex::new((Vec::new(), Arc::new(McpHub::default()))));

/// `requestId:callId` → answer channel for a pending approval.
static APPROVALS: LazyLock<Mutex<HashMap<String, oneshot::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The hub for the current config. Rebuilt (servers restarted) only when the
/// `[[mcp_servers]]` section changed.
pub fn hub() -> Arc<McpHub> {
    let servers = with_engine(|e| e.mcp_servers().to_vec());
    let mut guard = HUB.lock().unwrap();
    if guard.0 != servers {
        *guard = (servers.clone(), Arc::new(McpHub::new(&servers)));
    }
    guard.1.clone()
}

/// Fetch every server's tools, resources and prompts and cache them. Returns
/// (tool count, errors); reloads the engine when anything changed so new
/// commands appear.
pub async fn refresh_tools() -> (usize, Vec<String>) {
    let hub = hub();
    let dir = with_engine(|e| e.config_dir.clone());
    if hub.server_ids().is_empty() {
        return (0, Vec::new());
    }
    let (cache, extras, errors) = hub.refresh_all().await;
    let mut merged = opencast_core::mcp::load_cache(&dir);
    // Keep the old list for servers that failed this time; drop removed servers.
    merged.retain(|k, _| hub.server_ids().contains(k));
    let changed = cache.iter().any(|(k, v)| merged.get(k) != Some(v));
    for (k, v) in cache {
        merged.insert(k, v);
    }
    // Resources and prompts are in their own file, so an older cache still loads.
    let mut merged_extras = opencast_core::mcp::load_extras(&dir);
    merged_extras.resources.retain(|k, _| hub.server_ids().contains(k));
    merged_extras.prompts.retain(|k, _| hub.server_ids().contains(k));
    let before = merged_extras.clone();
    for (k, v) in extras.resources {
        merged_extras.resources.insert(k, v);
    }
    for (k, v) in extras.prompts {
        merged_extras.prompts.insert(k, v);
    }
    let count = merged.values().map(Vec::len).sum();
    if changed || merged_extras != before {
        opencast_core::mcp::save_cache(&dir, &merged);
        opencast_core::mcp::save_extras(&dir, &merged_extras);
        crate::exec::reload();
    }
    (count, errors)
}

/// Panel answered an approval prompt.
pub fn answer_approval(request_id: &str, call_id: &str, allow: bool) -> bool {
    let key = format!("{request_id}:{call_id}");
    match APPROVALS.lock().unwrap().remove(&key) {
        Some(tx) => tx.send(allow).is_ok(),
        None => false,
    }
}

fn emit_ai(request_id: &str, event: serde_json::Value) {
    emit(json!({ "type": "ai", "requestId": request_id, "event": event }));
}

/// Stream one model turn, forwarding text deltas. Returns the text and the
/// turn's last event (Done, Error or ToolCalls).
async fn one_turn(provider: Arc<dyn Provider>, req: ChatRequest, request_id: &str) -> (String, ChatEvent) {
    let (tx, mut rx) = mpsc::channel::<ChatEvent>(128);
    let producer = async move {
        if let Err(e) = provider.stream(req, tx.clone()).await {
            let _ = tx.send(ChatEvent::Error(format!("{e:#}"))).await;
        }
    };
    let consumer = async {
        let mut text = String::new();
        while let Some(ev) = rx.recv().await {
            match ev {
                ChatEvent::Delta(t) => {
                    text.push_str(&t);
                    emit_ai(request_id, json!({"type": "delta", "text": t}));
                }
                other => return (text, other),
            }
        }
        (text, ChatEvent::Done)
    };
    let ((), out) = tokio::join!(producer, consumer);
    out
}

/// Run one tool call (asking first when needed). Returns the text for the model.
async fn run_tool(call: &ToolCall, tools: &[ChatTool], request_id: &str) -> String {
    let Some(tool) = tools.iter().find(|t| t.spec.name == call.name) else {
        return format!("Error: there is no tool called `{}`.", call.name);
    };
    let label = format!("{} · {}", tool.server, tool.tool);
    if !tool.auto_approve {
        let (tx, rx) = oneshot::channel();
        APPROVALS
            .lock()
            .unwrap()
            .insert(format!("{request_id}:{}", call.id), tx);
        emit_ai(
            request_id,
            json!({
                "type": "approve",
                "text": label,
                "callId": call.id,
                "arguments": call.arguments,
            }),
        );
        let allowed = tokio::time::timeout(Duration::from_secs(300), rx)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false);
        APPROVALS.lock().unwrap().remove(&format!("{request_id}:{}", call.id));
        if !allowed {
            emit_ai(
                request_id,
                json!({"type": "tool", "text": format!("{label} — declined"), "callId": call.id}),
            );
            return "The user declined this tool call. Do not retry it; answer without it or ask what they want."
                .into();
        }
    }
    emit_ai(request_id, json!({"type": "tool", "text": label, "callId": call.id}));
    match hub().call(&tool.server, &tool.tool, call.arguments.clone()).await {
        Ok(mut out) => {
            if out.chars().count() > MAX_TOOL_OUTPUT {
                out = out.chars().take(MAX_TOOL_OUTPUT).collect::<String>() + "\n[output truncated]";
            }
            if out.trim().is_empty() {
                "(the tool returned no output)".into()
            } else {
                out
            }
        }
        Err(e) => format!("Error: {e:#}"),
    }
}

/// The whole answer: model turns and tool rounds until the model stops.
pub async fn converse(provider: Arc<dyn Provider>, mut req: ChatRequest, tools: Vec<ChatTool>, request_id: String) {
    req.tools = tools.iter().map(|t| t.spec.clone()).collect();
    for _ in 0..MAX_ROUNDS {
        let (text, last) = one_turn(provider.clone(), req.clone(), &request_id).await;
        match last {
            ChatEvent::ToolCalls(calls) if !calls.is_empty() && !tools.is_empty() => {
                req.messages.push(Message::assistant_tool_calls(text, calls.clone()));
                for call in &calls {
                    let out = run_tool(call, &tools, &request_id).await;
                    req.messages.push(Message::tool_result(call, out));
                }
            }
            ChatEvent::Error(e) => {
                emit_ai(&request_id, json!({"type": "error", "text": e}));
                return;
            }
            _ => {
                emit_ai(&request_id, json!({"type": "done"}));
                return;
            }
        }
    }
    emit_ai(
        &request_id,
        json!({"type": "error", "text": format!("Stopped after {MAX_ROUNDS} tool rounds.")}),
    );
}

/// Start tool discovery in the background (startup and Reload).
pub fn refresh_in_background() {
    runtime().spawn(async {
        let (_, errors) = refresh_tools().await;
        for e in errors {
            eprintln!("opencast: {e}");
        }
    });
}
