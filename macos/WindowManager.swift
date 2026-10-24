import AppKit
import ApplicationServices

/// Window management through the Accessibility API: snapping, display moves,
/// layouts, the front app's selected text. Nothing here spawns a process, so
/// a snap takes a few milliseconds instead of a few hundred.
///
/// Needs System Settings → Privacy & Security → Accessibility → OpenCast.
@MainActor
enum WindowManager {
    struct Rect: Equatable {
        var x: Double
        var y: Double
        var w: Double
        var h: Double
    }


    // MARK: - Public API

    static var accessibilityTrusted: Bool { AXIsProcessTrusted() }

    /// Short; actionable toast is built by `permissionToast` at the catch site.
    nonisolated static let permissionHint = "Accessibility is required to move windows"

    /// The app that had focus before the panel came up. Call before activating.
    static func frontApp() -> NSRunningApplication? {
        let front = NSWorkspace.shared.frontmostApplication
        if front?.processIdentifier == ProcessInfo.processInfo.processIdentifier {
            return nil
        }
        return front
    }

    /// Selected text in `app`'s focused UI element (AX), or nil.
    static func selectedText(of app: NSRunningApplication) -> String? {
        guard accessibilityTrusted else { return nil }
        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(appElement, kAXFocusedUIElementAttribute as CFString, &focused) == .success,
              let element = focused
        else { return nil }
        var value: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(element as! AXUIElement, kAXSelectedTextAttribute as CFString, &value)
        guard status == .success, let text = value as? String, !text.isEmpty else { return nil }
        return text
    }

    /// The last snap, so repeating a command on the same window cycles sizes.
    private static var lastSnap: (window: AXUIElement, action: String, cycle: Int, target: Rect)?
    /// Frames before each move, per window (Undo), newest last.
    private static var history: [(window: AXUIElement, frames: [Rect])] = []
    /// The frame before the first snap, per window (Restore, drag-away).
    private static var preSnap: [(window: AXUIElement, frame: Rect)] = []

    /// Snap/move the frontmost window of `app` (or the front app). Geometry comes
    /// from the engine (`window::target`): gaps, cycling, display moves, custom commands.
    /// Returns the frame it was moved to (nil for undo/restore/fullscreen).
    @discardableResult
    static func apply(action: String, frac: [Double]?, app: NSRunningApplication? = nil) throws -> Rect? {
        let started = CFAbsoluteTimeGetCurrent()
        defer {
            let ms = Int((CFAbsoluteTimeGetCurrent() - started) * 1000)
            NSLog("opencast: window %@ on %@ took %d ms", action, (app ?? frontApp())?.localizedName ?? "?", ms)
        }
        // Spaces are SkyLight state: changing the view needs no AX window.
        if action == "space-next" || action == "space-prev" {
            try Spaces.switchRelative(action == "space-next" ? 1 : -1)
            return nil
        }
        guard accessibilityTrusted else { throw WMError.permission }
        guard let app = app ?? frontApp() else { throw WMError.message("no frontmost app") }
        let pid = app.processIdentifier
        guard let window = focusedWindow(pid: pid) else {
            throw WMError.message("the front app (\(app.localizedName ?? "?")) has no window")
        }
        guard let current = frame(of: window) else { throw WMError.message("could not read the window frame") }

        // Moving a window between spaces is SkyLight's operation, not a frame change.
        if action == "space-move-next" || action == "space-move-prev" {
            try Spaces.move(window, relative: action == "space-move-next" ? 1 : -1)
            return nil
        }

        switch action {
        case "fullscreen", "full-screen", "exit-fullscreen":
            let on = boolAttribute(window, "AXFullScreen")
            let want: Bool = switch action {
            case "full-screen": true
            case "exit-fullscreen": false
            default: !on
            }
            if action != "fullscreen" && want == on {
                let why = if want { "that window is already full screen" } else { "that window is not full screen" }
                throw WMError.message(why)
            }
            AXUIElementSetAttributeValue(window, "AXFullScreen" as CFString, want ? kCFBooleanTrue : kCFBooleanFalse)
            return nil
        case "undo":
            guard let i = history.firstIndex(where: { CFEqual($0.window, window) }), let prev = history[i].frames.popLast() else {
                throw WMError.message("nothing to undo for this window")
            }
            if history[i].frames.isEmpty { history.remove(at: i) }
            lastSnap = nil
            try setFrame(window, prev)
            return nil
        case "restore":
            guard let i = preSnap.firstIndex(where: { CFEqual($0.window, window) }) else {
                throw WMError.message("this window has not been snapped")
            }
            let f = preSnap.remove(at: i).frame
            remember(window, current)
            lastSnap = nil
            try setFrame(window, f)
            return nil
        default:
            break
        }
        if boolAttribute(window, "AXFullScreen") {
            throw WMError.message("Can't move a fullscreen window — Exit Full Screen first")
        }
        var cycle = 0
        if let l = lastSnap, l.action == action, CFEqual(l.window, window), current.near(l.target, 3) {
            cycle = l.cycle + 1
        }
        guard let target = target(action: action, current: current, cycle: cycle) else {
            throw WMError.message("nothing to do for \(action)")
        }
        if !preSnap.contains(where: { CFEqual($0.window, window) }) {
            preSnap.append((window, current))
            if preSnap.count > 40 { preSnap.removeFirst() }
        }
        remember(window, current)
        try setFrame(window, target)
        lastSnap = (window, action, cycle, target)
        return target
    }

    /// The frame `action` would give the front window of `app` (live preview), without moving it.
    static func previewTarget(action: String, app: NSRunningApplication?) -> Rect? {
        guard accessibilityTrusted, let app, let window = focusedWindow(pid: app.processIdentifier),
              let current = frame(of: window) else { return nil }
        var cycle = 0
        if let l = lastSnap, l.action == action, CFEqual(l.window, window), current.near(l.target, 3) {
            cycle = l.cycle + 1
        }
        return target(action: action, current: current, cycle: cycle)
    }

    /// Ask the engine where `action` puts a window at `current`.
    static func target(action: String, current: Rect, cycle: Int) -> Rect? {
        let screens = orderedScreens().map { $0.workTopLeft.json }
        let r = Engine.call([
            "op": "windowTarget", "action": action, "current": current.json, "screens": screens, "cycle": cycle,
        ])
        guard let d = r["rect"] as? [String: Any] else { return nil }
        return Rect(json: d)
    }

    private static func remember(_ window: AXUIElement, _ frame: Rect) {
        if let i = history.firstIndex(where: { CFEqual($0.window, window) }) {
            history[i].frames.append(frame)
            if history[i].frames.count > 10 { history[i].frames.removeFirst() }
        } else {
            history.append((window, [frame]))
            if history.count > 40 { history.removeFirst() }
        }
    }

    /// Drag-away restore: the window was snapped by us and is being dragged.
    static func snappedFrame(of window: AXUIElement) -> (snapped: Rect, before: Rect)? {
        guard let l = lastSnap, CFEqual(l.window, window),
              let pre = preSnap.first(where: { CFEqual($0.window, window) }) else { return nil }
        return (l.target, pre.frame)
    }

    static func forgetSnap(_ window: AXUIElement) {
        preSnap.removeAll { CFEqual($0.window, window) }
        if let l = lastSnap, CFEqual(l.window, window) { lastSnap = nil }
    }

    /// Focused window of the front app and its frame (drag-to-snap).
    static func frontWindow() -> (AXUIElement, Rect)? {
        guard accessibilityTrusted, let app = NSWorkspace.shared.frontmostApplication,
              let w = focusedWindow(pid: app.processIdentifier), let f = frame(of: w) else { return nil }
        return (w, f)
    }

    /// Whether the screen holding `app`'s window (else the main screen) is
    /// taller than wide, i.e. a rotated monitor.
    static func isPortraitScreen(for app: NSRunningApplication?) -> Bool {
        var screen = NSScreen.main
        if accessibilityTrusted, let app, let w = focusedWindow(pid: app.processIdentifier), let f = frame(of: w) {
            let c = CGPoint(x: f.x + f.w / 2, y: f.y + f.h / 2)
            screen = orderedScreens().first { $0.frameTopLeft.contains(c) } ?? screen
        }
        guard let s = screen?.frame else { return false }
        return s.height > s.width * 1.05
    }

    static func move(_ window: AXUIElement, to r: Rect) {
        try? setFrame(window, r)
    }

    /// Main display first (it is "Display 1"), then the rest left to right.
    static func orderedScreens() -> [NSScreen] {
        guard let main = NSScreen.screens.first else { return [] }
        return [main] + NSScreen.screens.dropFirst().sorted { $0.frame.minX < $1.frame.minX }
    }

    /// Screen rectangles in AX coordinates (overlays convert back to AppKit).
    static func screenFrame(containing p: CGPoint) -> Rect? {
        NSScreen.screens.first { $0.frameTopLeft.contains(p) }?.frameTopLeft
    }

    /// Every visible window of every regular app, in AX (top-left) coordinates.
    static func captureLayout() throws -> [AppWindows] {
        guard accessibilityTrusted else { throw WMError.permission }
        var out: [AppWindows] = []
        let me = ProcessInfo.processInfo.processIdentifier
        for app in NSWorkspace.shared.runningApplications where app.activationPolicy == .regular && app.processIdentifier != me {
            let name = app.localizedName ?? ""
            guard !name.isEmpty else { continue }
            var windows: [SavedWindow] = []
            for w in allWindows(pid: app.processIdentifier) {
                guard let f = frame(of: w), f.w > 1, f.h > 1 else { continue }
                if isMinimized(w) { continue }
                windows.append(SavedWindow(title: title(of: w), x: f.x, y: f.y, w: f.w, h: f.h))
            }
            if !windows.isEmpty {
                out.append(AppWindows(app: name, bundleId: app.bundleIdentifier, windows: windows))
            }
        }
        return out
    }

    /// Place the windows of running apps. Returns the names of apps that are not running.
    static func applyLayout(_ apps: [AppWindows]) throws -> [AppWindows] {
        guard accessibilityTrusted else { throw WMError.permission }
        var missing: [AppWindows] = []
        let running = NSWorkspace.shared.runningApplications
        for entry in apps {
            let app = running.first { r in
                (entry.bundleId != nil && r.bundleIdentifier == entry.bundleId) || r.localizedName == entry.app
            }
            guard let app else {
                missing.append(entry)
                continue
            }
            let windows = allWindows(pid: app.processIdentifier)
            // Match by title when possible, else by order.
            var used = Set<Int>()
            for saved in entry.windows {
                var idx: Int? = nil
                if !saved.title.isEmpty {
                    idx = windows.indices.first { !used.contains($0) && title(of: windows[$0]) == saved.title }
                }
                if idx == nil { idx = windows.indices.first { !used.contains($0) } }
                guard let i = idx else { break }
                used.insert(i)
                try? setFrame(windows[i], clamp(Rect(x: saved.x, y: saved.y, w: saved.w, h: saved.h)))
            }
        }
        return missing
    }

    /// Entries whose app is not running yet or has no window to place.
    static func notReady(_ apps: [AppWindows]) -> [AppWindows] {
        let running = NSWorkspace.shared.runningApplications
        return apps.filter { entry in
            guard let app = running.first(where: { r in
                (entry.bundleId != nil && r.bundleIdentifier == entry.bundleId) || r.localizedName == entry.app
            }) else { return true }
            return allWindows(pid: app.processIdentifier).isEmpty
        }
    }

    /// Launch apps by bundle id (preferred) or name.
    static func launch(_ apps: [AppWindows]) {
        for a in apps {
            if let bid = a.bundleId, let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bid) {
                NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
                continue
            }
            for dir in ["/Applications", "/System/Applications", NSHomeDirectory() + "/Applications", "/Applications/Utilities"] {
                let url = URL(fileURLWithPath: "\(dir)/\(a.app).app")
                if FileManager.default.fileExists(atPath: url.path) {
                    NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
                    break
                }
            }
        }
    }

    /// Sorted display frames for `layouts::display_signature` (AX coordinates).
    static func displaySignature() -> String {
        NSScreen.screens
            .map { $0.frameTopLeft }
            .map { "\(Int($0.x.rounded())),\(Int($0.y.rounded())),\(Int($0.w.rounded())),\(Int($0.h.rounded()))" }
            .sorted()
            .joined(separator: ";")
    }

    // MARK: - AX helpers

    enum WMError: Error, CustomStringConvertible {
        case permission
        case message(String)

        var description: String {
            switch self {
            case .permission: permissionHint
            case .message(let m): m
            }
        }
    }

    static func focusedWindow(pid: pid_t) -> AXUIElement? {
        let app = AXUIElementCreateApplication(pid)
        var value: CFTypeRef?
        if AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &value) == .success, let v = value {
            return (v as! AXUIElement)
        }
        if AXUIElementCopyAttributeValue(app, kAXMainWindowAttribute as CFString, &value) == .success, let v = value {
            return (v as! AXUIElement)
        }
        return allWindows(pid: pid).first
    }

    private static func allWindows(pid: pid_t) -> [AXUIElement] {
        let app = AXUIElementCreateApplication(pid)
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success,
              let list = value as? [AXUIElement]
        else { return [] }
        return list.filter { w in
            var role: CFTypeRef?
            AXUIElementCopyAttributeValue(w, kAXSubroleAttribute as CFString, &role)
            let sub = role as? String
            return sub == nil || sub == kAXStandardWindowSubrole as String
        }
    }

    private static func frame(of window: AXUIElement) -> Rect? {
        var posValue: CFTypeRef?
        var sizeValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &posValue) == .success,
              AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeValue) == .success,
              let pv = posValue, let sv = sizeValue
        else { return nil }
        var pos = CGPoint.zero
        var size = CGSize.zero
        guard AXValueGetValue(pv as! AXValue, .cgPoint, &pos), AXValueGetValue(sv as! AXValue, .cgSize, &size) else { return nil }
        return Rect(x: pos.x, y: pos.y, w: size.width, h: size.height)
    }

    private static func setFrame(_ window: AXUIElement, _ r: Rect) throws {
        var pos = CGPoint(x: r.x.rounded(), y: r.y.rounded())
        var size = CGSize(width: r.w.rounded(), height: r.h.rounded())
        guard let p = AXValueCreate(.cgPoint, &pos), let s = AXValueCreate(.cgSize, &size) else {
            throw WMError.message("could not build the window frame")
        }
        // Chrome, Electron apps (Slack, VS Code, Discord…) and others switch on
        // AXEnhancedUserInterface, which makes macOS *animate* every AX move and
        // resize — that is the visible lag. Turn it off for the move, then restore it
        // (Rectangle does the same).
        var pid: pid_t = 0
        AXUIElementGetPid(window, &pid)
        let app = AXUIElementCreateApplication(pid)
        let enhanced = boolAttribute(app, "AXEnhancedUserInterface")
        if enhanced {
            AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanFalse)
        }
        defer {
            if enhanced {
                AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
            }
        }
        let current = frame(of: window)
        let sameSize = current.map { abs($0.w - size.width) < 1 && abs($0.h - size.height) < 1 } ?? false
        // Position first so a grow does not get clamped by the screen edge, then size,
        // then position again only if the app moved the window while resizing.
        let a = AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, p)
        var b = AXError.success
        if !sameSize {
            b = AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, s)
            if let now = frame(of: window), abs(now.x - pos.x) >= 1 || abs(now.y - pos.y) >= 1 {
                AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, p)
            }
        }
        if a != .success && b != .success {
            if a == .apiDisabled || a == .notImplemented || b == .apiDisabled || b == .notImplemented {
                throw WMError.permission
            }
            throw WMError.message("the app refused to move its window")
        }
    }

    private static func boolAttribute(_ element: AXUIElement, _ name: String) -> Bool {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return false }
        return (value as? Bool) ?? false
    }

    private static func title(of window: AXUIElement) -> String {
        var value: CFTypeRef?
        AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &value)
        return value as? String ?? ""
    }

    private static func isMinimized(_ window: AXUIElement) -> Bool {
        var value: CFTypeRef?
        AXUIElementCopyAttributeValue(window, kAXMinimizedAttribute as CFString, &value)
        return (value as? Bool) ?? false
    }

    private static func center(of r: Rect) -> CGPoint {
        CGPoint(x: r.x + r.w / 2, y: r.y + r.h / 2)
    }

    private static func screenContaining(_ p: CGPoint) -> NSScreen? {
        NSScreen.screens.first { $0.frameTopLeft.contains(p) }
    }

    private static func sortedScreens() -> [NSScreen] {
        NSScreen.screens.sorted { $0.frameTopLeft.x < $1.frameTopLeft.x }
    }

    /// Keep a saved frame on some screen when the display set changed.
    private static func clamp(_ r: Rect) -> Rect {
        if screenContaining(center(of: r)) != nil { return r }
        guard let screen = NSScreen.main else { return r }
        let work = screen.workTopLeft
        let w = min(r.w, work.w), h = min(r.h, work.h)
        return Rect(x: work.x + max(0, work.w - w) / 2, y: work.y + max(0, work.h - h) / 2, w: w, h: h)
    }
}

extension WindowManager.Rect {
    func contains(_ p: CGPoint) -> Bool {
        p.x >= x && p.x < x + w && p.y >= y && p.y < y + h
    }

    func near(_ o: WindowManager.Rect, _ tol: Double) -> Bool {
        abs(x - o.x) <= tol && abs(y - o.y) <= tol && abs(w - o.w) <= tol && abs(h - o.h) <= tol
    }

    var json: [String: Double] { ["x": x, "y": y, "w": w, "h": h] }

    init?(json d: [String: Any]) {
        guard let x = d["x"] as? Double, let y = d["y"] as? Double, let w = d["w"] as? Double, let h = d["h"] as? Double else { return nil }
        self.init(x: x, y: y, w: w, h: h)
    }

    /// AppKit (bottom-left origin) frame for an overlay window.
    var appKitFrame: NSRect {
        let primary = NSScreen.screens.first?.frame.maxY ?? 0
        return NSRect(x: x, y: primary - y - h, width: w, height: h)
    }
}

extension NSScreen {
    /// AppKit's origin is bottom-left; AX wants top-left (y down) relative to the primary screen.
    static var primaryHeight: CGFloat { NSScreen.screens.first?.frame.maxY ?? 0 }

    var frameTopLeft: WindowManager.Rect {
        let f = frame
        return WindowManager.Rect(x: f.minX, y: Self.primaryHeight - f.maxY, w: f.width, h: f.height)
    }

    /// Visible frame (menu bar and Dock excluded) in AX coordinates.
    var workTopLeft: WindowManager.Rect {
        let f = visibleFrame
        return WindowManager.Rect(x: f.minX, y: Self.primaryHeight - f.maxY, w: f.width, h: f.height)
    }
}
