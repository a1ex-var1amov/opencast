//! Caps Lock as a hyper key (⌃⌥⌘⇧), Superkey-style. macOS only; needs the
//! Accessibility permission, like window management.
//!
//! `hidutil` remaps Caps Lock to F18. A CGEventTap then swallows F18 itself,
//! adds all four modifiers to any key pressed while it is held, and treats a
//! tap with nothing else pressed as "toggle the launcher".

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

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
    }

    /// Install the event tap on its own thread. `on_tap` runs when F18 is
    /// pressed and released with no other key in between.
    // ponytail: a tap macOS disables (timeout) is not re-enabled; the
    // callback is microseconds so it should not happen. Restart if it does.
    pub fn start(on_tap: impl Fn() + Send + 'static) {
        std::thread::spawn(move || {
            let held = AtomicBool::new(false);
            let used = AtomicBool::new(false);
            let hyper = CGEventFlags::CGEventFlagControl
                | CGEventFlags::CGEventFlagAlternate
                | CGEventFlags::CGEventFlagCommand
                | CGEventFlags::CGEventFlagShift;
            let r = CGEventTap::with_enabled(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![CGEventType::KeyDown, CGEventType::KeyUp],
                |_, kind, event| {
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
                CFRunLoop::run_current,
            );
            if r.is_err() {
                eprintln!("opencast: could not install the hyper key (is Accessibility enabled for OpenCast?)");
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(not(target_os = "macos"))]
pub fn remap_caps_lock(_on: bool) {}

#[cfg(not(target_os = "macos"))]
pub fn start(_on_tap: impl Fn() + Send + 'static) {}
