//! Caps Lock as a hyper key (⌃⌥⌘⇧), Superkey-style. macOS only; needs the
//! Accessibility permission, like window management.
//!
//! `hidutil` remaps Caps Lock to F18. A CGEventTap then swallows F18 itself,
//! adds all four modifiers to any key pressed while it is held, and treats a
//! tap with nothing else pressed as "toggle the launcher".
//!
//! The remap outlives the process, so it is undone on quit, on SIGTERM, and
//! on the next launch if a crash left it behind. macOS disables a tap whose
//! callback stalls; a watchdog re-enables it.

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::runloop::{kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop};
    use core_graphics::event::{
        CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use opencast_core::config::Config;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    use std::sync::Once;
    use std::time::Duration;

    const MARKER_NAME: &str = "opencast-hyperkey.on";

    pub fn marker_path() -> PathBuf {
        Config::config_dir().join(MARKER_NAME)
    }

    fn set_marker() {
        let path = marker_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::File::create(path);
    }

    fn clear_marker() {
        let _ = std::fs::remove_file(marker_path());
    }

    extern "C" fn atexit_restore() {
        restore_caps_lock_if_ours();
    }

    /// Last-resort restore when the process exits without AppKit shutdown or signals.
    pub fn register_exit_restore() {
        static REGISTERED: Once = Once::new();
        REGISTERED.call_once(|| unsafe {
            extern "C" {
                fn atexit(cb: extern "C" fn()) -> i32;
            }
            let _ = atexit(atexit_restore);
        });
    }

    const F18_KEYCODE: i64 = 79;
    const CAPS_LOCK_USAGE: u64 = 0x700000039;
    const F18_USAGE: u64 = 0x70000006D;

    /// Remap Caps Lock → F18 (or undo it). Lasts until reboot.
    pub fn remap_caps_lock(on: bool) {
        let mapping = if on {
            format!(
                r#"{{"UserKeyMapping":[{{"HIDKeyboardModifierMappingSrc":{CAPS_LOCK_USAGE},"HIDKeyboardModifierMappingDst":{F18_USAGE}}}]}}"#
            )
        } else {
            r#"{"UserKeyMapping":[]}"#.to_string()
        };
        let _ = Command::new("hidutil")
            .args(["property", "--set", &mapping])
            .stdout(Stdio::null())
            .status();
        if on {
            set_marker();
        } else {
            clear_marker();
        }
    }

    /// True when the current `hidutil` mapping is the Caps Lock → F18 one we install.
    pub fn caps_lock_is_ours() -> bool {
        let Ok(out) = Command::new("hidutil")
            .args(["property", "--get", "UserKeyMapping"])
            .output()
        else {
            return false;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let src = format!("{CAPS_LOCK_USAGE}");
        let dst = format!("{F18_USAGE}");
        // hidutil prints the values in decimal or hex depending on the release.
        let src_hex = format!("{CAPS_LOCK_USAGE:#x}");
        let dst_hex = format!("{F18_USAGE:#x}");
        (text.contains(&src) || text.contains(&src_hex)) && (text.contains(&dst) || text.contains(&dst_hex))
    }

    /// Undo our remap, and only ours (leave Karabiner & co alone). The marker
    /// covers a crash where `hidutil --get` parsing is flaky but Caps Lock is still F18.
    pub fn restore_caps_lock_if_ours() {
        if caps_lock_is_ours() || marker_path().is_file() {
            remap_caps_lock(false);
        }
    }

    static TAP_DISABLED: AtomicBool = AtomicBool::new(false);

    /// Install the event tap on its own thread. `on_tap` runs when F18 is
    /// pressed and released with no other key in between.
    pub fn start(on_tap: impl Fn() + Send + Sync + 'static) {
        std::thread::spawn(move || {
            let held = AtomicBool::new(false);
            let used = AtomicBool::new(false);
            let hyper = CGEventFlags::CGEventFlagControl
                | CGEventFlags::CGEventFlagAlternate
                | CGEventFlags::CGEventFlagCommand
                | CGEventFlags::CGEventFlagShift;
            let tap = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                // Disabled-tap notices arrive regardless of the mask (and must not be masked: their raw values overflow the bit shift).
                vec![CGEventType::KeyDown, CGEventType::KeyUp],
                move |_, kind, event| {
                    match kind {
                        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                            TAP_DISABLED.store(true, SeqCst);
                            return CallbackResult::Keep;
                        }
                        _ => {}
                    }
                    if event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) == F18_KEYCODE {
                        match kind {
                            CGEventType::KeyDown => {
                                // Auto-repeat sends more KeyDowns; only the first counts.
                                if !held.swap(true, SeqCst) {
                                    used.store(false, SeqCst);
                                }
                            }
                            CGEventType::KeyUp => {
                                held.store(false, SeqCst);
                                if !used.load(SeqCst) {
                                    on_tap();
                                }
                            }
                            _ => {}
                        }
                        return CallbackResult::Drop;
                    }
                    if held.load(SeqCst) {
                        used.store(true, SeqCst);
                        event.set_flags(event.get_flags() | hyper);
                    }
                    CallbackResult::Keep
                },
            );
            let tap = match tap {
                Ok(t) => t,
                Err(_) => {
                    eprintln!("opencast: could not install the hyper key (is Accessibility enabled for OpenCast?)");
                    return;
                }
            };
            let Ok(source) = tap.mach_port().create_runloop_source(0) else {
                eprintln!("opencast: could not attach the hyper key to the run loop");
                return;
            };
            let run_loop = CFRunLoop::get_current();
            unsafe {
                run_loop.add_source(&source, kCFRunLoopCommonModes);
            }
            tap.enable();
            // Pump the run loop in slices so the watchdog can re-enable a tap
            // macOS switched off (it does that when a callback runs too long).
            loop {
                // Common modes is a set you register sources in, not a mode you can run:
                // running it returns at once and this loop would spin. The tap's source is in
                // the common set, which includes the default mode.
                unsafe {
                    CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_secs(1), false);
                }
                if TAP_DISABLED.swap(false, SeqCst) {
                    tap.enable();
                    eprintln!("opencast: hyper key tap re-enabled");
                }
            }
        });
    }

    #[cfg(test)]
    mod tests {
        use super::marker_path;
        use std::path::PathBuf;

        #[test]
        fn marker_path_under_config_dir() {
            std::env::set_var("OPENCAST_CONFIG_DIR", "/tmp/opencast-hyperkey-marker-test");
            assert_eq!(
                marker_path(),
                PathBuf::from("/tmp/opencast-hyperkey-marker-test/opencast-hyperkey.on")
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(not(target_os = "macos"))]
pub fn remap_caps_lock(_on: bool) {}

#[cfg(not(target_os = "macos"))]
pub fn restore_caps_lock_if_ours() {}

#[cfg(not(target_os = "macos"))]
pub fn start(_on_tap: impl Fn() + Send + Sync + 'static) {}

#[cfg(not(target_os = "macos"))]
pub fn register_exit_restore() {}
