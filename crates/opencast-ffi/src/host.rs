use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use opencast_core::engine::Engine;
use opencast_core::providers::{ChatEvent, ChatRequest, Message, Role};
use opencast_core::Config;
use serde::Deserialize;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::dispatch;

pub type Callback = extern "C" fn(*const c_char);

static CALLBACK: Mutex<Option<Callback>> = Mutex::new(None);
static ENGINE: OnceLock<Mutex<std::sync::Arc<Engine>>> = OnceLock::new();
static STREAMS: LazyLock<Mutex<HashMap<String, JoinHandle<()>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static VISIBLE: AtomicBool = AtomicBool::new(false);

pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

pub fn emit(value: serde_json::Value) {
    match value.get("type").and_then(|t| t.as_str()) {
        Some("show") => VISIBLE.store(true, Ordering::SeqCst),
        Some("hide") => VISIBLE.store(false, Ordering::SeqCst),
        _ => {}
    }
    let Ok(text) = CString::new(value.to_string()) else {
        return;
    };
    // Drop the callback mutex before the call so the panel can re-enter.
    let cb = CALLBACK.lock().unwrap().clone();
    if let Some(cb) = cb {
        cb(text.as_ptr());
    }
}

pub(crate) fn toggle_panel() {
    if VISIBLE.load(Ordering::SeqCst) {
        emit(serde_json::json!({ "type": "hide" }));
    } else {
        emit(serde_json::json!({ "type": "show" }));
    }
}

pub fn with_engine<T>(f: impl FnOnce(&Engine) -> T) -> T {
    let guard = ENGINE.get().expect("opencast_start was not called").lock().unwrap();
    f(&guard)
}

pub fn replace_engine(engine: std::sync::Arc<Engine>) {
    *ENGINE.get().expect("opencast_start").lock().unwrap() = engine;
}

fn handle(request: &str) -> String {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let v: serde_json::Value = match serde_json::from_str(request) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
        };
        let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
        match op {
            "execute" => {
                let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let args = v
                    .get("args")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let r = runtime().block_on(crate::exec::run_command(id, args));
                if !matches!(r, crate::exec::ExecResult::Hide) {
                    emit(serde_json::json!({ "type": "show" }));
                }
                serde_json::to_string(&r).unwrap_or_else(|_| r#"{"type":"error","message":"internal error"}"#.into())
            }
            "copy" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
                match crate::platform::write_clipboard(text) {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "paste" => {
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let r = runtime().block_on(async {
                    crate::platform::write_clipboard(&text)?;
                    emit(serde_json::json!({ "type": "hide" }));
                    crate::platform::paste_into_frontmost().await
                });
                match r {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "seekClick" => {
                let x = v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0);
                let y = v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0);
                let r = runtime().block_on(async {
                    emit(serde_json::json!({ "type": "hide" }));
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    crate::window_mgmt::click_at(x, y).map_err(|e| format!("{e:#}"))
                });
                match r {
                    Ok(()) => r#"{"ok":true}"#.into(),
                    Err(e) => serde_json::json!({ "error": e }).to_string(),
                }
            }
            "seekClose" => {
                emit(serde_json::json!({ "type": "hide" }));
                r#"{"ok":true}"#.into()
            }
            "reload" => serde_json::json!({ "text": crate::exec::reload() }).to_string(),
            "stream" => stream(&v),
            "cancel" => cancel(&v),
            _ => with_engine(|engine| dispatch::dispatch(engine, request)),
        }
    }));
    match result {
        Ok(s) => s,
        Err(_) => serde_json::json!({ "error": "internal error" }).to_string(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamArgs {
    request_id: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    system: Option<String>,
    messages: Vec<Message>,
}

/// Start a streamed completion. Tokens arrive as `ai` callbacks tagged with
/// `requestId`; the stream always ends with a `done` or `error` event.
fn stream(v: &serde_json::Value) -> String {
    let args: StreamArgs = match serde_json::from_value(v.clone()) {
        Ok(a) => a,
        Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
    };
    let provider_id = args.provider.clone();
    let provider = with_engine(|engine| {
        engine.router.get(provider_id.as_deref()).ok_or_else(|| {
            if engine.router.is_empty() {
                "No AI provider configured. Add a [[providers]] entry to config.toml (Open Config Folder), then Reload Commands.".to_string()
            } else {
                format!("Unknown provider `{}`", provider_id.unwrap_or_default())
            }
        })
    });
    let provider = match provider {
        Ok(p) => p,
        Err(e) => return serde_json::json!({ "error": e }).to_string(),
    };

    let mut messages = Vec::new();
    if let Some(s) = args.system.filter(|s| !s.trim().is_empty()) {
        messages.push(Message::new(Role::System, s));
    }
    messages.extend(args.messages);
    let req = ChatRequest {
        model: args.model.unwrap_or_default(),
        messages,
        ..Default::default()
    };

    let id = args.request_id;
    let id2 = id.clone();
    let handle = runtime().spawn(async move {
        let (tx, mut rx) = mpsc::channel::<ChatEvent>(128);
        let producer = async move {
            if let Err(e) = provider.stream(req, tx.clone()).await {
                let _ = tx.send(ChatEvent::Error(format!("{e:#}"))).await;
            }
        };
        let consumer = {
            let id = id2.clone();
            async move {
                let mut finished = false;
                while let Some(ev) = rx.recv().await {
                    finished = !matches!(ev, ChatEvent::Delta(_));
                    emit(serde_json::json!({
                        "type": "ai",
                        "requestId": id,
                        "event": ev,
                    }));
                    if finished {
                        break;
                    }
                }
                if !finished {
                    emit(serde_json::json!({
                        "type": "ai",
                        "requestId": id,
                        "event": ChatEvent::Done,
                    }));
                }
            }
        };
        tokio::join!(producer, consumer);
        STREAMS.lock().unwrap().remove(&id2);
    });
    STREAMS.lock().unwrap().insert(id, handle);
    r#"{"ok":true}"#.into()
}

fn cancel(v: &serde_json::Value) -> String {
    let request_id = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if let Some(h) = STREAMS.lock().unwrap().remove(&request_id) {
        h.abort();
    }
    r#"{"ok":true}"#.into()
}

pub fn install(cb: Callback) {
    *CALLBACK.lock().unwrap() = Some(cb);
    let engine = std::sync::Arc::new(Engine::load(&Config::config_dir()));
    for w in &engine.warnings {
        eprintln!("opencast: {w}");
    }
    let hyperkey = engine.config.general.hyperkey;
    ENGINE.set(Mutex::new(std::sync::Arc::clone(&engine))).ok();
    let _ = runtime();
    crate::hotkeys::listen();
    if hyperkey {
        crate::hyperkey::remap_caps_lock(true);
        // The tap callback must return before Swift `hide`. A slow tap is disabled.
        crate::hyperkey::start(|| {
            std::thread::spawn(toggle_panel);
        });
    }
    for p in crate::hotkeys::register(&engine) {
        eprintln!("opencast: {p}");
    }
    emit(serde_json::json!({ "type": "show" }));
}

pub fn call_raw(json: *const c_char) -> *mut c_char {
    let request = unsafe { CStr::from_ptr(json) }.to_string_lossy();
    CString::new(handle(&request))
        .unwrap_or_else(|_| CString::new(r#"{"error":"internal error"}"#).unwrap())
        .into_raw()
}

pub fn free_raw(p: *mut c_char) {
    if !p.is_null() {
        unsafe { drop(CString::from_raw(p)) }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamArgs;

    #[test]
    fn stream_args_omit_optional_fields() {
        let v = serde_json::json!({
            "op": "stream",
            "requestId": "abc",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let args: StreamArgs = serde_json::from_value(v).unwrap();
        assert_eq!(args.request_id, "abc");
        assert!(args.provider.is_none());
        assert!(args.model.is_none());
        assert!(args.system.is_none());
        assert_eq!(args.messages.len(), 1);
    }
}
