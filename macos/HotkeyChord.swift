import Foundation

/// Launcher hotkey string helpers (AppKit-free for panel logic tests).
enum HotkeyChord {
    /// ANSI key codes → the names the hotkey parser understands.
    static let keyNames: [UInt16: String] = [
        0: "A", 1: "S", 2: "D", 3: "F", 4: "H", 5: "G", 6: "Z", 7: "X", 8: "C", 9: "V", 11: "B", 12: "Q",
        13: "W", 14: "E", 15: "R", 16: "Y", 17: "T", 18: "1", 19: "2", 20: "3", 21: "4", 22: "6", 23: "5",
        24: "Equal", 25: "9", 26: "7", 27: "Minus", 28: "8", 29: "0", 30: "BracketRight", 31: "O", 32: "U",
        33: "BracketLeft", 34: "I", 35: "P", 37: "L", 38: "J", 39: "Quote", 40: "K", 41: "Semicolon",
        42: "Backslash", 43: "Comma", 44: "Slash", 45: "N", 46: "M", 47: "Period", 50: "Backquote",
        36: "Enter", 48: "Tab", 49: "Space", 51: "Backspace", 117: "Delete",
        123: "Left", 124: "Right", 125: "Down", 126: "Up",
        122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7", 100: "F8", 101: "F9",
        109: "F10", 103: "F11", 111: "F12", 105: "F13", 107: "F14", 113: "F15", 106: "F16", 64: "F17",
        79: "F18", 80: "F19", 90: "F20",
    ]

    /// `Cmd+Space` / `Command+Space` (any case) — Spotlight's default chord.
    static func isCmdSpace(_ spec: String) -> Bool { canonical(spec) == "cmd+space" }

    /// A chord with spelling and order taken out, so two ways of writing it compare equal:
    /// `Command + Space`, `cmd+space` and `SUPER+Space` are all `cmd+space`.
    /// ponytail: symbol keys match by name only (`Equal`, not `=`). A miss means no
    /// conflict warning, never a false one; add the symbol spellings if someone binds one.
    static func canonical(_ spec: String) -> String {
        var parts = spec.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
        guard var key = parts.popLast() else { return "" }
        for prefix in ["key", "digit"] where key.hasPrefix(prefix) && key.count == prefix.count + 1 {
            key = String(key.suffix(1))
        }
        let names = ["command": "cmd", "super": "cmd", "option": "alt", "control": "ctrl"]
        let mods = Set(parts.flatMap { $0 == "hyper" ? ["ctrl", "alt", "shift", "cmd"] : [names[$0] ?? $0] })
        return (mods.sorted() + [key]).joined(separator: "+")
    }

    /// The same form for a macOS shortcut: virtual key code and Carbon modifier mask.
    /// ponytail: shortcuts with any other bit set are skipped. macOS sets the function-key
    /// bit (0x20000) on every arrow and F-key shortcut, and its Globe-key tiling shortcuts
    /// (Globe+Ctrl+Left) look exactly like plain ones (Ctrl+Left), so those cannot be told
    /// apart here. Clashes on arrows and F-keys go unreported; Settings › Test still finds them.
    static func canonical(keyCode: Int, carbonModifiers: Int) -> String? {
        let masks = [(0x100, "cmd"), (0x200, "shift"), (0x800, "alt"), (0x1000, "ctrl")]
        guard carbonModifiers & ~masks.reduce(0, { $0 | $1.0 }) == 0,
              let key = keyNames[UInt16(truncatingIfNeeded: keyCode)]
        else { return nil }
        let mods = masks.filter { carbonModifiers & $0.0 != 0 }.map(\.1)
        return (mods.sorted() + [key.lowercased()]).joined(separator: "+")
    }
}
