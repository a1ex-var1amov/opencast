import SwiftUI

enum Theme {
    static let accent = Color(red: 1, green: Double(0x6b) / 255, blue: Double(0x3d) / 255)

    static func island(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? .black : rgb(0xfafafb)
    }

    static func primary(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? rgb(0xececf0) : rgb(0x17171a)
    }

    static func secondary(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? rgb(0x9d9da6) : rgb(0x6b6b73)
    }

    static func tertiary(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? rgb(0x6d6d76) : rgb(0x9a9aa3)
    }

    static func stroke(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color.white.opacity(0.14) : Color.black.opacity(0.18)
    }

    static func shadow(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color.black.opacity(0.55) : Color.black.opacity(0.28)
    }

    static func kbd(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color.white.opacity(0.09) : Color.black.opacity(0.06)
    }

    static let danger = Color(red: Double(0xd9) / 255, green: Double(0x38) / 255, blue: Double(0x38) / 255)

    /// SF Symbol for a command, or nil when the row should draw `command.icon`
    /// (center third draws U+2153). Id wins over the icon character so the two
    /// side thirds don't collapse to the fraction glyph they share.
    static func symbol(for command: Command) -> String? {
        switch command.id {
        case "window:center-third": return nil
        case "window:first-third": return "rectangle.leadingthird.inset.filled"
        case "window:last-third": return "rectangle.trailingthird.inset.filled"
        default: break
        }
        guard let icon = command.icon else { return nil }
        // ponytail: drop U+FE0F so ⚙️ and ⚙ share one row; add a presentation-selector table if a glyph needs both
        let key = String(icon.unicodeScalars.filter { $0.value != 0xfe0f })
        return icons[key]
    }

    private static func rgb(_ hex: UInt32) -> Color {
        Color(
            red: Double((hex >> 16) & 0xff) / 255,
            green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255
        )
    }

    private static let icons: [String: String] = [
        "\u{1F4AC}": "bubble.left",
        "\u{1F504}": "arrow.clockwise",
        "\u{2699}": "gearshape",
        "\u{1F4C2}": "folder",
        "\u{23FB}": "power",
        "\u{25E7}": "rectangle.lefthalf.filled",
        "\u{25E8}": "rectangle.righthalf.filled",
        "\u{2B12}": "rectangle.tophalf.filled",
        "\u{2B13}": "rectangle.bottomhalf.filled",
        "\u{25F0}": "rectangle.inset.topleft.filled",
        "\u{25F3}": "rectangle.inset.topright.filled",
        "\u{25F1}": "rectangle.inset.bottomleft.filled",
        "\u{25F2}": "rectangle.inset.bottomright.filled",
        "\u{26F6}": "arrow.up.left.and.arrow.down.right",
        "\u{25CE}": "circle.circle",
        "\u{1F3AF}": "viewfinder",
        "\u{1F4F8}": "camera",
        "\u{1F5C2}": "square.stack",
        "\u{1F5D1}": "trash",
        "\u{2728}": "sparkles",
        "\u{1F4DC}": "scroll",
        "\u{1F517}": "link",
    ]
}
