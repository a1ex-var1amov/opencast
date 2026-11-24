import AppKit
import ApplicationServices
import CoreGraphics
import OSLog
import SwiftUI

struct SeekRect {
    var x, y, w, h: CGFloat
}

/// Opt-in diagnostics (Settings → About). NSLog lines never reach the unified
/// log on macOS 27; os.Logger with public interpolation does.
enum Diagnostics {
    static let key = "diagnostics"
    static let command = "log stream --style compact --predicate 'subsystem == \"dev.opencast.launcher\"'"
    private static let logger = Logger(subsystem: "dev.opencast.launcher", category: "diag")

    static func log(_ message: @autoclosure () -> String, error: Bool = false) {
        guard UserDefaults.standard.bool(forKey: key) else { return }
        let text = message()
        if error { logger.error("\(text, privacy: .public)") } else { logger.notice("\(text, privacy: .public)") }
        Telemetry.event(error ? "error" : "info", text)
    }
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
            case .noText: return "This screen doesn't expose its text to Accessibility"
            }
        }
    }

    /// Labels and frames from the accessibility tree of on-screen apps.
    static func scan() throws -> SeekScan {
        let t0 = Date()
        var ms: Int { Int(-t0.timeIntervalSinceNow * 1000) }
        do {
            guard PermissionAccess.accessibilityWorks() else { throw Failure.accessibility }
            let m = try cursorMonitor()
            guard let ax = accessibilityHits(on: m) else { throw Failure.noText }
            Diagnostics.log("seek \(ax.hits.count) accessibility hits in \(ms) ms")
            return ax
        } catch {
            Diagnostics.log("seek failed after \(ms) ms: \(error)", error: true)
            throw error
        }
    }

    /// Press the control. Same path as menu search. Falls back to a HID click
    /// only when the hit has no live element (legacy JSON scans).
    static func press(_ hit: SeekHit, fallbackCenter: CGPoint) {
        if let el = hit.element {
            var pid: pid_t = 0
            AXUIElementGetPid(el, &pid)
            if pid != 0 {
                NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateIgnoringOtherApps])
            }
            usleep(80_000)
            if AXUIElementPerformAction(el, kAXPressAction as CFString) == .success {
                Diagnostics.log("seek AXPress ok")
                return
            }
            Diagnostics.log("seek AXPress failed, falling back to click", error: true)
        }
        _ = Engine.call(["op": "seekClick", "x": Double(fallbackCenter.x), "y": Double(fallbackCenter.y)])
    }

    /// Nil when this display's accessibility tree has no labels.
    private static func accessibilityHits(on monitor: SeekRect) -> SeekScan? {
        guard PermissionAccess.accessibilityWorks() else { return nil }
        // Chrome we never want as a click target; still walk past them.
        let skip: Set<String> = [
            "AXScrollBar", "AXSplitter", "AXHandle", "AXGrowArea", "AXBusyIndicator",
            "AXProgressIndicator", "AXValueIndicator", "AXLayoutArea", "AXLayoutItem",
            "AXRuler", "AXRulerMarker", "AXDockItem",
        ]
        var hits: [SeekHit] = []
        var visits = 0
        let maxVisits = 8000
        let maxHits = 800
        let me = ProcessInfo.processInfo.processIdentifier

        func frame(of el: AXUIElement) -> SeekRect? {
            var posV: CFTypeRef?
            var sizeV: CFTypeRef?
            guard AXUIElementCopyAttributeValue(el, kAXPositionAttribute as CFString, &posV) == .success,
                  AXUIElementCopyAttributeValue(el, kAXSizeAttribute as CFString, &sizeV) == .success,
                  let posV, let sizeV,
                  CFGetTypeID(posV) == AXValueGetTypeID(),
                  CFGetTypeID(sizeV) == AXValueGetTypeID()
            else { return nil }
            var pos = CGPoint.zero
            var size = CGSize.zero
            guard AXValueGetValue(posV as! AXValue, .cgPoint, &pos),
                  AXValueGetValue(sizeV as! AXValue, .cgSize, &size),
                  size.width >= 2, size.height >= 2
            else { return nil }
            return SeekRect(x: pos.x, y: pos.y, w: size.width, h: size.height)
        }

        func overlaps(_ a: SeekRect) -> Bool {
            a.x < monitor.x + monitor.w && a.x + a.w > monitor.x && a.y < monitor.y + monitor.h && a.y + a.h > monitor.y
        }

        func stringValue(_ value: CFTypeRef?) -> String? {
            guard let value else { return nil }
            if let s = value as? String {
                let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
                return t.isEmpty || t.count > 240 ? nil : t
            }
            if let a = value as? NSAttributedString {
                let t = a.string.trimmingCharacters(in: .whitespacesAndNewlines)
                return t.isEmpty || t.count > 240 ? nil : t
            }
            if let n = value as? NSNumber {
                return n.stringValue
            }
            return nil
        }

        func text(_ el: AXUIElement, _ attr: String) -> String? {
            var value: CFTypeRef?
            guard AXUIElementCopyAttributeValue(el, attr as CFString, &value) == .success else { return nil }
            return stringValue(value)
        }

        func label(of el: AXUIElement) -> String? {
            text(el, kAXValueAttribute as String)
                ?? text(el, kAXTitleAttribute as String)
                ?? text(el, kAXDescriptionAttribute as String)
                ?? text(el, "AXAttributedDescription")
        }

        func elements(_ value: CFTypeRef?) -> [AXUIElement] {
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

        func children(_ el: AXUIElement) -> [AXUIElement] {
            var value: CFTypeRef?
            if AXUIElementCopyAttributeValue(el, kAXVisibleChildrenAttribute as CFString, &value) != .success {
                AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &value)
            }
            return elements(value)
        }

        func walk(_ el: AXUIElement, depth: Int) {
            visits += 1
            guard visits <= maxVisits, hits.count < maxHits else { return }
            AXUIElementSetMessagingTimeout(el, 0.15)
            let role = text(el, kAXRoleAttribute as String) ?? ""
            if skip.contains(role) { return }
            if let box = frame(of: el), overlaps(box), let label = label(of: el) {
                let len = (label as NSString).length
                hits.append(SeekHit(
                    label: label, x: box.x, y: box.y, w: box.w, h: box.h,
                    words: [SeekWord(at: 0, len: len, x: box.x, w: box.w)],
                    element: el
                ))
            }
            guard depth < 12 else { return }
            for child in children(el) { walk(child, depth: depth + 1) }
        }

        var onScreen = Set<pid_t>()
        if let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
            for row in info {
                guard let pid = (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value, pid != me,
                      let b = row[kCGWindowBounds as String] as? NSDictionary,
                      let x = (b["X"] as? NSNumber)?.doubleValue,
                      let y = (b["Y"] as? NSNumber)?.doubleValue,
                      let w = (b["Width"] as? NSNumber)?.doubleValue,
                      let h = (b["Height"] as? NSNumber)?.doubleValue,
                      overlaps(SeekRect(x: x, y: y, w: w, h: h))
                else { continue }
                onScreen.insert(pid)
            }
        }

        // Frontmost first so a dense background app cannot eat the hit budget.
        var pids = Array(onScreen)
        if let front = NSWorkspace.shared.frontmostApplication?.processIdentifier,
           let i = pids.firstIndex(of: front)
        {
            pids.remove(at: i)
            pids.insert(front, at: 0)
        }

        for pid in pids {
            let ax = AXUIElementCreateApplication(pid)
            AXUIElementSetMessagingTimeout(ax, 0.25)
            var windows: CFTypeRef?
            guard AXUIElementCopyAttributeValue(ax, kAXWindowsAttribute as CFString, &windows) == .success else { continue }
            let list = elements(windows)
            for window in list {
                guard hits.count < maxHits, let box = frame(of: window), overlaps(box) else { continue }
                walk(window, depth: 0)
            }
            if hits.count >= maxHits { break }
        }
        Diagnostics.log("seek accessibility walk \(visits) nodes, \(hits.count) hits")
        return hits.isEmpty ? nil : SeekScan(monitor: monitor, hits: hits)
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

struct SeekWord {
    var at: Int
    var len: Int
    var x, w: CGFloat
}

struct SeekHit {
    var label: String
    var x, y, w, h: CGFloat
    var words: [SeekWord]
    /// Live accessibility node for AXPress. Nil for legacy JSON scans.
    var element: AXUIElement? = nil
}

struct SeekScan {
    var monitor: SeekRect
    var hits: [SeekHit]
}

struct SeekSpan: Identifiable {
    var id: Int
    /// Index into `SeekScan.hits` — Return presses that whole control.
    var hitIndex: Int
    var x, y, w, h: CGFloat
}

extension SeekRect {
    init?(_ raw: Any?) {
        guard let raw = raw as? [String: Any],
              let x = cg(raw["x"]), let y = cg(raw["y"]), let w = cg(raw["w"]), let h = cg(raw["h"])
        else { return nil }
        self.init(x: x, y: y, w: w, h: h)
    }
}

extension SeekWord {
    init?(_ raw: [String: Any]) {
        guard let at = jsonInt(raw["at"]), let len = jsonInt(raw["len"]), let x = cg(raw["x"]), let w = cg(raw["w"])
        else { return nil }
        self.init(at: at, len: len, x: x, w: w)
    }
}

extension SeekHit {
    init?(_ raw: [String: Any]) {
        guard let label = raw["label"] as? String, let rect = SeekRect(raw) else { return nil }
        let words = (raw["words"] as? [[String: Any]])?.compactMap(SeekWord.init) ?? []
        self.init(label: label, x: rect.x, y: rect.y, w: rect.w, h: rect.h, words: words, element: nil)
    }
}

extension SeekScan {
    init?(_ raw: Any?) {
        guard let raw = raw as? [String: Any], let monitor = SeekRect(raw["monitor"]) else { return nil }
        self.init(monitor: monitor, hits: (raw["hits"] as? [[String: Any]])?.compactMap(SeekHit.init) ?? [])
    }
}

/// Screen x of character index `i`. Whole-label box when words cover it, otherwise an even split.
func charX(_ hit: SeekHit, _ i: Int) -> CGFloat {
    if let word = hit.words.last(where: { $0.at <= i }) {
        guard word.len > 0 else { return word.x }
        let span = min(i - word.at, word.len)
        return word.x + (CGFloat(span) / CGFloat(word.len)) * word.w
    }
    let n = CGFloat(max(hit.label.utf16.count, 1))
    return hit.x + (hit.w / n) * CGFloat(i)
}

/// One span per matching control. Highlight is the whole hit; Return presses that element.
func matchSpans(scan: SeekScan, query: String) -> [SeekSpan] {
    let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    if needle.isEmpty { return [] }
    var out: [SeekSpan] = []
    for (hi, hit) in scan.hits.enumerated() {
        guard hit.label.lowercased().contains(needle) else { continue }
        out.append(SeekSpan(
            id: out.count,
            hitIndex: hi,
            x: hit.x - scan.monitor.x,
            y: hit.y - scan.monitor.y,
            w: max(hit.w, 4),
            h: max(hit.h, 4)
        ))
    }
    return out
}

func seekSpanCheck() {
    let worded = SeekHit(
        label: "Ryan Hanson",
        x: 120, y: 160, w: 220, h: 30,
        words: [
            SeekWord(at: 0, len: 4, x: 120, w: 90),
            SeekWord(at: 5, len: 6, x: 220, w: 120),
        ]
    )
    assert(charX(worded, 2) == 165)
    assert(charX(worded, 5) == 220)
    let scan = SeekScan(
        monitor: SeekRect(x: 100, y: 50, w: 800, h: 600),
        hits: [SeekHit(label: "abcd", x: 110, y: 80, w: 40, h: 10, words: [])]
    )
    let spans = matchSpans(scan: scan, query: "Bc")
    assert(spans.count == 1)
    assert(spans[0].hitIndex == 0)
    assert(spans[0].x == 10 && spans[0].y == 30 && spans[0].w == 40 && spans[0].h == 10)
    assert(matchSpans(scan: scan, query: "  ").isEmpty)
}

struct SeekView: View {
    @Bindable var model: Model

    var body: some View {
        let spans = model.seekSpans
        let cursor = spans.isEmpty ? 0 : ((model.seekIndex % spans.count) + spans.count) % spans.count
        ZStack {
            Color.clear
            if let from = model.seekLeaderFrom, spans.indices.contains(cursor) {
                leader(from: from, to: spans[cursor])
            }
            ForEach(spans) { span in
                hit(span, current: span.id == cursor)
            }
        }
    }

    /// A line from the search field to the current match, so the eye finds it at once.
    private func leader(from: CGPoint, to span: SeekSpan) -> some View {
        let to = CGPoint(x: span.x + span.w / 2, y: span.y + span.h / 2)
        return ZStack {
            Path { p in
                p.move(to: from)
                // A gentle curve reads better than a straight diagonal.
                p.addQuadCurve(to: to, control: CGPoint(x: to.x, y: from.y))
            }
            .stroke(Theme.accent.opacity(0.85), style: StrokeStyle(lineWidth: 2, lineCap: .round, dash: [6, 5]))
            Circle()
                .fill(Theme.accent)
                .frame(width: 7, height: 7)
                .position(from)
        }
        .allowsHitTesting(false)
        .animation(.easeOut(duration: 0.15), value: to)
    }

    private func hit(_ span: SeekSpan, current: Bool) -> some View {
        RoundedRectangle(cornerRadius: 4)
            .fill(Theme.accent.opacity(current ? 1 : 0.5))
            .overlay {
                RoundedRectangle(cornerRadius: 4)
                    .stroke(Theme.accent, lineWidth: 1.5)
            }
            .overlay {
                if current {
                    RoundedRectangle(cornerRadius: 4)
                        .stroke(Theme.accent.opacity(0.12), lineWidth: 3)
                        .padding(-3)
                }
            }
            .frame(width: span.w, height: span.h)
            .position(x: span.x + span.w / 2, y: span.y + span.h / 2)
    }
}

private func cg(_ raw: Any?) -> CGFloat? {
    switch raw {
    case let number as NSNumber: return CGFloat(number.doubleValue)
    case let value as Double: return CGFloat(value)
    case let value as Int: return CGFloat(value)
    default: return nil
    }
}

private func jsonInt(_ raw: Any?) -> Int? {
    switch raw {
    case let number as NSNumber: return number.intValue
    case let value as Int: return value
    default: return nil
    }
}
