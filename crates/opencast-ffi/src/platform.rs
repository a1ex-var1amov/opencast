//! OS glue: clipboard, grabbing the selected text, pasting into the
//! frontmost app.
//!
//! Keystrokes are synthesized with CGEvent (no `osascript` round trip), which
//! needs the Accessibility permission for OpenCast in System Settings →
//! Privacy & Security → Accessibility.

use std::time::Duration;

pub fn write_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| format!("clipboard: {e}"))
}

pub fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// ⌘+key via a HID-level CGEvent. `keycode` is the ANSI virtual key code.
#[cfg(target_os = "macos")]
fn command_key(keycode: u16) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "could not create an event source".to_string())?;
    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(src.clone(), keycode, down).map_err(|_| {
            "macOS blocked the keystroke — allow OpenCast under Privacy & Security → Accessibility".to_string()
        })?;
        ev.set_flags(CGEventFlags::CGEventFlagCommand);
        ev.post(CGEventTapLocation::HID);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
const KEY_C: u16 = 8;
#[cfg(target_os = "macos")]
const KEY_V: u16 = 9;

/// Copy the selection of the app that had focus before the launcher, via
/// ⌘C and a clipboard marker. Hides the launcher so focus returns to that
/// app; the caller re-shows it. The shell's AX read is tried first (see
/// `ExecContext::selection`); this is the fallback for apps without AX text.
pub async fn grab_selection() -> String {
    crate::host::emit(serde_json::json!({ "type": "hide" }));
    tokio::time::sleep(Duration::from_millis(120)).await;
    let saved = read_clipboard();
    let marker = format!("\u{2063}opencast-{}", std::process::id());
    let _ = write_clipboard(&marker);
    if command_key(KEY_C).is_err() {
        if let Some(s) = &saved {
            let _ = write_clipboard(s);
        }
        return String::new();
    }
    let mut got = String::new();
    for _ in 0..12 {
        tokio::time::sleep(Duration::from_millis(25)).await;
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

/// Paste the clipboard into the frontmost app (launcher already hidden).
pub async fn paste_into_frontmost() -> Result<(), String> {
    // Focus needs a beat to return to the previous app after the panel hides.
    tokio::time::sleep(Duration::from_millis(120)).await;
    command_key(KEY_V)
}

/// Press ← `n` times in the front app (snippet `{cursor}`).
#[cfg(target_os = "macos")]
pub fn move_cursor_left(n: usize) {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    if n == 0 {
        return;
    }
    // Let the paste land first.
    std::thread::sleep(Duration::from_millis(60));
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    for _ in 0..n.min(2000) {
        for down in [true, false] {
            if let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), 123, down) {
                ev.post(CGEventTapLocation::HID);
            }
        }
    }
}
