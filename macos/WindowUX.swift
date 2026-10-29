import AppKit
import SwiftUI

// MARK: - Region glyphs

/// A small screen with the target region filled: the icon of every window
/// command, the HUD and the Arrange Window picker.
struct WindowGlyph: View {
    /// Region on a unit screen (0…1).
    var region: WindowManager.Rect
    var size: CGFloat = 28
    var tint: Color = .accentColor
    /// Draw a rotated (tall) screen.
    var portrait = false

    var body: some View {
        let w = portrait ? size * 0.7 : size, h = portrait ? size : size * 0.7
        ZStack(alignment: .topLeading) {
            RoundedRectangle(cornerRadius: size * 0.12, style: .continuous)
                .stroke(Color.primary.opacity(0.55), lineWidth: max(1, size / 22))
                .frame(width: w, height: h)
            RoundedRectangle(cornerRadius: size * 0.06, style: .continuous)
                .fill(tint)
                .frame(width: max(2, region.w * (w - size * 0.16)), height: max(2, region.h * (h - size * 0.16)))
                .offset(x: size * 0.08 + region.x * (w - size * 0.16), y: size * 0.08 + region.y * (h - size * 0.16))
        }
        .frame(width: w, height: h)
        .frame(width: size, height: size)
    }
}

@MainActor
enum WindowGlyphs {
    /// Command id (`window:left-half`) → region, loaded from the engine once.
    private(set) static var regions: [String: WindowManager.Rect] = [:]
    private static var asked = Set<String>()
    /// Glyphs are drawn for the screen the target window is on; a rotated
    /// monitor gets tall icons with thirds as rows.
    private(set) static var portrait = false

    /// Regions for window command ids not seen yet (one engine call per batch).
    static func load(_ ids: [String], portrait: Bool? = nil) {
        if let portrait, portrait != Self.portrait {
            reset()
            Self.portrait = portrait
        }
        let want = ids.filter { $0.hasPrefix("window:") && !asked.contains($0) }
        guard !want.isEmpty else { return }
        want.forEach { asked.insert($0) }
        let keys = want.map { String($0.dropFirst("window:".count)) }
        let r = Engine.call(["op": "windowGlyphs", "actions": keys, "portrait": Self.portrait])
        for (k, v) in r["glyphs"] as? [String: [String: Any]] ?? [:] {
            if let rect = WindowManager.Rect(json: v) { regions["window:" + k] = rect }
        }
    }

    static func reset() {
        regions = [:]
        asked = []
    }

    /// PNG of a glyph (for grid rows, which show image icons), cached on disk.
    static func imagePath(for id: String, size: CGFloat = 96) -> String? {
        guard let region = regions[id] else { return nil }
        let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("OpenCast/glyphs", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let key = String(format: "%@-%@-%.3f-%.3f-%.3f-%.3f", id.replacingOccurrences(of: ":", with: "_"), portrait ? "p" : "l", region.x, region.y, region.w, region.h)
        let url = dir.appendingPathComponent(key + ".png")
        if FileManager.default.fileExists(atPath: url.path) { return url.path }
        let renderer = ImageRenderer(content: WindowGlyph(region: region, size: size, tint: Color(nsColor: .controlAccentColor), portrait: portrait).padding(4))
        renderer.scale = 2
        guard let image = renderer.nsImage, let tiff = image.tiffRepresentation,
              let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { return nil }
        try? png.write(to: url)
        return url.path
    }
}

// MARK: - HUD

/// Brief "Left Half" HUD when a hotkey moves a window (`window_hud`).
@MainActor
final class WindowHUD {
    static let shared = WindowHUD()
    private var panel: NSPanel?
    private var hideTask: Task<Void, Never>?

    func show(action: String, on frame: WindowManager.Rect?) {
        let id = "window:" + action
        let tall = frame.flatMap { f in NSScreen.screens.first { $0.frame.intersects(f.appKitFrame) } }.map { $0.frame.height > $0.frame.width * 1.05 }
        WindowGlyphs.load([id], portrait: tall)
        let title = Self.title(for: action)
        let view = HStack(spacing: 10) {
            if let region = WindowGlyphs.regions[id] {
                WindowGlyph(region: region, size: 34, portrait: WindowGlyphs.portrait)
            }
            Text(title).font(.system(size: 15, weight: .semibold))
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        let host = NSHostingView(rootView: view)
        let size = host.fittingSize
        let screen = frame.flatMap { f in NSScreen.screens.first { $0.frame.intersects(f.appKitFrame) } } ?? NSScreen.main
        guard let vf = screen?.visibleFrame else { return }
        let rect = NSRect(x: vf.midX - size.width / 2, y: vf.minY + vf.height * 0.12, width: size.width, height: size.height)
        let p = panel ?? {
            let p = NSPanel(contentRect: rect, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
            p.level = .statusBar
            p.isOpaque = false
            p.backgroundColor = NSColor.black.withAlphaComponent(OverlayPanelPolicy.backingAlpha)
            p.ignoresMouseEvents = true
            p.hasShadow = true
            p.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
            p.sharingType = .none
            return p
        }()
        p.contentView = host
        p.setFrame(rect, display: true)
        p.alphaValue = 1
        precondition(OverlayPanelPolicy.orderFrontRegardless)
        p.orderFrontRegardless()
        panel = p
        hideTask?.cancel()
        hideTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 700_000_000)
            guard !Task.isCancelled else { return }
            NSAnimationContext.runAnimationGroup({ $0.duration = 0.2; p.animator().alphaValue = 0 }, completionHandler: {
                MainActor.assumeIsolated { p.orderOut(nil) }
            })
        }
    }

    static func title(for action: String) -> String {
        let key = action.hasPrefix("custom:") ? String(action.dropFirst(7)) : action
        let spaces = [
            "space-next": "Next Space", "space-prev": "Previous Space",
            "space-move-next": "Window → Next Space", "space-move-prev": "Window → Previous Space",
        ]
        if let name = spaces[key] { return name }
        return key.split(separator: "-").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }
}

// MARK: - Live preview

/// A translucent frame on the real screen showing where the highlighted window
/// command (or a drag to an edge) will put the window.
@MainActor
final class WindowPreview {
    static let shared = WindowPreview()
    private var window: NSWindow?
    private(set) var shown: WindowManager.Rect?

    func show(_ r: WindowManager.Rect) {
        if let shown, shown.near(r, 0.5), window?.isVisible == true { return }
        shown = r
        let w = window ?? {
            let w = NSWindow(contentRect: r.appKitFrame, styleMask: .borderless, backing: .buffered, defer: false)
            w.isOpaque = false
            w.backgroundColor = NSColor.black.withAlphaComponent(OverlayPanelPolicy.backingAlpha)
            w.ignoresMouseEvents = true
            w.level = .floating
            w.hasShadow = false
            w.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
            w.sharingType = .none
            w.contentView = NSHostingView(rootView:
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Color(nsColor: .controlAccentColor).opacity(0.18))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .stroke(Color(nsColor: .controlAccentColor).opacity(0.85), lineWidth: 3))
                    .padding(2)
            )
            return w
        }()
        let animate = window?.isVisible == true && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        if animate {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.12
                w.animator().setFrame(r.appKitFrame, display: true)
            }
        } else {
            w.setFrame(r.appKitFrame, display: true)
        }
        precondition(OverlayPanelPolicy.orderFrontRegardless)
        w.orderFrontRegardless()
        window = w
    }

    func hide() {
        shown = nil
        window?.orderOut(nil)
    }
}

// MARK: - Drag to snap

/// `drag_snap = true`: drag a window to a screen edge or corner and let go to
/// snap it. Edges = halves, corners = quarters, top = maximize, ⌥ on the
/// left/right edge = thirds. Dragging a window we snapped restores its size.
@MainActor
final class DragSnap {
    static let shared = DragSnap()
    private var monitors: [Any] = []
    private var dragging: (window: AXUIElement, start: WindowManager.Rect)?
    private var moved = false
    private var zone: String?
    private var lastCheck: TimeInterval = 0

    /// macOS 15+ tiles windows dragged to edges itself; never fight it.
    static var systemTilingOn: Bool {
        let d = UserDefaults(suiteName: "com.apple.WindowManager")
        guard ProcessInfo.processInfo.isOperatingSystemAtLeast(OperatingSystemVersion(majorVersion: 15, minorVersion: 0, patchVersion: 0)) else { return false }
        return d?.object(forKey: "EnableTilingByEdgeDrag") as? Bool ?? true
    }

    func setEnabled(_ on: Bool) {
        monitors.forEach { NSEvent.removeMonitor($0) }
        monitors = []
        guard on, !Self.systemTilingOn else { return }
        if let m = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp], handler: { e in
            MainActor.assumeIsolated { DragSnap.shared.handle(e) }
        }) {
            monitors.append(m)
        }
    }

    private func handle(_ e: NSEvent) {
        switch e.type {
        case .leftMouseDown:
            moved = false
            zone = nil
            dragging = WindowManager.frontWindow().map { ($0.0, $0.1) }
        case .leftMouseDragged:
            guard let d = dragging, e.timestamp - lastCheck > 0.04 else { return }
            lastCheck = e.timestamp
            if !moved {
                // Only window drags (the window's origin follows the mouse), not selections.
                guard let now = WindowManager.frontWindow(), CFEqual(now.0, d.window),
                      abs(now.1.x - d.start.x) > 2 || abs(now.1.y - d.start.y) > 2 else { return }
                moved = true
                // Rectangle-style: a snapped window dragged away gets its old size back.
                if let s = WindowManager.snappedFrame(of: d.window), now.1.w.rounded() == s.snapped.w.rounded() {
                    let mouse = Self.mouseTopLeft()
                    let rel = (mouse.x - now.1.x) / max(now.1.w, 1)
                    WindowManager.move(d.window, to: WindowManager.Rect(x: mouse.x - rel * s.before.w, y: now.1.y, w: s.before.w, h: s.before.h))
                    WindowManager.forgetSnap(d.window)
                }
            }
            zone = Self.zone(at: Self.mouseTopLeft(), option: e.modifierFlags.contains(.option))
            if let zone, let now = WindowManager.frontWindow(), let r = WindowManager.target(action: zone, current: now.1, cycle: 0) {
                WindowPreview.shared.show(r)
            } else {
                WindowPreview.shared.hide()
            }
        case .leftMouseUp:
            defer {
                dragging = nil
                zone = nil
                WindowPreview.shared.hide()
            }
            guard moved, let zone else { return }
            _ = try? WindowManager.apply(action: zone, frac: nil, app: NSWorkspace.shared.frontmostApplication)
        default:
            break
        }
    }

    private static func mouseTopLeft() -> CGPoint {
        let p = NSEvent.mouseLocation
        let primary = NSScreen.screens.first?.frame.maxY ?? 0
        return CGPoint(x: p.x, y: primary - p.y)
    }

    /// Which command an edge or corner means, or nil away from the edges.
    static func zone(at p: CGPoint, option: Bool) -> String? {
        guard let s = WindowManager.screenFrame(containing: p) else { return nil }
        let edge: CGFloat = 6, corner: CGFloat = 60
        let left = p.x - s.x < edge, right = s.x + s.w - p.x < edge + 1
        let top = p.y - s.y < edge, bottom = s.y + s.h - p.y < edge + 1
        let nearTop = p.y - s.y < corner, nearBottom = s.y + s.h - p.y < corner
        let nearLeft = p.x - s.x < corner, nearRight = s.x + s.w - p.x < corner
        if (left && nearTop) || (top && nearLeft) { return "top-left" }
        if (right && nearTop) || (top && nearRight) { return "top-right" }
        if (left && nearBottom) || (bottom && nearLeft) { return "bottom-left" }
        if (right && nearBottom) || (bottom && nearRight) { return "bottom-right" }
        if s.h > s.w * 1.05 {
            // Rotated monitor: windows stack. Top/bottom edge = halves (⌥ = thirds).
            if top { return option ? "top-third" : "top-half" }
            if bottom { return option ? "bottom-third" : "bottom-half" }
            if left { return "left-half" }
            if right { return "right-half" }
            return nil
        }
        if left { return option ? "first-third" : "left-half" }
        if right { return option ? "last-third" : "right-half" }
        if top { return "maximize" }
        if bottom { return "bottom-half" }
        return nil
    }
}
