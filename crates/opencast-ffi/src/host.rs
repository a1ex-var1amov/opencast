use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use opencast_core::engine::Engine;
use opencast_core::Config;
use tokio::runtime::Runtime;

use crate::dispatch;

pub type Callback = extern "C" fn(*const c_char);

static CALLBACK: Mutex<Option<Callback>> = Mutex::new(None);
static ENGINE: OnceLock<Mutex<std::sync::Arc<Engine>>> = OnceLock::new();

pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

pub fn emit(value: serde_json::Value) {
    let Ok(text) = CString::new(value.to_string()) else { return };
    let cb = CALLBACK.lock().unwrap().clone();
    if let Some(cb) = cb {
        cb(text.as_ptr());
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
            _ => with_engine(|engine| dispatch::dispatch(engine, request)),
        }
    }));
    match result {
        Ok(s) => s,
        Err(_) => serde_json::json!({ "error": "internal error" }).to_string(),
    }
}

pub fn install(cb: Callback) {
    *CALLBACK.lock().unwrap() = Some(cb);
    let engine = std::sync::Arc::new(Engine::load(&Config::config_dir()));
    for w in &engine.warnings {
        eprintln!("opencast: {w}");
    }
    ENGINE.set(Mutex::new(engine)).ok();
    let _ = runtime();
    emit(serde_json::json!({ "type": "show" }));
}

pub fn call_raw(json: *const c_char) -> *mut c_char {
    let request = unsafe { CStr::from_ptr(json) }.to_string_lossy();
    CString::new(handle(&request)).unwrap_or_else(|_| CString::new(r#"{"error":"internal error"}"#).unwrap()).into_raw()
}

pub fn free_raw(p: *mut c_char) {
    if !p.is_null() {
        unsafe { drop(CString::from_raw(p)) }
    }
}
