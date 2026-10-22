//! System Settings panes as commands (`wifi`, `bluetooth`, `privacy`…).
//! macOS opens the pane from its `x-apple.systempreferences:` URL; an id it
//! does not know still opens System Settings.

use crate::commands::{Command, CommandKind};

/// (id, title, url id, keywords)
pub const PANES: &[(&str, &str, &str, &[&str])] = &[
    (
        "wifi",
        "Wi-Fi",
        "com.apple.wifi-settings-extension",
        &["wireless", "network", "internet"],
    ),
    (
        "bluetooth",
        "Bluetooth",
        "com.apple.BluetoothSettings",
        &["airpods", "headphones", "pair"],
    ),
    (
        "network",
        "Network",
        "com.apple.Network-Settings.extension",
        &["ethernet", "proxy", "dns", "internet"],
    ),
    (
        "vpn",
        "VPN",
        "com.apple.NetworkExtensionSettingsUI.NESettingsUIExtension",
        &["tunnel", "network"],
    ),
    (
        "notifications",
        "Notifications",
        "com.apple.Notifications-Settings.extension",
        &["alerts", "banners"],
    ),
    (
        "sound",
        "Sound",
        "com.apple.Sound-Settings.extension",
        &["audio", "volume", "output", "input", "microphone"],
    ),
    (
        "focus",
        "Focus",
        "com.apple.Focus-Settings.extension",
        &["do not disturb", "dnd"],
    ),
    (
        "screen-time",
        "Screen Time",
        "com.apple.Screen-Time-Settings.extension",
        &["limits", "usage"],
    ),
    (
        "general",
        "General",
        "com.apple.systempreferences.GeneralSettings",
        &["about", "software"],
    ),
    (
        "about",
        "About This Mac",
        "com.apple.SystemProfiler.AboutExtension",
        &["serial", "model", "version"],
    ),
    (
        "software-update",
        "Software Update",
        "com.apple.Software-Update-Settings.extension",
        &["upgrade", "macos"],
    ),
    (
        "storage",
        "Storage",
        "com.apple.settings.Storage",
        &["disk", "space", "free"],
    ),
    (
        "login-items",
        "Login Items",
        "com.apple.LoginItems-Settings.extension",
        &["startup", "launch at login", "background"],
    ),
    (
        "date-time",
        "Date & Time",
        "com.apple.Date-Time-Settings.extension",
        &["clock", "timezone", "time zone"],
    ),
    (
        "sharing",
        "Sharing",
        "com.apple.Sharing-Settings.extension",
        &["airdrop", "file sharing", "remote", "hostname"],
    ),
    (
        "appearance",
        "Appearance",
        "com.apple.Appearance-Settings.extension",
        &["dark mode", "accent", "theme"],
    ),
    (
        "accessibility",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        &["zoom", "voiceover", "contrast"],
    ),
    (
        "control-center",
        "Control Center",
        "com.apple.ControlCenter-Settings.extension",
        &["menu bar", "menubar"],
    ),
    (
        "siri",
        "Apple Intelligence & Siri",
        "com.apple.Siri-Settings.extension",
        &["siri", "ai", "assistant"],
    ),
    (
        "privacy",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        &["security", "filevault", "permissions"],
    ),
    (
        "desktop-dock",
        "Desktop & Dock",
        "com.apple.Desktop-Settings.extension",
        &["dock", "stage manager", "mission control", "hot corners"],
    ),
    (
        "displays",
        "Displays",
        "com.apple.Displays-Settings.extension",
        &["monitor", "resolution", "night shift", "brightness"],
    ),
    (
        "wallpaper",
        "Wallpaper",
        "com.apple.Wallpaper-Settings.extension",
        &["background", "desktop picture"],
    ),
    (
        "screen-saver",
        "Screen Saver",
        "com.apple.ScreenSaver-Settings.extension",
        &["screensaver"],
    ),
    (
        "battery",
        "Battery",
        "com.apple.Battery-Settings.extension",
        &["energy", "power", "low power"],
    ),
    (
        "lock-screen",
        "Lock Screen",
        "com.apple.Lock-Screen-Settings.extension",
        &["sleep", "password", "display off"],
    ),
    (
        "touch-id",
        "Touch ID & Password",
        "com.apple.Touch-ID-Settings.extension",
        &["fingerprint", "password"],
    ),
    (
        "users",
        "Users & Groups",
        "com.apple.Users-Groups-Settings.extension",
        &["accounts", "guest"],
    ),
    (
        "passwords",
        "Passwords",
        "com.apple.Passwords-Settings.extension",
        &["keychain"],
    ),
    (
        "internet-accounts",
        "Internet Accounts",
        "com.apple.Internet-Accounts-Settings.extension",
        &["email", "google", "icloud mail"],
    ),
    (
        "keyboard",
        "Keyboard",
        "com.apple.Keyboard-Settings.extension",
        &["shortcuts", "input sources", "dictation", "key repeat"],
    ),
    (
        "trackpad",
        "Trackpad",
        "com.apple.Trackpad-Settings.extension",
        &["gestures", "tap to click", "scroll"],
    ),
    (
        "mouse",
        "Mouse",
        "com.apple.Mouse-Settings.extension",
        &["scroll", "tracking speed"],
    ),
    (
        "printers",
        "Printers & Scanners",
        "com.apple.Print-Scan-Settings.extension",
        &["printer", "scanner"],
    ),
    // Privacy panes OpenCast itself needs.
    (
        "privacy-accessibility",
        "Privacy: Accessibility",
        "com.apple.preference.security?Privacy_Accessibility",
        &["permission", "ax", "allow apps to control"],
    ),
    (
        "privacy-screen",
        "Privacy: Screen & System Audio Recording",
        "com.apple.preference.security?Privacy_ScreenCapture",
        &["screen recording", "permission", "capture"],
    ),
    (
        "privacy-files",
        "Privacy: Full Disk Access",
        "com.apple.preference.security?Privacy_AllFiles",
        &["full disk access", "fda", "permission"],
    ),
    (
        "privacy-automation",
        "Privacy: Automation",
        "com.apple.preference.security?Privacy_Automation",
        &["apple events", "permission"],
    ),
    (
        "privacy-microphone",
        "Privacy: Microphone",
        "com.apple.preference.security?Privacy_Microphone",
        &["permission", "mic"],
    ),
    (
        "privacy-camera",
        "Privacy: Camera",
        "com.apple.preference.security?Privacy_Camera",
        &["permission", "webcam"],
    ),
    (
        "privacy-calendars",
        "Privacy: Calendars",
        "com.apple.preference.security?Privacy_Calendars",
        &["permission"],
    ),
    (
        "privacy-contacts",
        "Privacy: Contacts",
        "com.apple.preference.security?Privacy_Contacts",
        &["permission"],
    ),
    (
        "privacy-location",
        "Privacy: Location Services",
        "com.apple.preference.security?Privacy_LocationServices",
        &["permission", "gps"],
    ),
];

pub fn url(pane: &str) -> Option<String> {
    PANES
        .iter()
        .find(|(id, ..)| *id == pane)
        .map(|(_, _, u, _)| format!("x-apple.systempreferences:{u}"))
}

pub fn commands() -> Vec<Command> {
    PANES
        .iter()
        .map(|(id, title, _, kw)| Command {
            id: format!("pane:{id}"),
            title: (*title).to_string(),
            subtitle: Some("System Settings".into()),
            icon: Some("⚙️".into()),
            kind: CommandKind::Builtin,
            arguments: vec![],
            hotkey: None,
            keywords: kw
                .iter()
                .map(|s| s.to_string())
                .chain([
                    "settings".to_string(),
                    "system settings".to_string(),
                    "preferences".to_string(),
                ])
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_have_unique_ids_and_urls() {
        let mut ids: Vec<&str> = PANES.iter().map(|p| p.0).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PANES.len());
        assert_eq!(
            url("bluetooth").as_deref(),
            Some("x-apple.systempreferences:com.apple.BluetoothSettings")
        );
        assert!(url("nope").is_none());
        assert_eq!(commands().len(), PANES.len());
    }
}
