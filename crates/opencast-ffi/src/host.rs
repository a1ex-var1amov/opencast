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
        with_engine(|engine| dispatch::dispatch(engine, request))
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
