import Foundation

/// Rules for OpenCast status-item glyphs. AppKit-free so tests lock the policy
/// that kept icons black on Tahoe's dark menu bar.
enum MenuBarSymbol {
    /// Status items must not set `contentTintColor` to `.labelColor` /
    /// `.secondaryLabelColor` — those follow the *app* appearance and stay black
    /// while Control Center icons are white on a dark menu bar.
    static let forcesAppLabelTint = false

    /// SF Symbols that ship multicolor and ignore template ink unless monochrome.
    static func needsMonochrome(_ systemName: String) -> Bool {
        switch systemName {
        case "sparkles": true
        default: false
        }
    }
}
