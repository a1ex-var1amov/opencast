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
    fn dispatch_async_f(queue: *const u8, context: *mut std::ffi::c_void, work: extern "C" fn(*mut std::ffi::c_void));
    fn pthread_main_np() -> i32;
}

fn dispatch_get_main_queue() -> *const u8 {
    std::ptr::addr_of!(_dispatch_main_q)
}

/// `RegisterEventHotKey` only delivers presses to a manager created on the main
/// run loop. `dispatch_sync` from that thread deadlocks, so it runs inline.
///
/// From any other thread the work goes to the main queue *asynchronously* and we
/// wait on a channel with a timeout: a main thread stuck in slow or permission-
/// gated work (a TCC prompt for the Desktop folder, say) must not wedge engine
/// startup forever. The queued block still runs when the main thread frees up,
/// so the keys do get registered — just later.
fn on_main<F>(work: F) -> Vec<String>
where
    F: FnOnce() -> Vec<String> + Send + 'static,
{
    if unsafe { pthread_main_np() } != 0 {
        return work();
    }
    extern "C" fn run<F: FnOnce() -> Vec<String>>(ctx: *mut std::ffi::c_void) {
        let slot = unsafe { &mut *(ctx as *mut (Option<F>, Option<Vec<String>>)) };
        let work = slot.0.take().unwrap();
        slot.1 = Some(work());
    }
    if std::env::var("OPENCAST_HOTKEY_SYNC").is_ok() {
        // Escape hatch for debugging: the old blocking path.
        let mut slot = (Some(work), None);
        unsafe {
            dispatch_sync_f(
                dispatch_get_main_queue(),
                &mut slot as *mut _ as *mut std::ffi::c_void,
                run::<F>,
            );
        }
        return slot.1.unwrap();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let task: Box<dyn FnOnce() + Send> = Box::new(move || {
        let _ = tx.send(work());
    });
    extern "C" fn run_boxed(ctx: *mut std::ffi::c_void) {
        let task: Box<Box<dyn FnOnce() + Send>> = unsafe { Box::from_raw(ctx as *mut Box<dyn FnOnce() + Send>) };
        (*task)();
    }
    let raw = Box::into_raw(Box::new(task)) as *mut std::ffi::c_void;
    unsafe { dispatch_async_f(dispatch_get_main_queue(), raw, run_boxed) };
    match rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(problems) => problems,
        Err(_) => vec!["hotkeys: the app was still busy 10 s after launch, so keys land late".into()],
    }
}

pub fn register(engine: &std::sync::Arc<opencast_core::engine::Engine>) -> Vec<String> {
    let engine = std::sync::Arc::clone(engine);
    on_main(move || register_here(&engine))
}

fn register_here(engine: &opencast_core::engine::Engine) -> Vec<String> {
    let mut problems = Vec::new();
    // Tear the old manager down *first*. Carbon refuses a second handler on the same
    // target while the previous one is alive (eventHandlerAlreadyInstalledErr), so
    // building a manager before unregistering the old one broke every re-registration
    // after the first: reloads and the post-launch retry left the keys dead.
    let previous = MANAGER.lock().unwrap().take();
    if let Some(previous) = previous {
        let mut keys = BINDS.lock().unwrap().iter().map(|(k, _)| *k).collect::<Vec<_>>();
        if let Some(t) = *TOGGLE.lock().unwrap() {
            keys.push(t);
        }
        let _ = previous.unregister_all(&keys);
        drop(previous); // RemoveEventHandler runs here
    }

    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            problems.push(format!("could not start the hotkey manager ({e})"));
            return problems;
        }
    };

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
                            crate::exec::run_command(cmd, vec![], crate::exec::ExecContext::default()).await
                        };
                        if matches!(result, crate::exec::ExecResult::Hide) {
                            return;
                        }
                        // Window snaps and layouts run in the shell without the panel.
                        if result.wants_panel() {
                            host::emit(serde_json::json!({ "type": "show" }));
                        }
                        host::emit(serde_json::json!({ "type": "result", "result": result }));
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

    /// A preset key the parser rejects would just never fire, silently.
    #[test]
    fn every_preset_window_key_parses() {
        for (name, _, set) in opencast_core::config::WINDOW_HOTKEY_PRESETS {
            let mut seen = std::collections::HashSet::new();
            for &(key, id) in set.iter() {
                let Ok(parsed) = key.parse::<HotKey>() else {
                    panic!("{name} preset: `{key}` ({id}) is not a hotkey");
                };
                assert!(seen.insert(parsed), "{name} preset: `{key}` duplicates another entry");
            }
        }
    }
}
