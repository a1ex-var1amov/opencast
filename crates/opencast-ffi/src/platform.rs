//! OS glue: clipboard, grabbing the selected text, pasting into the
//! frontmost app. macOS uses System Events (needs Accessibility permission
//! for OpenCast in System Settings → Privacy & Security → Accessibility).

use std::process::Command as Process;
use std::time::Duration;

pub fn write_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| format!("clipboard: {e}"))
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

#[cfg(target_os = "macos")]
fn keystroke(key: &str) -> Result<(), String> {
    let script = format!("tell application \"System Events\" to keystroke \"{key}\" using command down");
    let st = Process::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|e| format!("osascript: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err("macOS blocked the keystroke — allow OpenCast under Privacy & Security → Accessibility".into())
    }
}

/// Copy the selection of the app that had focus before the launcher.
/// Hides the launcher so focus returns to that app; the caller re-shows it.
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

/// Paste the clipboard into the frontmost app (launcher already hidden).
pub async fn paste_into_frontmost() -> Result<(), String> {
    tokio::time::sleep(Duration::from_millis(180)).await;
    keystroke("v")
}
