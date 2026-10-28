import Foundation

/// Launcher hotkey string helpers (AppKit-free for panel logic tests).
enum HotkeyChord {
    /// `Cmd+Space` / `Command+Space` (any case) — Spotlight's default chord.
    static func isCmdSpace(_ spec: String) -> Bool {
        let parts = spec.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
        guard parts.count == 2, parts.last == "space" else { return false }
        return parts[0] == "cmd" || parts[0] == "command"
    }
}
