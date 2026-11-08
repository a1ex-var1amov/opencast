import AppKit
import Foundation

/// Rules for OpenCast status-item glyphs. Image building lives here so Stay Awake /
/// Focus / OpenCast share one path; the Bool/string policy is what panel tests lock.
enum MenuBarSymbol {
    /// Status items must not set `contentTintColor` to `.labelColor` /
    /// `.secondaryLabelColor` — those follow the *app* appearance and stay black
    /// while Control Center icons are white on a dark menu bar.
    static let forcesAppLabelTint = false

    /// After an AppKit menu hop, `Task { @MainActor }` is dropped on Tahoe.
    /// Settings uses a non-MainActor controller; Model hops via `DispatchQueue.main`.
    static let usesTaskAfterMenu = false

    /// `MainActor.assumeIsolated` after `perform(afterDelay:)` traps on Tahoe —
    /// the callback is main-thread AppKit, not the MainActor executor.
    static let usesAssumeIsolatedAfterMenu = false

    /// SF Symbols that ship multicolor and ignore template ink unless monochrome.
    static func needsMonochrome(_ systemName: String) -> Bool {
        switch systemName {
        case "sparkles": true
        default: false
        }
    }

    /// Prefer these when the primary SF Symbol is missing on a given OS.
    static func fallbacks(for systemName: String) -> [String] {
        switch systemName {
        case "sparkle": ["sparkles", "star.fill", "star"]
        case "sparkles": ["star.fill", "star"]
        case "circle.fill": ["circle"]
        case "target": ["scope", "circle"]
        case "pause.fill": ["pause", "circle.fill"]
        default: []
        }
    }

    /// Template image the menu bar can tint. Never returns an empty zero-size image.
    static func templateImage(systemName: String, accessibilityDescription: String, pointSize: CGFloat = 15) -> NSImage {
        let names = [systemName] + fallbacks(for: systemName)
        for name in names {
            if let image = configured(name, pointSize: pointSize, mono: needsMonochrome(name)) {
                image.isTemplate = true
                image.accessibilityDescription = accessibilityDescription
                return image
            }
            if needsMonochrome(name), let image = configured(name, pointSize: pointSize, mono: false) {
                image.isTemplate = true
                image.accessibilityDescription = accessibilityDescription
                return image
            }
        }
        // Last resort so NSStatusItem never collapses to zero width (invisible click target).
        let fallback = NSImage(size: NSSize(width: pointSize, height: pointSize))
        fallback.isTemplate = true
        fallback.accessibilityDescription = accessibilityDescription
        return fallback
    }

    /// The one OpenCast status glyph: a single large sparkle. Outline when idle,
    /// filled while Stay Awake is on. SF Symbols has no hollow sparkle, so the
    /// outline is derived: the glyph dilated by `stroke`, minus the glyph itself.
    /// Both states share one canvas size so the item never shifts.
    static func statusImage(awake: Bool) -> NSImage {
        let glyph = templateImage(systemName: "sparkle", accessibilityDescription: "OpenCast", pointSize: 16)
        let stroke: CGFloat = 1.4
        let size = NSSize(width: glyph.size.width + 2 * stroke, height: glyph.size.height + 2 * stroke)
        let image = NSImage(size: size, flipped: false) { _ in
            let origin = NSPoint(x: stroke, y: stroke)
            if awake {
                glyph.draw(at: origin, from: .zero, operation: .sourceOver, fraction: 1)
            } else {
                for i in 0..<16 {
                    let a = CGFloat(i) / 16 * 2 * .pi
                    let p = NSPoint(x: origin.x + stroke * cos(a), y: origin.y + stroke * sin(a))
                    glyph.draw(at: p, from: .zero, operation: .sourceOver, fraction: 1)
                }
                glyph.draw(at: origin, from: .zero, operation: .destinationOut, fraction: 1)
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "OpenCast"
        return image
    }

    private static func configured(_ name: String, pointSize: CGFloat, mono: Bool) -> NSImage? {
        var conf = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .medium)
        if mono { conf = conf.applying(.preferringMonochrome()) }
        guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(conf)
        else { return nil }
        // Some OS / config combos yield a blank rep — treat as missing.
        guard image.size.width > 0, image.size.height > 0 else { return nil }
        return image
    }
}
