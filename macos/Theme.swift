import AppKit
import SwiftUI

private struct ThemePreferenceKey: EnvironmentKey {
    static let defaultValue = "system"
}

extension EnvironmentValues {
    /// OpenCast Appearance setting: `system` | `light` | `dark`.
    var themePreference: String {
        get { self[ThemePreferenceKey.self] }
        set { self[ThemePreferenceKey.self] = newValue }
    }
}

enum Theme {
    /// `theme_file` values from config; set by `Model.refreshSettings`.
    static var file = ThemeFile()

    /// Follows System Settings → Accent Color unless the theme file sets one.
    static var accent: Color {
        if let hex = file.accent, let c = color(hex: hex) { return c }
        return Color(nsColor: .controlAccentColor)
    }

    /// Row/field font: the theme's family when it names one that exists.
    static func font(size: Double, weight: Font.Weight = .regular) -> Font {
        if let name = file.font, !name.isEmpty, NSFont(name: name, size: CGFloat(size)) != nil {
            return .custom(name, size: CGFloat(size)).weight(weight)
        }
        return .system(size: CGFloat(size), weight: weight)
    }

    /// `#rrggbb` / `#rrggbbaa` → Color.
    static func color(hex: String) -> Color? {
        var h = hex.trimmingCharacters(in: .whitespaces)
        if h.hasPrefix("#") { h.removeFirst() }
        guard h.count == 6 || h.count == 8, let v = UInt64(h, radix: 16) else { return nil }
        let r, g, b, a: Double
        if h.count == 8 {
            r = Double((v >> 24) & 0xff) / 255; g = Double((v >> 16) & 0xff) / 255
            b = Double((v >> 8) & 0xff) / 255; a = Double(v & 0xff) / 255
        } else {
            r = Double((v >> 16) & 0xff) / 255; g = Double((v >> 8) & 0xff) / 255
            b = Double(v & 0xff) / 255; a = 1
        }
        return Color(red: r, green: g, blue: b, opacity: a)
    }

    /// Footer label for a binding: `⌘⇧P`, `Tab`, `` ` ``.
    static func keyLabel(_ b: KeyBinding?, _ fallback: String) -> String {
        guard let b else { return fallback }
        var out = ""
        if b.mods.contains("ctrl") { out += "⌃" }
        if b.mods.contains("alt") { out += "⌥" }
        if b.mods.contains("shift") { out += "⇧" }
        if b.mods.contains("cmd") { out += "⌘" }
        switch b.key {
        case "tab": out += "Tab"
        case "return": out += "↩"
        case "escape": out += "Esc"
        case "space": out += "Space"
        case "up": out += "↑"
        case "down": out += "↓"
        case "left": out += "←"
        case "right": out += "→"
        case "backspace": out += "⌫"
        case "delete": out += "⌦"
        default: out += b.key.uppercased()
        }
        return out
    }

    /// Accessibility → Display → Reduce transparency.
    static var reduceTransparency: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
    }

    /// Opaque fallback when glass is off (Reduce transparency or solid toast contrast).
    static func island(_ scheme: ColorScheme) -> Color {
        if let hex = scheme == .dark ? file.panelDark : file.panelLight, let c = color(hex: hex) { return c }
        return scheme == .dark ? rgb(0x1c1c1e) : rgb(0xf5f5f7)
    }

    static let spotlightRadius: CGFloat = 22
    static let islandBottomRadius: CGFloat = 18

    /// Semantic ink under aqua / darkAqua so Appearance: Light never keeps dark-mode gray.
    static func primary(_ scheme: ColorScheme) -> Color {
        if let hex = scheme == .dark ? file.textDark : file.textLight, let c = color(hex: hex) { return c }
        return Color(nsColor: semanticLabel(.label, scheme: scheme))
    }

    static func secondary(_ scheme: ColorScheme) -> Color {
        Color(nsColor: semanticLabel(.secondary, scheme: scheme))
    }

    static func tertiary(_ scheme: ColorScheme) -> Color {
        Color(nsColor: semanticLabel(.tertiary, scheme: scheme))
    }

    static func stroke(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color.white.opacity(0.14) : Color.black.opacity(0.18)
    }

    static func kbd(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color.white.opacity(0.09) : Color.black.opacity(0.06)
    }

    static let danger = Color(red: Double(0xd9) / 255, green: Double(0x38) / 255, blue: Double(0x38) / 255)

    /// Resolve light/dark for paints. Prefer the in-app Appearance setting.
    /// For `system`, read `NSApp.effectiveAppearance` — accessory apps often
    /// keep a stale `@Environment(\.colorScheme)`.
    static func scheme(preference: String, system: ColorScheme) -> ColorScheme {
        switch preference {
        case "light": .light
        case "dark": .dark
        default: systemAppearanceScheme() ?? system
        }
    }

    static func systemAppearanceScheme() -> ColorScheme? {
        let match = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua])
        switch match {
        case .darkAqua: return .dark
        case .aqua: return .light
        default: return nil
        }
    }

    /// AppKit appearance for the island / Seek panels.
    static func nsAppearance(preference: String) -> NSAppearance? {
        switch preference {
        case "light": NSAppearance(named: .aqua)
        case "dark": NSAppearance(named: .darkAqua)
        default: nil
        }
    }

    /// SF Symbol for a command, or nil when the row should draw a literal glyph
    /// (center third draws U+2153). Id wins so shared emoji glyphs stay distinct.
    static func symbol(for command: Command) -> String? {
        if let byId = byId[command.id] {
            return byId.isEmpty ? nil : byId
        }
        if command.id.hasPrefix("layout:restore:") {
            return "square.stack.3d.up.fill"
        }
        if command.id.hasPrefix("quit:app:") {
            return "xmark.circle.fill"
        }
        if command.id.hasPrefix("emoji:") {
            return nil // keep the emoji glyph itself
        }
        if command.id.hasPrefix("note:") {
            return "note.text"
        }
        // `[[commands]] icon = "safari"`: a bare symbol name (letters, dots) is an SF Symbol.
        if let icon = command.icon, icon.count > 1,
           icon.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) || $0 == "." })
        {
            return icon
        }
        switch command.kind {
        case .file: return "doc.fill"
        case .snippet: return "text.quote"
        case .clipboard: return "clipboard.fill"
        case .app: return "app.fill"
        case .ai: return "sparkles"
        case .script: return "terminal.fill"
        case .quicklink: return "link"
        case .calculator: return "plus.forwardslash.minus"
        case .window: return "rectangle.split.2x1"
        case .layout: return "square.stack.3d.up.fill"
        case .shortcut: return "square.2.layers.3d.fill"
        case .mcp: return "puzzlepiece.extension.fill"
        case .builtin: break
        }
        guard let icon = command.icon else { return nil }
        // Drop U+FE0F / U+FE0E so ⚙️ and ⚙ (and ☀️/☀) share one row.
        let key = String(icon.unicodeScalars.filter {
            $0.value != 0xfe0f && $0.value != 0xfe0e
        })
        return glyphs[key]
    }

    /// Symbol names already checked against the system; `NSImage(systemSymbolName:)`
    /// per row per frame was a measurable cost on the search list.
    private static var symbolExists: [String: Bool] = [:]

    /// Resolved SF Symbol name if the system has it; else nil (caller draws emoji).
    static func resolvedSymbol(for command: Command) -> String? {
        guard let name = symbol(for: command) else { return nil }
        if let known = symbolExists[name] { return known ? name : nil }
        let exists = NSImage(systemSymbolName: name, accessibilityDescription: nil) != nil
        symbolExists[name] = exists
        return exists ? name : nil
    }

    private static func rgb(_ hex: UInt32) -> Color {
        Color(
            red: Double((hex >> 16) & 0xff) / 255,
            green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255
        )
    }

    private enum LabelKind { case label, secondary, tertiary }

    private static func semanticLabel(_ kind: LabelKind, scheme: ColorScheme) -> NSColor {
        let named: NSAppearance.Name = scheme == .dark ? .darkAqua : .aqua
        guard let appearance = NSAppearance(named: named) else { return .labelColor }
        var cg: CGColor?
        appearance.performAsCurrentDrawingAppearance {
            let ink: NSColor = switch kind {
            case .label: .labelColor
            case .secondary: .secondaryLabelColor
            case .tertiary: .tertiaryLabelColor
            }
            cg = ink.cgColor
        }
        return cg.flatMap(NSColor.init(cgColor:)) ?? .labelColor
    }

    /// Empty string = intentionally show the literal (center third).
    private static let byId: [String: String] = [
        // Builtins — prefer filled glyphs when the system has them
        "builtin:chat": "bubble.left.and.bubble.right.fill",
        "builtin:reload": "arrow.clockwise",
        "builtin:config": "folder.fill",
        "builtin:scripts": "folder.badge.gearshape",
        "builtin:quit": "power",
        "builtin:theme-system": "desktopcomputer",
        "builtin:theme-light": "sun.max.fill",
        "builtin:theme-dark": "moon.stars.fill",
        "builtin:presentation-island": "rectangle.topthird.inset.filled",
        "builtin:presentation-spotlight": "circle.dashed",
        "builtin:settings": "gearshape.fill",
        "builtin:shortcuts": "keyboard.fill",
        "builtin:stay-awake-off": "zzz",
        "builtin:stay-awake-indefinite": "cup.and.saucer.fill",
        "builtin:stay-awake-30m": "cup.and.saucer.fill",
        "builtin:stay-awake-1h": "cup.and.saucer.fill",
        "builtin:stay-awake-3h": "cup.and.saucer.fill",
        "builtin:clipboard": "clipboard.fill",
        "builtin:screenshots": "photo.on.rectangle.angled",
        "builtin:paste-latest-screenshot": "photo.badge.arrow.down.fill",
        "builtin:emoji": "face.smiling.fill",
        "builtin:notes": "note.text",
        "builtin:new-note": "square.and.pencil",
        "builtin:dark-mode": "circle.lefthalf.filled",
        "builtin:volume-up": "speaker.wave.3.fill",
        "builtin:volume-down": "speaker.wave.1.fill",
        "builtin:volume-mute": "speaker.slash.fill",
        "builtin:brightness-up": "sun.max.fill",
        "builtin:brightness-down": "sun.min.fill",
        "builtin:empty-trash": "trash.fill",
        "builtin:lock-screen": "lock.fill",
        "builtin:sleep": "moon.zzz.fill",
        "builtin:eject-all": "eject.fill",
        "builtin:quit-front": "xmark.circle.fill",
        "builtin:kill-front": "xmark.octagon.fill",
        // Seek / layouts
        "seek": "viewfinder",
        "layout:remember": "camera.viewfinder",
        "layout:forget": "trash.fill",
        // Shortcuts
        "shortcut:return": "return",
        "shortcut:tab": "arrow.right.to.line.compact",
        "shortcut:backtick": "viewfinder",
        "shortcut:esc": "escape",
        "shortcut:arrows": "arrow.up.arrow.down",
        // Window
        "window:left-half": "rectangle.lefthalf.filled",
        "window:right-half": "rectangle.righthalf.filled",
        "window:top-half": "rectangle.tophalf.filled",
        "window:bottom-half": "rectangle.bottomhalf.filled",
        "window:top-left": "rectangle.inset.topleft.filled",
        "window:top-right": "rectangle.inset.topright.filled",
        "window:bottom-left": "rectangle.inset.bottomleft.filled",
        "window:bottom-right": "rectangle.inset.bottomright.filled",
        "window:first-third": "rectangle.leadingthird.inset.filled",
        "window:center-third": "", // literal ⅓
        "window:last-third": "rectangle.trailingthird.inset.filled",
        "window:maximize": "arrow.up.left.and.arrow.down.right",
        "window:almost-maximize": "rectangle.inset.filled",
        "window:center": "circle.circle.fill",
        "window:next-display": "display",
        "window:previous-display": "display.2",
        "window:restore": "arrow.uturn.backward",
    ]

    private static let glyphs: [String: String] = [
        "\u{1F4AC}": "bubble.left.and.bubble.right.fill", // 💬
        "\u{1F504}": "arrow.clockwise", // 🔄
        "\u{1F4C1}": "folder.fill", // 📁
        "\u{1F4C2}": "folder.fill", // 📂
        "\u{23FB}": "power", // ⏻
        "\u{1F5A5}": "desktopcomputer", // 🖥️
        "\u{2600}": "sun.max.fill", // ☀
        "\u{1F319}": "moon.stars.fill", // 🌙
        "\u{1F3DD}": "rectangle.topthird.inset.filled", // 🏝
        "\u{1F526}": "flashlight.on.fill", // 🔦
        "\u{2699}": "gearshape.fill", // ⚙
        "\u{2328}": "keyboard.fill", // ⌨
        "\u{1F4A4}": "zzz", // 💤
        "\u{2615}": "cup.and.saucer.fill", // ☕
        "\u{1F4CB}": "clipboard.fill", // 📋
        "\u{1F600}": "face.smiling.fill", // 😀
        "\u{1F4DD}": "note.text", // 📝
        "\u{1F312}": "circle.lefthalf.filled", // 🌗
        "\u{1F50A}": "speaker.wave.3.fill", // 🔊
        "\u{1F509}": "speaker.wave.1.fill", // 🔉
        "\u{1F507}": "speaker.slash.fill", // 🔇
        "\u{1F324}": "sun.min.fill", // 🌤
        "\u{1F5D1}": "trash.fill", // 🗑
        "\u{1F512}": "lock.fill", // 🔒
        "\u{1F634}": "moon.zzz.fill", // 😴
        "\u{1F4BF}": "opticaldisc", // 💿
        "\u{2715}": "xmark", // ✕
        "\u{2620}": "xmark.octagon.fill", // ☠
        "\u{1F3AF}": "viewfinder", // 🎯
        "\u{1F4F8}": "camera.viewfinder", // 📸
        "\u{1F5C2}": "square.stack.3d.up.fill", // 🗂
        "\u{2728}": "sparkles", // ✨
        "\u{1F4DC}": "scroll.fill", // 📜
        "\u{1F517}": "link", // 🔗
        "\u{2702}": "scissors", // ✂
        "\u{1F4C4}": "doc.fill", // 📄
        "\u{1F5BC}": "photo.fill", // 🖼
        "\u{1F321}": "thermometer.medium", // 🌡
        "\u{25E7}": "rectangle.lefthalf.filled",
        "\u{25E8}": "rectangle.righthalf.filled",
        "\u{2B12}": "rectangle.tophalf.filled",
        "\u{2B13}": "rectangle.bottomhalf.filled",
        "\u{25F0}": "rectangle.inset.topleft.filled",
        "\u{25F3}": "rectangle.inset.topright.filled",
        "\u{25F1}": "rectangle.inset.bottomleft.filled",
        "\u{25F2}": "rectangle.inset.bottomright.filled",
        "\u{26F6}": "arrow.up.left.and.arrow.down.right",
        "\u{25CE}": "circle.circle.fill",
        "\u{25A3}": "rectangle.inset.filled",
        "\u{2192}": "arrow.right",
        "\u{2190}": "arrow.left",
        "\u{21A9}": "arrow.uturn.backward",
        "\u{21B5}": "return", // ↩
        "\u{21E5}": "arrow.right.to.line.compact", // ⇥
        "\u{238B}": "escape", // ⎋
        "\u{1F9EE}": "plus.forwardslash.minus", // 🧮
        "\u{1F418}": "globe", // 🐙 used in quicklinks — keep link via kind
        "\u{1F50D}": "magnifyingglass", // 🔎
    ]
}
