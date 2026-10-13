//! Toggle hotkey and `[[hotkeys]]` bindings. Problems are returned, never a hard failure.

use std::sync::{Mutex, Once};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::host;

static MANAGER: Mutex<Option<GlobalHotKeyManager>> = Mutex::new(None);
static TOGGLE: Mutex<Option<HotKey>> = Mutex::new(None);
static BINDS: Mutex<Vec<(HotKey, String)>> = Mutex::new(Vec::new());

extern "C" {
    // `dispatch_get_main_queue()` is a header inline of this symbol.
    static _dispatch_main_q: u8;
    fn dispatch_sync_f(queue: *const u8, context: *mut std::ffi::c_void, work: extern "C" fn(*mut std::ffi::c_void));
    fn pthread_main_np() -> i32;
}

fn dispatch_get_main_queue() -> *const u8 {
    std::ptr::addr_of!(_dispatch_main_q)
}

/// `RegisterEventHotKey` only delivers presses to a manager created on the main
/// run loop. `dispatch_sync` from that thread deadlocks, so it runs inline.
fn on_main<F: FnOnce() -> Vec<String>>(work: F) -> Vec<String> {
    if unsafe { pthread_main_np() } != 0 {
        return work();
    }
    extern "C" fn run<F: FnOnce() -> Vec<String>>(ctx: *mut std::ffi::c_void) {
        let slot = unsafe { &mut *(ctx as *mut (Option<F>, Option<Vec<String>>)) };
        let work = slot.0.take().unwrap();
        slot.1 = Some(work());
    }
    let mut slot = (Some(work), None);
    unsafe {
        dispatch_sync_f(
            dispatch_get_main_queue(),
            &mut slot as *mut _ as *mut std::ffi::c_void,
            run::<F>,
        );
    }
    slot.1.unwrap()
}

pub fn register(engine: &opencast_core::engine::Engine) -> Vec<String> {
    on_main(|| register_here(engine))
}

fn register_here(engine: &opencast_core::engine::Engine) -> Vec<String> {
    let manager = GlobalHotKeyManager::new();
    let mut problems = Vec::new();
    let Ok(manager) = manager else {
        problems.push("could not start the hotkey manager".into());
        return problems;
    };
    {
        let prev = MANAGER.lock().unwrap();
        if let Some(prev) = prev.as_ref() {
            let mut keys = BINDS.lock().unwrap().iter().map(|(k, _)| *k).collect::<Vec<_>>();
            if let Some(t) = *TOGGLE.lock().unwrap() {
                keys.push(t);
            }
            let _ = prev.unregister_all(&keys);
        }
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
        // `Hyper+X` = all four modifiers, what the hyper key sends.
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

/// The event channel is process-global, so a second reader would steal presses.
pub(crate) fn listen() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::thread::spawn(|| {
            let rx = GlobalHotKeyEvent::receiver();
            for event in rx.iter() {
                if event.state != HotKeyState::Pressed {
                    continue;
                }
                let id = event.id;
                // Drop the guard before the callback. A reload from that callback
                // locks `TOGGLE` again on this thread.
                let toggle = { TOGGLE.lock().unwrap().is_some_and(|t| t.id == id) };
                if toggle {
                    host::toggle_panel();
                    continue;
                }
                let cmd = BINDS
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(k, _)| k.id == id)
                    .map(|(_, c)| c.clone());
                if let Some(cmd) = cmd {
                    host::runtime().spawn(async move {
                        // Clone under the engine lock, then drop it before `.await`.
                        let command = host::with_engine(|engine| {
                            engine
                                .registry
                                .get(&cmd)
                                .and_then(|c| c.arguments.iter().any(|a| !a.optional).then(|| c.clone()))
                        });
                        let result = if let Some(command) = command {
                            crate::exec::ExecResult::Args { command }
                        } else {
                            crate::exec::run_command(cmd, vec![]).await
                        };
                        if !matches!(result, crate::exec::ExecResult::Hide) {
                            host::emit(serde_json::json!({ "type": "show" }));
                            host::emit(serde_json::json!({ "type": "result", "result": result }));
                        }
                    });
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::HotKey;

    #[test]
    fn parser_accepts_the_tauri_shortcut_strings() {
        let toggle: HotKey = "Alt+Space".parse().unwrap();
        let arrows: HotKey = "Ctrl+Alt+Left".parse().unwrap();
        let f18: HotKey = "F18".parse().unwrap();
        let hyper: HotKey = "Hyper+K".replace("Hyper+", "Ctrl+Alt+Cmd+Shift+").parse().unwrap();
        assert_ne!(toggle, arrows);
        assert_ne!(toggle, f18);
        assert_ne!(hyper, toggle);
        assert_eq!(hyper, "Ctrl+Alt+Cmd+Shift+K".parse::<HotKey>().unwrap());
    }
}
