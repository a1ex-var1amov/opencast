import AppKit
import ApplicationServices

// Click Text's scan. No dependency on the panel, so `macos-tests/seek-probe.swift`
// can compile this file alone and run it against the live screen.

struct SeekRect {
    var x, y, w, h: CGFloat
}

struct SeekWord {
    var at: Int
    var len: Int
    var x, w: CGFloat
}

struct SeekHit {
    var label: String
    /// The part of the control that is on screen, in CG screen points.
    var x, y, w, h: CGFloat
    var words: [SeekWord]
    /// The control AXPress goes to. Nil: plain text, clicked at its centre.
    var element: AXUIElement? = nil
}

struct SeekScan {
    var monitor: SeekRect
    var hits: [SeekHit]
    var visited = 0
}

/// One entry of the window server's on-screen list, which is ordered front to back.
struct SeekWindow {
    var pid: pid_t
    var frame: CGRect
    var layer: Int
}

/// On-screen labels from Accessibility. Same permission as the hyper key. No screenshot.
enum SeekAX {
    enum Failure: Error, LocalizedError {
        case accessibility
        case noMonitor
        case noText

        var errorDescription: String? {
            switch self {
            case .accessibility: return "Click Text needs Accessibility"
            case .noMonitor: return "no monitors found"
            case .noText: return "Nothing on this screen exposes its text to Accessibility"
            }
        }
    }

    /// Never a target, and nothing useful below them.
    private static let skip: Set<String> = [
        "AXScrollBar", "AXSplitter", "AXHandle", "AXGrowArea", "AXBusyIndicator",
        "AXProgressIndicator", "AXValueIndicator", "AXLayoutArea", "AXLayoutItem",
        "AXRuler", "AXRulerMarker", "AXDockItem",
    ]
    /// Controls AXPress works on. A labeled one is one hit; the text inside it is not listed again.
    private static let pressable: Set<String> = [
        "AXButton", "AXLink", "AXMenuItem", "AXMenuButton", "AXPopUpButton",
        "AXCheckBox", "AXRadioButton", "AXDisclosureTriangle",
    ]
    private static let fields: Set<String> = ["AXTextField", "AXTextArea", "AXComboBox"]
    /// One round trip per node instead of one per attribute.
    private static let attributes = [
        kAXRoleAttribute, kAXPositionAttribute, kAXSizeAttribute, kAXValueAttribute, kAXTitleAttribute,
        kAXDescriptionAttribute, kAXPlaceholderValueAttribute, kAXVisibleChildrenAttribute,
    ] as CFArray
    /// Wall-clock cap for one scan. A hung or huge app shows what was found so far.
    static let budget: TimeInterval = 1.0
    private static let maxHits = 1000
    private static let maxDepth = 40

    /// Labels you can see on the display under the cursor: windows that are not
    /// covered, front to back, and inside them only what is scrolled into view.
    static func scan() throws -> SeekScan {
        let monitor = try cursorMonitor()
        let bounds = CGRect(x: monitor.x, y: monitor.y, width: monitor.w, height: monitor.h)
        let windows = onScreenWindows(in: bounds)
        let me = ProcessInfo.processInfo.processIdentifier
        let deadline = Date().addingTimeInterval(budget)
        var hits: [SeekHit] = []
        var seen = Set<String>()
        var visited = 0

        func walk(_ el: AXUIElement, depth: Int, clip: CGRect, front: [CGRect], press: AXUIElement?) {
            guard hits.count < maxHits, Date() < deadline else { return }
            visited += 1
            AXUIElementSetMessagingTimeout(el, 0.15)
            guard let n = node(el), !skip.contains(n.role) else { return }
            var clip = clip
            var shown: CGRect?
            if let frame = n.frame {
                let part = frame.intersection(clip)
                // Scrolled out of view, or wholly under another window: nothing below it can be seen.
                if hidden(part, by: front) { return }
                shown = part
                if n.role == "AXScrollArea" || n.role == "AXWebArea" { clip = part }
            }
            let canPress = pressable.contains(n.role)
            // Text you can read. A name that is not drawn (an icon's description) only counts on a control.
            let label = canPress
                ? n.title ?? n.value ?? n.description
                : n.value ?? (fields.contains(n.role) ? n.placeholder : nil) ?? n.title
            let kids = canPress && label != nil ? [] : n.children ?? children(el)
            // Leaves and labeled controls only: a window, group or web area with a title is not a target.
            if let label, let shown, kids.isEmpty {
                let center = CGPoint(x: shown.midX, y: shown.midY)
                // Web pages often carry the same text twice in one box (one copy for screen readers).
                let twin = "\(label)\u{0}\(Int(shown.minX)),\(Int(shown.minY)),\(Int(shown.width)),\(Int(shown.height))"
                if !front.contains(where: { $0.contains(center) }), seen.insert(twin).inserted {
                    hits.append(SeekHit(
                        label: label, x: shown.minX, y: shown.minY, w: shown.width, h: shown.height,
                        words: [], element: canPress ? el : press
                    ))
                }
            }
            guard depth < maxDepth else { return }
            for kid in kids { walk(kid, depth: depth + 1, clip: clip, front: front, press: canPress ? el : press) }
        }

        var asked = Set<pid_t>()
        for (i, win) in windows.enumerated() {
            // Only apps with a normal window that shows: the ones behind are slow to answer and have
            // nothing to click, and menu bar agents have no windows to ask for.
            guard win.pid != me, win.layer == 0, !asked.contains(win.pid),
                  !hidden(win.frame.intersection(bounds), by: occluders(windows, inFrontOf: i))
            else { continue }
            asked.insert(win.pid)
            let app = AXUIElementCreateApplication(win.pid)
            AXUIElementSetMessagingTimeout(app, 0.25)
            var value: CFTypeRef?
            guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success else { continue }
            // AX also lists minimized windows and windows on other Spaces. Matching each to its
            // window-server entry drops those and gives the stacking order.
            // ponytail: matched by frame, twins in AX order. `_AXUIElementGetWindow` (private) is the exact answer.
            var taken = Set<Int>()
            var tops: [(el: AXUIElement, at: Int)] = []
            for el in elements(value) {
                guard let frame = node(el)?.frame,
                      let at = windows.indices.first(where: {
                          windows[$0].pid == win.pid && !taken.contains($0) && near(windows[$0].frame, frame)
                      })
                else { continue }
                taken.insert(at)
                tops.append((el, at))
            }
            for top in tops.sorted(by: { $0.at < $1.at }) {
                let front = occluders(windows, inFrontOf: top.at, own: taken)
                let clip = windows[top.at].frame.intersection(bounds)
                if hidden(clip, by: front) { continue }
                walk(top.el, depth: 0, clip: clip, front: front, press: nil)
            }
        }
        if hits.isEmpty { throw Failure.noText }
        return SeekScan(monitor: monitor, hits: hits, visited: visited)
    }

    // MARK: - What is in front of what

    static func onScreenWindows(in bounds: CGRect) -> [SeekWindow] {
        let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        return info.compactMap { row in
            guard let pid = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value,
                  let dict = row[kCGWindowBounds as String] as? NSDictionary,
                  let frame = CGRect(dictionaryRepresentation: dict as CFDictionary),
                  frame.intersects(bounds),
                  ((row[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 1) > 0
            else { return nil }
            return SeekWindow(pid: pid, frame: frame, layer: (row[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0)
        }
    }

    /// Frames that hide `index`: normal-level windows in front of it. Menu bar, Dock and
    /// overlay levels are often full-screen and see-through, so they do not count. Of the
    /// app's own windows only `own` (its top-level ones) count: a sheet or popover sits in
    /// front of its parent and must not hide itself.
    static func occluders(_ windows: [SeekWindow], inFrontOf index: Int, own: Set<Int> = []) -> [CGRect] {
        windows[..<index].enumerated().compactMap { i, w in
            w.layer == 0 && (w.pid != windows[index].pid || own.contains(i)) ? w.frame : nil
        }
    }

    static func hidden(_ rect: CGRect, by front: [CGRect]) -> Bool {
        rect.isEmpty || front.contains { $0.contains(rect) }
    }

    private static func near(_ a: CGRect, _ b: CGRect) -> Bool {
        abs(a.minX - b.minX) <= 2 && abs(a.minY - b.minY) <= 2 && abs(a.width - b.width) <= 2 && abs(a.height - b.height) <= 2
    }

    // MARK: - Reading one node

    private struct Node {
        var role = ""
        var frame: CGRect?
        var value, title, description, placeholder: String?
        /// Nil when the element has no visible-children list; ask for all children then.
        var children: [AXUIElement]?
    }

    private static func node(_ el: AXUIElement) -> Node? {
        var out: CFArray?
        guard AXUIElementCopyMultipleAttributeValues(el, attributes, AXCopyMultipleAttributeOptions(rawValue: 0), &out) == .success,
              let v = out as? [AnyObject], v.count == 8
        else { return nil }
        var n = Node()
        n.role = v[0] as? String ?? ""
        var pos = CGPoint.zero
        var size = CGSize.zero
        if CFGetTypeID(v[1]) == AXValueGetTypeID(), CFGetTypeID(v[2]) == AXValueGetTypeID(),
           AXValueGetValue(v[1] as! AXValue, .cgPoint, &pos), AXValueGetValue(v[2] as! AXValue, .cgSize, &size),
           size.width >= 2, size.height >= 2
        {
            n.frame = CGRect(origin: pos, size: size)
        }
        n.value = string(v[3])
        n.title = string(v[4])
        n.description = string(v[5])
        n.placeholder = string(v[6])
        if CFGetTypeID(v[7]) == CFArrayGetTypeID() { n.children = elements(v[7]) }
        return n
    }

    /// Text only. A checkbox's 0 or a slider's 0.5 is a value, not a label.
    private static func string(_ value: AnyObject) -> String? {
        let raw = (value as? String) ?? (value as? NSAttributedString)?.string
        guard let text = raw?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty, text.count <= 240
        else { return nil }
        return text
    }

    private static func children(_ el: AXUIElement) -> [AXUIElement] {
        var value: CFTypeRef?
        AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &value)
        return elements(value)
    }

    private static func elements(_ value: CFTypeRef?) -> [AXUIElement] {
        guard let value, CFGetTypeID(value) == CFArrayGetTypeID() else { return [] }
        let array = value as! CFArray
        var out: [AXUIElement] = []
        for i in 0..<CFArrayGetCount(array) {
            guard let ptr = CFArrayGetValueAtIndex(array, i) else { continue }
            let el = Unmanaged<AXUIElement>.fromOpaque(ptr).takeUnretainedValue()
            if CFGetTypeID(el) == AXUIElementGetTypeID() { out.append(el) }
        }
        return out
    }

    private static func cursorMonitor() throws -> SeekRect {
        let point = CGEvent(source: nil)?.location ?? .zero
        var count: UInt32 = 16
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        guard CGGetActiveDisplayList(count, &ids, &count) == .success, count > 0 else {
            throw Failure.noMonitor
        }
        let rects: [SeekRect] = (0..<Int(count)).map { i in
            let b = CGDisplayBounds(ids[i])
            return SeekRect(x: b.origin.x, y: b.origin.y, w: b.size.width, h: b.size.height)
        }
        if let hit = rects.first(where: {
            point.x >= $0.x && point.x < $0.x + $0.w && point.y >= $0.y && point.y < $0.y + $0.h
        }) {
            return hit
        }
        return rects[0]
    }
}
