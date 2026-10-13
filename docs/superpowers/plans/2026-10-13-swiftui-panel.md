# SwiftUI Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tauri/React panel with a SwiftUI accessory app that links `libopencast_ffi.a`, and make the launcher hug its results, show real app icons, and draw built-in commands with SF Symbols.

**Architecture:** `opencast-core` stays the portable engine. A new macOS crate `opencast-ffi` exports `opencast_start`, `opencast_call`, and `opencast_free`. Swift owns the two `NSPanel`s and calls Rust off the main thread. Rust calls Swift only through the callback. `hide` is the only event Swift finishes before returning.

**Tech Stack:** Rust (existing workspace, edition 2021), Tokio, `global-hotkey` 0.8, `core-graphics` 0.25, SwiftUI on macOS 14, `swiftc`.

## Global Constraints

- Deployment target is macOS 14.0. Bundle id stays `dev.opencast.launcher`. `LSUIElement` is true. The app is not sandboxed.
- `opencast-core` stays free of AppKit, Swift, and Tauri. `cargo test` at the repo root still runs only that crate.
- No new crate besides `global-hotkey` 0.8 (it replaces `tauri-plugin-global-shortcut`). No cbindgen, no Swift package, no Xcode project, no markdown library.
- JSON field names stay camelCase, matching the existing `Command`, `ExecResult`, `ProviderInfo`, `ChatEvent`, and `SeekScan` serde.
- Accent is `#ff6b3d`. Dark island background is `#000000`. Light island background is `#fafafb`. Bottom corner radius is 18. Search field is 19pt. Rows are 44pt. Drop animation is 0.16s. Window level is 25.
- Do not put a vibrancy blur behind the panel.
- `hide` runs to completion on the main thread before the callback returns. Every other callback hops to the main queue asynchronously. Swift never calls `opencast_start` or `opencast_call` on the main thread.
- Rust code is formatted with rustfmt (`max_width = 120`).
- Leave `src-tauri/` and the React app in place until Task 10. Until then both shells build.

## File map

- Create `crates/opencast-ffi/` — C ABI, command execution, hotkeys, moved shell modules.
- Create `macos/` — SwiftUI app, `opencast.h`, `Info.plist`.
- Modify `Cargo.toml` — add the ffi member.
- Modify `crates/opencast-core/src/engine.rs` — two subtitles become `None`.
- Modify `crates/opencast-core/src/lib.rs`, `README.md`, `CLAUDE.md`, `build-app.command` — in Task 10.
- Delete `src/`, `src-tauri/`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `seek.html` — in Task 10, after the new app launches.

---

### Task 1: Drop the repeated subtitles

**Files:**
- Modify: `crates/opencast-core/src/engine.rs` (the window-action push near line 229, the app push near line 364)

**Interfaces:**
- Consumes: nothing new
- Produces: application commands and window actions serialize with `"subtitle": null`. Command ids are unchanged (`app:<path>`, `window:<key>`).

- [ ] **Step 1: Change the two subtitle fields**

In the window-action loop, replace `subtitle: Some("Move the front window".into()),` with `subtitle: None,`.

In the app loop, replace `subtitle: Some("Application".into()),` with `subtitle: None,`.

- [ ] **Step 2: Run the core tests**

Run: `cargo test -p opencast-core`

Expected: PASS. No current test asserts those two strings. If one fails, update that assertion to `None` and re-run.

- [ ] **Step 3: Commit**

```bash
git add crates/opencast-core/src/engine.rs
git commit -m "$(cat <<'EOF'
Drop subtitles that repeat the row kind.

Application and window rows already say what they are in the title and the kind label.
EOF
)"
```

---

### Task 2: JSON dispatch for the read-only ops

**Files:**
- Create: `crates/opencast-ffi/Cargo.toml`
- Create: `crates/opencast-ffi/src/lib.rs`
- Create: `crates/opencast-ffi/src/dispatch.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `Engine::assemble`, `Engine::search`, `Engine::warnings`, `Router::list` (via `engine.router`)
- Produces: `pub fn dispatch(engine: &Engine, request: &str) -> String` in `opencast-ffi`. Handles `search`, `warnings`, `providers`. Any other `op`, and any invalid JSON, returns `{"error":"..."}`. Search uses limit 60.

- [ ] **Step 1: Add the crate to the workspace**

In the root `Cargo.toml`, change members to:

```toml
members = ["crates/opencast-core", "crates/opencast-ffi", "src-tauri"]
```

Leave `default-members = ["crates/opencast-core"]`.

Create `crates/opencast-ffi/Cargo.toml`:

```toml
[package]
name = "opencast-ffi"
description = "macOS C ABI in front of the OpenCast engine"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["staticlib", "rlib"]

[dependencies]
opencast-core = { path = "../opencast-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Write the failing test**

Create `crates/opencast-ffi/src/lib.rs`:

```rust
//! C ABI for the SwiftUI panel. Non-macOS builds compile an empty stub so
//! `cargo test --workspace` still configures.

#[cfg(target_os = "macos")]
mod dispatch;

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn _stub() {}
```

Create `crates/opencast-ffi/src/dispatch.rs` with only the test, so the function is missing:

```rust
use opencast_core::apps::AppEntry;
use opencast_core::engine::Engine;
use opencast_core::Config;
use std::path::PathBuf;

#[cfg(test)]
fn fixture() -> Engine {
    Engine::assemble(
        Config::default(),
        PathBuf::from("/tmp/opencast-test"),
        vec![AppEntry {
            name: "Safari".into(),
            path: PathBuf::from("/Applications/Safari.app"),
        }],
        vec![],
        vec![],
        vec![],
    )
}

#[test]
fn search_op_returns_commands() {
    let engine = fixture();
    let out = dispatch(&engine, r#"{"op":"search","query":""}"#);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let commands = v["commands"].as_array().expect("commands array");
    assert!(commands.iter().any(|c| c["title"] == "Safari"));
    assert!(commands.iter().any(|c| c["id"] == "app:/Applications/Safari.app" && c["subtitle"].is_null()));
    let bad = dispatch(&engine, r#"{"op":"nope"}"#);
    assert!(serde_json::from_str::<serde_json::Value>(&bad).unwrap()["error"].is_string());
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p opencast-ffi --lib search_op_returns_commands`

Expected: FAIL to compile, `dispatch` is not defined.

- [ ] **Step 4: Implement dispatch**

Add this above the test in `crates/opencast-ffi/src/dispatch.rs`:

```rust
const SEARCH_LIMIT: usize = 60;

pub fn dispatch(engine: &Engine, request: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({ "error": format!("bad json: {e}") }).to_string(),
    };
    let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
    let out = match op {
        "search" => {
            let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
            serde_json::json!({ "commands": engine.search(q, SEARCH_LIMIT) })
        }
        "warnings" => serde_json::json!({ "warnings": engine.warnings }),
        "providers" => serde_json::json!({ "providers": engine.router.list() }),
        _ => serde_json::json!({ "error": format!("unknown op `{op}`") }),
    };
    out.to_string()
}
```

`Engine.warnings` and `Engine.router` are pub fields. `Router::list` is pub.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p opencast-ffi --lib search_op_returns_commands`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/opencast-ffi
git commit -m "$(cat <<'EOF'
Add a JSON dispatch for search, warnings, and providers.

The Swift panel will call this through the C ABI. The test locks the camelCase command shape.
EOF
)"
```

---

### Task 3: C ABI, callback, and startup

**Files:**
- Create: `crates/opencast-ffi/src/host.rs`
- Modify: `crates/opencast-ffi/src/lib.rs`
- Modify: `crates/opencast-ffi/Cargo.toml` (tokio)

**Interfaces:**
- Consumes: `dispatch` from Task 2
- Produces:
  - `pub extern "C" fn opencast_start(cb: extern "C" fn(*const c_char))`
  - `pub extern "C" fn opencast_call(json: *const c_char) -> *mut c_char`
  - `pub extern "C" fn opencast_free(p: *mut c_char)`
  - `pub(crate) fn emit(value: serde_json::Value)` — copies the callback out of the mutex before invoking it
  - `pub(crate) fn runtime() -> &'static tokio::runtime::Runtime`
  - `pub(crate) fn with_engine<T>(f: impl FnOnce(&Engine) -> T) -> T`
  - On start: load `Engine::load(&Config::config_dir())`, print each warning to stderr, emit `{"type":"show"}`. `opencast_call` routes `search`, `warnings`, and `providers` through `dispatch`. A panic in `opencast_call` becomes `{"error":"internal error"}`.

- [ ] **Step 1: Add Tokio**

In `crates/opencast-ffi/Cargo.toml` dependencies, add:

```toml
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros", "time"] }
```

- [ ] **Step 2: Write host.rs**

```rust
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
```

`Callback` is a function pointer, which is `Copy`. `clone()` on `Option<Callback>` copies it, so the mutex is released before Swift runs. That is required so a `hide` callback can re-enter `opencast_call` without deadlocking. Do not hold `CALLBACK` or `ENGINE` across `emit`.

- [ ] **Step 3: Export the three C functions**

Replace `crates/opencast-ffi/src/lib.rs` with:

```rust
//! C ABI for the SwiftUI panel. Non-macOS builds compile an empty stub so
//! `cargo test --workspace` still configures.

#[cfg(target_os = "macos")]
mod dispatch;
#[cfg(target_os = "macos")]
mod host;

#[cfg(target_os = "macos")]
pub use dispatch::dispatch;

#[cfg(target_os = "macos")]
use std::os::raw::c_char;

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_start(cb: host::Callback) {
    host::install(cb);
}

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_call(json: *const c_char) -> *mut c_char {
    host::call_raw(json)
}

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_free(p: *mut c_char) {
    host::free_raw(p);
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn _stub() {}
```

- [ ] **Step 4: Keep the Task 2 test compiling**

`dispatch.rs` is now a module. Its `pub fn dispatch` is what `host` calls. The unit test does not call `opencast_start`. Run:

`cargo test -p opencast-ffi --lib`

Expected: PASS (`search_op_returns_commands` still green). `cargo build -p opencast-ffi` produces `target/debug/libopencast_ffi.a`.

- [ ] **Step 5: Commit**

```bash
git add crates/opencast-ffi
git commit -m "$(cat <<'EOF'
Export the OpenCast C ABI and load the engine on start.

The callback is invoked without holding the host lock, so a synchronous hide cannot deadlock.
EOF
)"
```

---

### Task 4: Move the macOS glue off Tauri

**Files:**
- Create: `crates/opencast-ffi/src/platform.rs` (from `src-tauri/src/platform.rs`)
- Create: `crates/opencast-ffi/src/window_mgmt.rs` (from `src-tauri/src/window_mgmt.rs`)
- Create: `crates/opencast-ffi/src/hyperkey.rs` (from `src-tauri/src/hyperkey.rs`)
- Modify: `crates/opencast-ffi/Cargo.toml`
- Modify: `crates/opencast-ffi/src/lib.rs` (declare the modules)

**Interfaces:**
- Consumes: `host::emit`
- Produces:
  - `platform::write_clipboard(text: &str) -> Result<(), String>`
  - `platform::grab_selection() -> impl Future<Output = String>` — emits `{"type":"hide"}` then waits 180ms, same clipboard dance as today
  - `platform::paste_into_frontmost() -> impl Future<Output = Result<(), String>>`
  - `window_mgmt::apply_window_action(action: WindowAction) -> impl Future<Output = Result<()>>`
  - `window_mgmt::seek_scan() -> impl Future<Output = Result<SeekScan>>`
  - `window_mgmt::click_at(x: f64, y: f64) -> Result<()>`
  - `window_mgmt::remember_layout`, `restore_layout`, `forget_layout` — signatures unchanged
  - `hyperkey::remap_caps_lock(on: bool)`, `hyperkey::start(on_tap: impl Fn() + Send + 'static)`
  - `pub(crate) fn monitor_containing(rects: &[Rect], x: f64, y: f64) -> Option<Rect>` — pure, tested

- [ ] **Step 1: Add the shell dependencies**

```toml
anyhow = "1"
arboard = "3"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = "0.25"
global-hotkey = "0.8"
```

`global-hotkey` is used in Task 6. Adding it now keeps the crate file stable.

- [ ] **Step 2: Copy hyperkey unchanged**

```bash
cp src-tauri/src/hyperkey.rs crates/opencast-ffi/src/hyperkey.rs
```

Declare `#[cfg(target_os = "macos")] mod hyperkey;` in `lib.rs`. Do not call it yet.

- [ ] **Step 3: Copy platform.rs and drop Tauri**

```bash
cp src-tauri/src/platform.rs crates/opencast-ffi/src/platform.rs
```

Delete `use tauri::AppHandle;`. Delete the `#[cfg(target_os = "linux")]` and `#[cfg(not(any(...)))]` blocks inside `grab_selection` and `paste_into_frontmost`. Keep the macOS bodies. Change `grab_selection` to:

```rust
pub async fn grab_selection() -> String {
    crate::host::emit(serde_json::json!({ "type": "hide" }));
    tokio::time::sleep(std::time::Duration::from_millis(180)).await;
    let saved = read_clipboard();
    let marker = format!("\u{2063}opencast-{}", std::process::id());
    let _ = write_clipboard(&marker);
    if keystroke("c").is_err() {
        if let Some(s) = &saved {
            let _ = write_clipboard(s);
        }
        return String::new();
    }
    let mut got = String::new();
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        if let Some(t) = read_clipboard() {
            if t != marker {
                got = t;
                break;
            }
        }
    }
    match saved {
        Some(s) => {
            let _ = write_clipboard(&s);
        }
        None => {
            let _ = write_clipboard("");
        }
    }
    got
}
```

`paste_into_frontmost` keeps its 180ms sleep and `keystroke("v")` and drops the unused `app` concerns. It takes no arguments.

- [ ] **Step 4: Write the failing monitor test, then the function**

At the top of `window_mgmt.rs` (after the `Rect` struct, which already exists in the copied file), add:

```rust
pub fn monitor_containing(rects: &[Rect], x: f64, y: f64) -> Option<Rect> {
    rects
        .iter()
        .find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
        .or(rects.first())
        .copied()
}

#[cfg(test)]
mod monitor_tests {
    use super::*;

    #[test]
    fn picks_the_display_under_the_point_else_the_first() {
        let rects = [
            Rect { x: 0.0, y: 0.0, w: 1000.0, h: 800.0 },
            Rect { x: 1000.0, y: 0.0, w: 800.0, h: 600.0 },
        ];
        assert_eq!(monitor_containing(&rects, 1200.0, 10.0).unwrap().x, 1000.0);
        assert_eq!(monitor_containing(&rects, -5.0, -5.0).unwrap().x, 0.0);
        assert!(monitor_containing(&[], 0.0, 0.0).is_none());
    }
}
```

Write the test into the copied file first and run `cargo test -p opencast-ffi --lib picks_the_display` to see it fail if the function is not there yet, then add the function. Expected after the function: PASS.

- [ ] **Step 5: Replace Tauri monitor lookups**

Copy `src-tauri/src/window_mgmt.rs` to `crates/opencast-ffi/src/window_mgmt.rs` before adding the test (if you have not already). Then:

- Delete `use tauri::AppHandle;`.
- Delete the whole `#[cfg(not(target_os = "macos"))] mod imp { ... }` block and the `#[cfg]` on `use mac as imp;`. The public wrappers call `mac::` directly.
- Change `monitor_at` to take no `AppHandle`. It lists `CGDisplay::active_displays()`, maps each id through `CGDisplay::new(id).bounds()` into a `Rect { x: bounds.origin.x, y: bounds.origin.y, w: bounds.size.width, h: bounds.size.height }`, and returns `monitor_containing(&rects, x, y).ok_or_else(|| anyhow!("no monitors found"))`.
- Change `cursor_monitor` to take no `AppHandle`. Build a `CGEvent` from `CGEventSource::new(CGEventSourceStateID::CombinedSessionState)`, read `location()`, and return `monitor_at(point.x, point.y)`. This is the full display bounds in Core Graphics space (origin at the top-left of the primary display, y down), which is the space `screencapture -R` and `click_at` already use. Do not inset the menu bar here. The panel is at window level 25, so it is not clamped under the menu bar. Window snapping still subtracts `front.menu` from the JXA reply inside `apply_window_action`.
- Drop the `app` parameter from `mac::apply_window_action`, `mac::seek_scan`, and the public wrappers. Update the two call sites inside those functions (`monitor_at(...)`, `cursor_monitor()`).

Declare the three modules in `lib.rs` under `#[cfg(target_os = "macos")]`.

- [ ] **Step 6: Build and test**

Run: `cargo test -p opencast-ffi --lib`

Expected: PASS, including `picks_the_display_under_the_point_else_the_first` and `search_op_returns_commands`.

- [ ] **Step 7: Commit**

```bash
git add crates/opencast-ffi
git commit -m "$(cat <<'EOF'
Move clipboard, snapping, and the hyper key out of Tauri.

Monitor frames come from Core Graphics. Selection grab hides the panel through the callback.
EOF
)"
```

---

### Task 5: Execute, paste, and Seek clicks

**Files:**
- Create: `crates/opencast-ffi/src/exec.rs`
- Modify: `crates/opencast-ffi/src/host.rs` (`handle` learns the new ops)
- Modify: `crates/opencast-ffi/src/lib.rs` (`mod exec;`)

**Interfaces:**
- Consumes: `host::emit`, `host::with_engine`, `host::replace_engine`, `host::runtime`, `platform::*`, `window_mgmt::*`, `Engine::plan`, `Plan`
- Produces: `pub async fn run_command(id: String, args: Vec<String>) -> ExecResult`. `opencast_call` ops `execute`, `copy`, `paste`, `seekClick`, `seekClose`, `reload` return the JSON from the spec. `execute` blocks on the runtime from the Swift caller thread (never from inside a spawned task). When the result is not `Hide`, `execute` emits `{"type":"show"}` before returning, matching today's `execute` tauri command. `reload` returns `{"text":"Reloaded · N commands"}` or the warning variant from `main.rs`.

- [ ] **Step 1: Define ExecResult and run_command**

Create `crates/opencast-ffi/src/exec.rs`. Port `ExecResult`, `err`, and `run_command` from `src-tauri/src/main.rs`. Differences from that file:

- No `AppHandle`. `hide` is `host::emit(json!({"type":"hide"}))`. `show` is `host::emit(json!({"type":"show"}))`.
- `Plan::Window` emits hide, sleeps inside `apply_window_action` (that sleep stays in window_mgmt), then calls `window_mgmt::apply_window_action(action).await`.
- `Plan::Seek` emits hide, then on a successful `window_mgmt::seek_scan().await` emits `{"type":"seek","scan": scan}` and returns `ExecResult::Hide`.
- `Plan::Chat` calls `platform::grab_selection().await` when `needs_selection` is true, then emits show, then fills the prompt. Empty selection returns the same error string as `main.rs` (`No text selected...`).
- `Builtin::Quit` calls `hyperkey::remap_caps_lock(false)` when `engine.config.general.hyperkey` is set, then `std::process::exit(0)`.
- `Builtin::Reload` calls `reload()` defined below.
- Layout remember/restore/forget call the `window_mgmt` functions with `engine.config_dir`. Remember and forget call `reload()` so the registry picks up the new commands.
- Script and copy branches stay as they are in `main.rs`, including the silent/compact/full output modes.

```rust
pub fn reload() -> String {
    let engine = std::sync::Arc::new(opencast_core::engine::Engine::load(
        &opencast_core::Config::config_dir(),
    ));
    let n = engine.registry.len();
    let w = engine.warnings.len();
    crate::host::replace_engine(engine);
    if w == 0 {
        format!("Reloaded · {n} commands")
    } else {
        format!("Reloaded · {n} commands · {w} warning(s), see Open Config Folder")
    }
}
```

`ExecResult` keeps the serde attributes from `main.rs`:

```rust
#[derive(Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ExecResult {
    Hide,
    Toast { text: String },
    Output { title: String, text: String },
    Chat {
        prompt: Option<String>,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        auto_send: bool,
    },
    Error { message: String },
    Args { command: opencast_core::Command },
}
```

- [ ] **Step 2: Route the ops in handle**

In `host.rs`, replace `handle` so it matches `op` before falling through to `dispatch`. Pseudocode is not enough; use this:

```rust
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
```

`seekClose` emits `hide` rather than a separate close event. Swift treats `hide` as "order out the launcher and, when Seek is up, order out Seek too, then `NSApp.hide`". The click path's extra hide is harmless if Seek is already gone.

Do not call `runtime().block_on` from a task already running on that runtime. `opencast_call` is the only caller of `handle`, and Swift calls it off the runtime.

- [ ] **Step 3: Build**

Run: `cargo test -p opencast-ffi --lib`

Expected: PASS. Fix compile errors from the port (missing `apps::`, `scripts::`, `fill_selection`, `OutputMode`) by using the same `opencast_core` paths `main.rs` uses. Do not change behavior to silence a warning.

- [ ] **Step 4: Commit**

```bash
git add crates/opencast-ffi
git commit -m "$(cat <<'EOF'
Run launcher commands through the C ABI.

Execute, paste, and Seek clicks hide the panel via the callback before they touch the front app.
EOF
)"
```

---

### Task 6: Streaming and hotkeys

**Files:**
- Create: `crates/opencast-ffi/src/hotkeys.rs`
- Modify: `crates/opencast-ffi/src/exec.rs` (`reload` re-registers hotkeys)
- Modify: `crates/opencast-ffi/src/host.rs` (`stream` and `cancel` ops, start registers hotkeys and the hyper key)

**Interfaces:**
- Consumes: `global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, HotKeyState, hotkey::HotKey}`, `Engine::hotkey_bindings`, `engine.config.general.hotkey`, `engine.config.general.hyperkey`
- Produces:
  - `hotkeys::register(engine: &Engine) -> Vec<String>` — problems, never a hard failure. Toggle falls back to `Alt+Space` when the configured string does not parse. `Hyper+` is rewritten to `Ctrl+Alt+Cmd+Shift+` before parsing. Duplicate bindings are skipped with a warning, same text as `register_hotkeys` in `main.rs`.
  - A hotkey press spawns `run_command` on the runtime. If the command has a required argument, the result is `ExecResult::Args` without running it (same as `dispatch_hotkey`). If the result is not `Hide`, emit `show`, then emit `{"type":"result","result": <ExecResult>}`.
  - `stream` starts the same producer/consumer as `ai_stream` in `main.rs`. Events are `{"type":"ai","requestId":...,"event":...}` with `ChatEvent`'s existing serde. A stream that ends without `done` or `error` emits `done`. `cancel` aborts that task.
  - `install` calls `hotkeys::register` and, when `hyperkey` is true, `hyperkey::remap_caps_lock(true)` and `hyperkey::start` with a closure that emits `show` when the launcher is not visible and `hide` when it is. Track visibility with an `AtomicBool` that `emit` of `show`/`hide` updates. The hyper tap toggles that flag.

- [ ] **Step 1: Register hotkeys**

`HotKey` parses the same strings the Tauri plugin accepted (`"Alt+Space"`, `"Ctrl+Alt+Left"`, `"F18"`), because that plugin uses this crate's parser.

```rust
use std::sync::Mutex;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

static MANAGER: Mutex<Option<GlobalHotKeyManager>> = Mutex::new(None);
static TOGGLE: Mutex<Option<HotKey>> = Mutex::new(None);
static BINDS: Mutex<Vec<(HotKey, String)>> = Mutex::new(Vec::new());

pub fn register(engine: &opencast_core::engine::Engine) -> Vec<String> {
    let manager = GlobalHotKeyManager::new();
    let mut problems = Vec::new();
    let Ok(manager) = manager else {
        problems.push("could not start the hotkey manager".into());
        return problems;
    };
    if let Some(prev) = MANAGER.lock().unwrap().as_ref() {
        let mut keys = BINDS.lock().unwrap().iter().map(|(k, _)| *k).collect::<Vec<_>>();
        if let Some(t) = *TOGGLE.lock().unwrap() {
            keys.push(t);
        }
        let _ = prev.unregister_all(&keys);
    }

    let spec = engine.config.general.hotkey.as_str();
    let toggle = match spec.parse::<HotKey>() {
        Ok(s) => Some(s),
        Err(e) => {
            problems.push(format!("hotkey `{spec}` is not valid ({e}); using Alt+Space"));
            "Alt+Space".parse::<HotKey>().ok()
        }
    };
    if let Some(s) = toggle {
        if let Err(e) = manager.register(s) {
            problems.push(format!("could not register hotkey `{spec}`: {e}"));
        }
    }

    let (bindings, warns) = engine.hotkey_bindings();
    problems.extend(warns);
    let mut registered = Vec::new();
    for (key, id) in bindings {
        let spec = key.replace("Hyper+", "Ctrl+Alt+Cmd+Shift+");
        let shortcut = match spec.parse::<HotKey>() {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("hotkey `{key}` is not valid ({e})"));
                continue;
            }
        };
        if toggle == Some(shortcut) || registered.iter().any(|(s, _)| *s == shortcut) {
            problems.push(format!("hotkey `{key}` is bound twice; keeping the first binding"));
            continue;
        }
        match manager.register(shortcut) {
            Ok(()) => registered.push((shortcut, id)),
            Err(e) => problems.push(format!("could not register hotkey `{key}`: {e}")),
        }
    }
    *TOGGLE.lock().unwrap() = toggle;
    *BINDS.lock().unwrap() = registered;
    *MANAGER.lock().unwrap() = Some(manager);
    problems
}
```

Start the listener once, from `install`, before `register`:

```rust
std::thread::spawn(|| {
    let rx = GlobalHotKeyEvent::receiver();
    for event in rx.iter() {
        if event.state != HotKeyState::Pressed {
            continue;
        }
        let id = event.id;
        if TOGGLE.lock().unwrap().is_some_and(|t| t.id == id) {
            if VISIBLE.load(Ordering::SeqCst) {
                emit(serde_json::json!({ "type": "hide" }));
            } else {
                emit(serde_json::json!({ "type": "show" }));
            }
            continue;
        }
        let cmd = BINDS.lock().unwrap().iter().find(|(k, _)| k.id == id).map(|(_, c)| c.clone());
        if let Some(cmd) = cmd {
            runtime().spawn(async move {
                let needs_args = with_engine(|engine| {
                    engine
                        .registry
                        .get(&cmd)
                        .is_some_and(|c| c.arguments.iter().any(|a| !a.optional))
                });
                let result = if needs_args {
                    let command = with_engine(|engine| engine.registry.get(&cmd).unwrap().clone());
                    ExecResult::Args { command }
                } else {
                    run_command(cmd, vec![]).await
                };
                if !matches!(result, ExecResult::Hide) {
                    emit(serde_json::json!({ "type": "show" }));
                    emit(serde_json::json!({ "type": "result", "result": result }));
                }
            });
        }
    }
});
```

`VISIBLE` is `static VISIBLE: AtomicBool`. Set it to `true` inside `emit` when `value["type"] == "show"`, and `false` when it is `"hide"`, before invoking the callback.

Confirm `Registry::get` exists (it does; `hotkey_bindings` calls it). If `get` returns a reference, clone the `Command` while the engine lock is held, then drop the lock before `run_command`.

- [ ] **Step 2: Stream and cancel**

Port `ai_stream` / `ai_cancel` from `main.rs` into `host.rs`. Store `HashMap<String, tokio::task::JoinHandle<()>>` in a `Mutex`. Replace `app.emit("ai://event", ...)` with:

```rust
emit(serde_json::json!({
    "type": "ai",
    "requestId": id,
    "event": ev,
}));
```

`ChatEvent` already serializes as `{"type":"delta","text":"..."}`, `{"type":"done"}`, or `{"type":"error","text":"..."}`.

`stream` returns `{"ok":true}` or `{"error":"<the same sentences main.rs uses for a missing or unknown provider>"}`. `cancel` aborts the handle and returns `{"ok":true}`.

Call `hotkeys::register` at the end of `install`, print each problem with `eprintln!("opencast: {p}")`, and start the hyper key when `engine.config.general.hyperkey` is true. The tap closure toggles `VISIBLE` the same way the toggle hotkey does.

`exec::reload` calls `hotkeys::register(&engine)` and prints problems before `replace_engine`.

- [ ] **Step 3: Build**

Run: `cargo test -p opencast-ffi --lib && cargo build -p opencast-ffi`

Expected: tests PASS, staticlib builds.

- [ ] **Step 4: Commit**

```bash
git add crates/opencast-ffi
git commit -m "$(cat <<'EOF'
Register global hotkeys and stream AI tokens over the callback.

Hyper+ bindings still mean all four modifiers. A stream always ends with done or error.
EOF
)"
```

---

### Task 7: Swift panel, search list, icons

**Files:**
- Create: `macos/opencast.h`
- Create: `macos/Bridge.swift`
- Create: `macos/Theme.swift`
- Create: `macos/Models.swift`
- Create: `macos/LauncherView.swift`
- Create: `macos/OpenCastApp.swift`
- Create: `macos/Info.plist`

**Interfaces:**
- Consumes: the three C functions and the callback JSON from Tasks 3–6
- Produces: a debug app at `target/debug/OpenCast.app` that opens the island, searches, and runs a command that returns `hide` or `toast`. Args, output, chat, and Seek are stubs that show the title until Tasks 8 and 9.

- [ ] **Step 1: Header and bridge**

`macos/opencast.h`:

```c
#pragma once
typedef void (*opencast_callback)(const char *json);
void opencast_start(opencast_callback cb);
char *opencast_call(const char *json);
void opencast_free(char *p);
```

`macos/Bridge.swift` decodes events and calls Rust off the main thread. The C callback is a top-level function. On `type == "hide"`, if `Thread.isMainThread` run the hide inline, otherwise `DispatchQueue.main.sync`. Every other event is `DispatchQueue.main.async`. `Engine.call` uses `Task.detached` so it is never on the main thread:

```swift
import Foundation

func opencastCall(_ json: String) -> String {
    json.withCString { ptr in
        guard let out = opencast_call(ptr) else { return #"{"error":"null"}"# }
        defer { opencast_free(out) }
        return String(cString: out)
    }
}

enum Engine {
    static func call(_ body: [String: Any]) -> [String: Any] {
        let data = try! JSONSerialization.data(withJSONObject: body)
        let text = String(data: data, encoding: .utf8)!
        let raw = opencastCall(text)
        let parsed = try? JSONSerialization.jsonObject(with: Data(raw.utf8))
        return parsed as? [String: Any] ?? ["error": raw]
    }
}
```

`OpenCastApp.swift` holds `@MainActor @Observable final class Model` and the `NSPanel` subclass. `start` is called from a detached task with this callback:

```swift
let callback: opencast_callback = { ptr in
    guard let ptr else { return }
    let raw = String(cString: ptr)
    let hide = (try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any])?["type"] as? String == "hide"
    let deliver = { Model.shared.accept(raw) }
    if hide {
        if Thread.isMainThread { deliver() } else { DispatchQueue.main.sync(execute: deliver) }
    } else {
        DispatchQueue.main.async(execute: deliver)
    }
}
```

`Model.accept` switches on `type`: `show` orders the launcher front and focuses the field; `hide` orders both panels out and calls `NSApp.hide(nil)` unless Seek is visible and this hide came from a selection grab that will show again (do not special-case that: selection grab's hide is followed by `show`, and `NSApp.hide` then `activate` is correct); `result` applies `ExecResult`; `ai` and `seek` are stored for later tasks.

`show` reads `OPENCAST_KEEP_ON_BLUR` once into `keepOnBlur`. On resign-key, if `!keepOnBlur`, perform the same hide. While Seek is visible, resign-key on the launcher does not hide the app.

Panel: borderless `NSPanel`, `isOpaque = false`, `backgroundColor = .clear`, `level = NSWindow.Level(rawValue: 25)`, `canBecomeKey` and `canBecomeMain` return true, `collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`. On show, `NSApp.activate(ignoringOtherApps: true)` and `makeKeyAndOrderFront`. Place it on the `NSScreen` whose frame contains `NSEvent.mouseLocation` (else `NSScreen.main`). Width 760. AppKit origin is bottom-left, so the panel's top is `screen.frame.maxY` and its origin y is `screen.frame.maxY - height`. First appearance animates from `y = screen.frame.maxY` down, duration 0.16 via `NSAnimationContext`. Later height changes use the same duration.

Search height: `58 + 12 + CGFloat(min(max(rows, 1), 8)) * 44 + 38`, where `rows` includes the Ask AI row when the query is non-empty. Args/output/chat use 480 until those views exist; for this task only search is real, so use the search height.

- [ ] **Step 2: Models, theme, icons**

`Models.swift` decodes `Command` (`id`, `title`, `subtitle`, `icon`, `kind`, `arguments` with `placeholder`, `optional`, `secure`). Kind strings: `builtin`, `ai`, `script`, `quicklink`, `app`, `calculator`, `window`, `layout`. Kind labels: Application, AI Command, Script, Quicklink, Command, Calculator, Window, Layout.

`Theme.swift` holds the island colors, the accent `Color(red: 1, green: 0x6b/255, blue: 0x3d/255)`, and `func symbol(for command: Command) -> String?`. Resolve in this order. `window:center-third` returns nil so the row draws U+2153. Then match command id for the other two thirds. Then match the icon string's Unicode scalars. `NSImage(systemSymbolName:accessibilityDescription:)` nil means the caller draws `command.icon` as text.

| match | symbol |
| --- | --- |
| id `window:first-third` | `rectangle.leadingthird.inset.filled` |
| id `window:last-third` | `rectangle.trailingthird.inset.filled` |
| id `window:center-third` | none |
| U+1F4AC | `bubble.left` |
| U+1F504 | `arrow.clockwise` |
| U+2699 U+FE0F | `gearshape` |
| U+1F4C2 | `folder` |
| U+23FB | `power` |
| U+25E7 | `rectangle.lefthalf.filled` |
| U+25E8 | `rectangle.righthalf.filled` |
| U+2B12 | `rectangle.tophalf.filled` |
| U+2B13 | `rectangle.bottomhalf.filled` |
| U+25F0 | `rectangle.inset.topleft.filled` |
| U+25F3 | `rectangle.inset.topright.filled` |
| U+25F1 | `rectangle.inset.bottomleft.filled` |
| U+25F2 | `rectangle.inset.bottomright.filled` |
| U+26F6 | `arrow.up.left.and.arrow.down.right` |
| U+25CE | `circle.circle` |
| U+1F3AF | `viewfinder` |
| U+1F4F8 | `camera` |
| U+1F5C2 | `square.stack` |
| U+1F5D1 | `trash` |
| U+2728 | `sparkles` |
| U+1F4DC | `scroll` |
| U+1F517 | `link` |

App icons: if `kind == "app"`, the path is `String(id.dropFirst(4))` (`app:` is four characters). `NSWorkspace.shared.icon(forFile:)` at 24pt. Cache in a dictionary on `Model`.

- [ ] **Step 3: Search view**

`LauncherView` is a `VStack` with spacing 0: field, list, footer. Background is the island color, clip shape `UnevenRoundedRectangle(bottomLeadingRadius: 18, bottomTrailingRadius: 18)`. Overlay a 0.5pt stroke, black at 18% in light and white at 14% in dark. Shadow: radius 30, y 22, black 28% light / 55% dark.

Field: `.textFieldStyle(.plain)`, 19pt, no focus ring (`.focusEffectDisabled()` on macOS 14). Placeholder `Search apps and commands, or ask AI…`. A 16pt `ProgressView` when `busy`.

List: `ScrollView` + `LazyVStack`, overlay scroller (the system overlay style; do not draw a custom one). Row height 44, horizontal padding 10, corner radius 9. Selected row fill is accent at 12% when `colorScheme == .light`, else 18%. Icon 24pt. Title, subtitle if non-nil, kind label 12pt in tertiary color. The Ask AI row is last when the trimmed query is non-empty: symbol `sparkles` tinted accent, title `Ask AI “query”`, kind label `Tab`.

Keys: down / Ctrl-N and up / Ctrl-P move the selection and wrap. Return runs. Tab sets mode to chat with that query and `autoSend: true` (Task 8 paints it; for now set the mode). Esc clears the query, or hides when the query is empty. Mouse move selects. Click runs.

Running a command: `Task.detached` calls `Engine.call(["op":"execute","id":id,"args":[]])`, then on the main actor applies the result. `hide` clears the query and calls `Model.hide()`. `toast` shows the text for 1.3s (4s if it was `error`) and then hides. `error` toasts the message for 4s and stays. `args`, `output`, and `chat` switch mode (views land in Task 8).

Footer: `OpenCast` on the left in tertiary 12pt, height 38. Hints on the right: the verb (`Open` for an app, `Copy` for a calculator, `Continue` when the command has arguments, `Ask AI` for the ask row, otherwise `Run`) plus `Return`. When the current row is not Ask AI, also `Ask AI` / `Tab`.

On appear, and on every `show`, focus the field and select its text. Load warnings once: `Engine.call(["op":"warnings"])` and toast the first one as an error, appending ` (+N more)` when there are more.

Empty list copy: `No results`.

- [ ] **Step 4: Info.plist and a debug bundle**

`macos/Info.plist`: `CFBundleIdentifier` = `dev.opencast.launcher`, `CFBundleName` = `OpenCast`, `LSUIElement` = true, `LSMinimumSystemVersion` = `14.0`, `NSHighResolutionCapable` = true, `NSScreenCaptureUsageDescription` = `OpenCast reads the screen to find the text you typed.`, `NSAppleEventsUsageDescription` = `OpenCast controls other apps to snap windows, copy the selection, and paste answers.`, `CFBundleIconFile` = `AppIcon`.

Build:

```bash
cargo build -p opencast-ffi
FLAGS=$(cargo rustc -p opencast-ffi -- --print native-static-libs 2>&1 | awk -F'native-static-libs: ' '/native-static-libs:/ { print $2; exit }')
APP=target/debug/OpenCast.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp src-tauri/icons/icon.icns "$APP/Contents/Resources/AppIcon.icns"
eval swiftc -parse-as-library -g -target arm64-apple-macos14.0 \
  -import-objc-header macos/opencast.h \
  -framework AppKit -framework SwiftUI \
  macos/*.swift \
  -L target/debug -lopencast_ffi $FLAGS \
  -o "$APP/Contents/MacOS/OpenCast"
```

Expected: the compiler exits 0. Launch with `open target/debug/OpenCast.app`. The island appears at the top of the screen. Typing filters. An application row shows that app's icon, not a colored letter. Return on an app hides the panel and opens it. Esc hides. Option-Space (the default hotkey) toggles it.

- [ ] **Step 5: Commit**

```bash
git add macos
git commit -m "$(cat <<'EOF'
Open the launcher as a SwiftUI panel.

Search hugs the result list, application rows use the bundle icon, and built-in commands use SF Symbols.
EOF
)"
```

---

### Task 8: Arguments, output, and chat

**Files:**
- Create: `macos/ChatView.swift`
- Modify: `macos/LauncherView.swift`
- Modify: `macos/OpenCastApp.swift` (mode switches, height 480 for these modes)

**Interfaces:**
- Consumes: `ExecResult` variants `args`, `output`, `chat`, and the `ai` callback. `Engine.call` ops `providers`, `stream`, `cancel`, `copy`, `paste`.
- Produces: the three modes from the spec. Chat markdown is `AttributedString` after fenced blocks are split out. Each fence has a Copy button.

- [ ] **Step 1: Args and output**

Args: back chevron `chevron.left` in a 26pt rounded rect, command title, one field per argument. `secure` uses `SecureField`. Return submits `execute` with the string array. A missing required field focuses that field and does not submit. Esc, or Backspace when the first field is empty, returns to search.

Output: same back button and the title. Selectable monospaced text, 13pt. Empty text shows `(no output)`. Return or Cmd-C copies via `copy` and toasts `Copied output` for 1.3s. Esc returns to search.

Both modes set the panel height to 480.

- [ ] **Step 2: Chat**

On entering chat, `Task.detached` loads `providers`. Default provider is the one with `isDefault`, else the first. Model defaults to `defaultModel` until the user picks one.

Header: back button, `AI Chat`, two `Menu`s (provider id, model). Changing provider clears the explicit model.

Messages: user text in a bubble, accent at 12% (light) or 18% (dark), corner radius 12 except the bottom-trailing 4. Assistant body: split on `/```([^\n`]*)\n([\s\S]*?)(?:```|$)/` the way `Markdown.tsx` does. Non-fence parts go through `AttributedString(markdown:options:)` with `interpretedSyntax: .inlineOnlyPreservingWhitespace` when that parses, else plain `Text`. Each fence is a block with the language label, a Copy button, and monospaced text on `#f1f1f4` (light) / `#18181b` (dark). An empty assistant turn that is still streaming shows an 8×16 accent rectangle that blinks, in place of the web caret.

Composer: plain field, Return sends, Shift-Return inserts a newline. Esc stops the stream (`cancel`) or, if idle, goes back to search and clears the query. Cmd-Return calls `paste` with the last assistant text. Cmd-Shift-C calls `copy` and toasts `Copied answer`. Cmd-N clears the turns and cancels.

`stream` body:

```swift
[
  "op": "stream",
  "requestId": UUID().uuidString,
  "provider": provider as Any,
  "model": model as Any,
  "system": initSystem as Any,
  "messages": turns.map { ["role": $0.role, "content": $0.content] }
]
```

Omit `system` when nil. Apply `ai` events only when `requestId` matches. `delta` appends `event.text`. `error` shows `event.text` under the turn. `done` and `error` clear the streaming flag. If `stream` itself returns `error`, show that string the same way.

No providers: `No AI provider configured. Run Open Config Folder, add a [[providers]] entry to config.toml, then Reload Commands.`

Footer hints: `Paste answer` `⌘↩`, `Copy` `⌘⇧C`, `New` `⌘N`, and `Stop` or `Back` with `esc`. While streaming, the left side says `Streaming…` instead of `OpenCast`.

When `show` arrives and the mode is `args` or `output`, return to search. Leave chat up. That matches `App.tsx`.

- [ ] **Step 3: Rebuild and check**

Run the Task 7 `swiftc` command again. Open the app. Tab on a query opens chat and streams if a provider is configured. Esc returns to search. A script command that returns full output shows the output view. An argument command shows the form.

- [ ] **Step 4: Commit**

```bash
git add macos
git commit -m "$(cat <<'EOF'
Add argument, output, and chat modes to the Swift panel.

Chat tokens arrive on the AI callback. Fenced blocks keep a copy button.
EOF
)"
```

---

### Task 9: Seek overlay

**Files:**
- Create: `macos/SeekView.swift`
- Modify: `macos/OpenCastApp.swift` (second panel)

**Interfaces:**
- Consumes: callback `{"type":"seek","scan":...}` and ops `seekClick`, `seekClose`
- Produces: the transparent Seek panel from the spec

- [ ] **Step 1: Second panel**

On `seek`, build or reuse a borderless transparent `NSPanel` at level 25, frame equal to `scan.monitor` converted from Core Graphics top-left space into AppKit bottom-left space:

```swift
let screen = NSScreen.screens.max(by: { $0.frame.maxY < $1.frame.maxY }) ?? NSScreen.main!
let x = scan.monitor.x
let y = screen.frame.maxY - scan.monitor.y - scan.monitor.h
```

Use the screen with the greatest `frame.maxY` as the primary, which is the AppKit anchor. Set the panel frame to `(x, y, w, h)` with no animation. Order it front and make it key. Hide the launcher panel without calling `NSApp.hide` (the Seek panel needs to stay up). `Model.hide()` when Seek is visible only hides Seek if the event is a user Esc; the Rust `hide` emitted just before `seek` must not tear Seek down. Implement that as: `accept` applies events in order, and a `seek` event sets `seekScan` after processing a preceding `hide`, so the launcher hides and Seek shows. A later `hide` (Esc, or `seekClose`) clears `seekScan`, orders Seek out, and calls `NSApp.hide`.

- [ ] **Step 2: Pill and hits**

Pill: 480×48, black, bottom radius 24, centered on the top edge of the Seek panel, 0.16s drop. Accent `viewfinder` on the left. Plain white field, 16pt. Count on the right, white at 55%.

Hits are in the same point space as `scan.monitor`. Draw them in the panel's local coordinates by subtracting `monitor.x` and `monitor.y`. Fill accent at 50% opacity, 1.5pt accent stroke, 4pt corner radius. The current match is opacity 1 and an extra 3pt accent stroke at 12% opacity. The matched span is a 2pt accent line. Character x uses the word boxes (`at`, `len`, `x`, `w`) when present, otherwise an even split of the line width, matching `charX` in `src/Seek.tsx`.

Typing filters case-insensitively. Tab or `;` cycles the current match. Return calls `seekClick` with the match center in screen points (`monitor.x + localX`, `monitor.y + localY`) via `Task.detached`, after the model has hidden Seek. Esc calls `seekClose` the same way. Rust waits 120ms and clicks. Swift does not sleep.

- [ ] **Step 3: Rebuild and check**

Rebuild the debug app. Run Click Text from the launcher (or its hotkey). The pill hangs from the top of that display. Matches box the words. Return clicks. Esc dismisses.

- [ ] **Step 4: Commit**

```bash
git add macos
git commit -m "$(cat <<'EOF'
Show Click Text as a native overlay.

The Seek panel uses the OCR monitor rect, and a return click goes through seekClick.
EOF
)"
```

---

### Task 10: Bundle script, docs, and deleting the web shell

**Files:**
- Modify: `build-app.command`
- Modify: `README.md`, `CLAUDE.md`, `crates/opencast-core/src/lib.rs`
- Modify: `Cargo.toml` (drop `src-tauri` from members)
- Delete: `src/`, `src-tauri/`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `seek.html`
- Move: `src-tauri/icons/icon.icns` to `macos/AppIcon.icns` before deleting `src-tauri`

**Interfaces:**
- Consumes: the debug app from Task 9
- Produces: `build-app.command` installs `~/Applications/OpenCast.app` with no npm. `cargo test` at the root still passes.

- [ ] **Step 1: Replace build-app.command**

Keep the signing-identity lookup and the `pkill` of a running OpenCast. Delete the npm invocation and the vite pkill. Build the release staticlib, compile Swift with `-O` into `target/release/OpenCast.app` using the same bundle layout as Task 7 (icon from `macos/AppIcon.icns`), `codesign --force --deep --sign "$APPLE_SIGNING_IDENTITY"` when that variable is non-empty, then `ditto` to `~/Applications/OpenCast.app` and `open` it. The native-static-libs awk line from Task 7 is the link line. Target triple: `arm64-apple-macos14.0`.

- [ ] **Step 2: Docs**

`CLAUDE.md`: the shell is SwiftUI plus `crates/opencast-ffi`. Requirements are Rust and the Xcode command-line tools. `cargo test` runs the core. `./build-app.command` builds and installs the app. Delete the Node, Vite, and `src/mock.ts` notes.

`README.md` quick start: drop `npm install` and `npm run tauri`. Say the panel is SwiftUI and the engine is Rust. Delete the Linux WebKitGTK sentence. Keep the config, hotkey, permission, and provider sections; they are still true.

`crates/opencast-core/src/lib.rs` module comment: the macOS shell is `crates/opencast-ffi` and `macos/`, not Tauri.

Root `Cargo.toml` members become `["crates/opencast-core", "crates/opencast-ffi"]`.

- [ ] **Step 3: Delete the web shell**

```bash
git rm -r src src-tauri package.json package-lock.json vite.config.ts tsconfig.json index.html seek.html
```

If `tsconfig.node.json` or `dist/` is tracked, remove those too. Do not remove `examples/`.

- [ ] **Step 4: Verify**

Run: `cargo test`

Expected: core tests PASS. `cargo test -p opencast-ffi --lib` PASS.

Run: `./build-app.command`

Expected: `~/Applications/OpenCast.app` launches, Option-Space toggles the island, an app row shows its icon, the panel is short when there are few results.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
Ship OpenCast as a SwiftUI app and drop the Node build.

The release script links libopencast_ffi and no longer runs npm or Tauri.
EOF
)"
```
