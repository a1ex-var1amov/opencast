//! OS glue: clipboard, grabbing the selected text, pasting into the
//! frontmost app. macOS uses System Events (needs Accessibility permission
//! for OpenCast in System Settings → Privacy & Security → Accessibility).

use std::process::Command as Process;
use std::time::Duration;

use tauri::AppHandle;

pub fn write_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| format!("clipboard: {e}"))
}

#[allow(dead_code)] // unused on Linux, which reads the PRIMARY selection instead
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
pub async fn grab_selection(app: &AppHandle) -> String {
    #[cfg(target_os = "macos")]
    {
        super::hide_launcher(app);
        tokio::time::sleep(Duration::from_millis(180)).await;
        let saved = read_clipboard();
        let marker = format!("\u{2063}opencast-{}", std::process::id());
        let _ = write_clipboard(&marker);
        if keystroke("c").is_err() {
            if let Some(s) = &saved {
                let _ = write_clipboard(s);
            }
            return String::new();
        }
        // Poll briefly: some apps take a moment to fill the pasteboard.
        let mut got = String::new();
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(30)).await;
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
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        // X11/Wayland expose the selection directly as the PRIMARY buffer.
        for (cmd, args) in [
            ("wl-paste", &["--primary", "--no-newline"][..]),
            ("xclip", &["-o", "-selection", "primary"][..]),
        ] {
            if let Ok(out) = Process::new(cmd).args(args).output() {
                if out.status.success() {
                    return String::from_utf8_lossy(&out.stdout).into_owned();
                }
            }
        }
        String::new()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = app;
        let _ = Duration::ZERO;
        read_clipboard().unwrap_or_default()
    }
}

/// Paste the clipboard into the frontmost app (launcher already hidden).
pub async fn paste_into_frontmost() -> Result<(), String> {
    tokio::time::sleep(Duration::from_millis(180)).await;
    #[cfg(target_os = "macos")]
    {
        keystroke("v")
    }
    #[cfg(target_os = "linux")]
    {
        // Best effort on X11; on Wayland the text stays on the clipboard.
        let _ = Process::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .status();
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(())
    }
}
