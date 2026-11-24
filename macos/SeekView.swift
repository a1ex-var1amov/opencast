import AppKit
import CoreGraphics
import OSLog
import SwiftUI

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

    /// The box is a whole control now, so the fill stays see-through: its label has to stay readable.
    private func hit(_ span: SeekSpan, current: Bool) -> some View {
        RoundedRectangle(cornerRadius: 4)
            .fill(Theme.accent.opacity(current ? 0.45 : 0.2))
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
