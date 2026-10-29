import Foundation

/// Overlay / HUD panel rules. AppKit-free so panel tests lock Tahoe drawing policy.
enum OverlayPanelPolicy {
    /// Fully `NSColor.clear` backing on Tahoe can drop SwiftUI fills in a clear
    /// `NSHostingView`. A hair of alpha keeps the compositor drawing hits / HUDs.
    static let backingAlpha = 0.001

    /// Accessory overlays must use `orderFrontRegardless` — plain `orderFront`
    /// is a no-op when the app is not active (Click Text on Tahoe).
    static let orderFrontRegardless = true
}
