import AppKit

/// Titled windows from an LSUIElement / `.accessory` app need this or
/// `makeKeyAndOrderFront` is a no-op on Tahoe (window never appears).
final class SettingsKeyWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

/// How Settings must come on screen. Isolated from Engine / SwiftUI so
/// `./test-appkit.command` can actually create windows and fail the release.
enum SettingsPresentation {
    /// Launcher / Click Text overlays sit at 25 (status bar). A normal titled
    /// window is level 0 and opens *behind* them when the island is still up.
    static let islandLevel = 25
    static var windowLevel: NSWindow.Level { NSWindow.Level(rawValue: islandLevel + 1) }

    /// `handleUi("settings")` calls `hide()` first, which may `NSApp.hide` the
    /// whole accessory app. Unhide + regular policy before keying the window.
    static func prepareApp(_ app: NSApplication = NSApp) {
        app.unhide(nil)
        if app.activationPolicy() != .regular {
            app.setActivationPolicy(.regular)
        }
        app.activate(ignoringOtherApps: true)
    }

    static func present(_ window: NSWindow, hiding islands: [NSWindow] = []) {
        islands.forEach { $0.orderOut(nil) }
        window.level = windowLevel
        window.collectionBehavior.insert(.moveToActiveSpace)
        window.collectionBehavior.insert(.fullScreenAuxiliary)
        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
    }
}
