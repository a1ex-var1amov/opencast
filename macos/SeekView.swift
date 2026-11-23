import AppKit
import CoreGraphics
import OSLog
import SwiftUI
import Vision

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

/// In-process Seek OCR (Vision). Replaces the old Rust→osascript JXA path.
enum SeekOCR {
    enum Failure: Error, LocalizedError {
        case noMonitor
        case screenRecording
        case noText
        case recognitionFailed
        case recognitionStuck

        var errorDescription: String? {
            switch self {
            case .noMonitor: return "no monitors found"
            case .screenRecording:
                return "OpenCast could not capture the screen. System Settings → Privacy & Security → "
                    + "Screen Recording: enable OpenCast, or use Settings → Permissions → Re-check to add this version, "
                    + "then quit and relaunch OpenCast"
            case .noText: return "no readable text on screen"
            case .recognitionFailed: return "text recognition failed"
            case .recognitionStuck:
                return "Text recognition is still starting. Try Click Text again in a moment."
            }
        }
    }

    /// Screenshot the display under the cursor and OCR it. Hits are screen points (CG), with word boxes.
    static func scan() throws -> SeekScan {
        let t0 = Date()
        var ms: Int { Int(-t0.timeIntervalSinceNow * 1000) }
        do {
            let m = try cursorMonitor()
            let preflight = CGPreflightScreenCaptureAccess()
            Diagnostics.log("seek monitor \(Int(m.w))x\(Int(m.h)) at \(Int(m.x)),\(Int(m.y)) preflight=\(preflight)")
            let image = try capture(m)
            Diagnostics.log("seek captured \(image.width)x\(image.height) px in \(ms) ms")
            let scan = try recognize(image, monitor: m)
            Diagnostics.log("seek \(scan.hits.count) text hits in \(ms) ms")
            return scan
        } catch {
            Diagnostics.log("seek failed after \(ms) ms: \(error)", error: true)
            throw error
        }
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

    private static func capture(_ monitor: SeekRect) throws -> CGImage {
        let region = [monitor.x, monitor.y, monitor.w, monitor.h].map { "\(Int($0.rounded()))" }.joined(separator: ",")
        guard let data = Attachments.screencapture(["-R", region], tag: "seek"),
              let image = NSImage(data: data),
              let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else {
            throw Failure.screenRecording
        }
        return cg
    }

    /// Longest edge sent to Vision. A retina capture is ~4x the pixels of the
    /// screen points, and `perform` at that size can sit past any timeout on a
    /// cold model load. Boxes are normalized, so the smaller image still maps
    /// onto the real screen.
    private static func forOCR(_ image: CGImage) -> CGImage {
        let maxEdge = 2560
        let w = image.width, h = image.height
        let edge = max(w, h)
        guard edge > maxEdge else { return image }
        let scale = Double(maxEdge) / Double(edge)
        let nw = Int((Double(w) * scale).rounded())
        let nh = Int((Double(h) * scale).rounded())
        guard let ctx = CGContext(
            data: nil, width: nw, height: nh, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return image }
        ctx.interpolationQuality = .high
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: nw, height: nh))
        return ctx.makeImage() ?? image
    }

    /// One Vision pass. A second `perform` started while the first is still
    /// inside Vision queues behind it and times out too, which looks like OCR
    /// is missing. `cancel` does not unblock that first call.
    private static let vision = DispatchQueue(label: "dev.opencast.seek-ocr")
    private static let gate = NSLock()
    private static var ocrInFlight = false

    private static func recognize(_ image: CGImage, monitor: SeekRect) throws -> SeekScan {
        let image = forOCR(image)
        Diagnostics.log("seek ocr image \(image.width)x\(image.height)")
        gate.lock()
        let busy = ocrInFlight
        if !busy { ocrInFlight = true }
        gate.unlock()
        if busy {
            Diagnostics.log("seek OCR still in flight", error: true)
            throw Failure.recognitionStuck
        }
        let done = DispatchSemaphore(value: 0)
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .fast
        request.revision = VNRecognizeTextRequestRevision3
        request.usesLanguageCorrection = false
        request.automaticallyDetectsLanguage = false
        if let lang = Locale.preferredLanguages.first {
            request.recognitionLanguages = [lang]
        }
        var failed = false
        var results: [VNRecognizedTextObservation]?
        vision.async {
            failed = (try? VNImageRequestHandler(cgImage: image, options: [:]).perform([request])) == nil
            results = request.results
            gate.lock()
            ocrInFlight = false
            gate.unlock()
            done.signal()
        }
        let limit: TimeInterval = 20
        if done.wait(timeout: .now() + limit) == .timedOut {
            request.cancel()
            Diagnostics.log("seek OCR did not return in \(Int(limit)) s", error: true)
            throw Failure.recognitionStuck
        }
        if failed { throw Failure.recognitionFailed }
        Diagnostics.log("seek OCR returned \(results?.count ?? 0) observations")
        guard let results else { throw Failure.recognitionStuck }
        var hits: [SeekHit] = []
        for obs in results {
            guard let cand = obs.topCandidates(1).first else { continue }
            let s = cand.string
            let b = obs.boundingBox
            // Vision: normalized, origin bottom-left → top-left normalized, then screen points.
            let nx = b.origin.x
            let ny = 1 - b.origin.y - b.size.height
            let nw = b.size.width
            let nh = b.size.height
            let words = wordBoxes(cand, in: s, monitor: monitor)
            hits.append(SeekHit(
                label: s,
                x: monitor.x + nx * monitor.w,
                y: monitor.y + ny * monitor.h,
                w: nw * monitor.w,
                h: nh * monitor.h,
                words: words
            ))
        }
        if hits.isEmpty { throw Failure.noText }
        return SeekScan(monitor: monitor, hits: hits)
    }

    /// Per-word x extents (UTF-16 offsets), matching the old JXA Vision script.
    private static func wordBoxes(_ cand: VNRecognizedText, in s: String, monitor: SeekRect) -> [SeekWord] {
        let ns = s as NSString
        guard let re = try? NSRegularExpression(pattern: "\\S+") else { return [] }
        var words: [SeekWord] = []
        re.enumerateMatches(in: s, options: [], range: NSRange(location: 0, length: ns.length)) { match, _, _ in
            guard let match, let range = Range(match.range, in: s) else { return }
            guard let box = try? cand.boundingBox(for: range)?.boundingBox else { return }
            words.append(SeekWord(
                at: match.range.location,
                len: match.range.length,
                x: monitor.x + box.origin.x * monitor.w,
                w: box.size.width * monitor.w
            ))
        }
        return words
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
}

struct SeekScan {
    var monitor: SeekRect
    var hits: [SeekHit]
}

struct SeekSpan: Identifiable {
    var id: Int
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
        self.init(label: label, x: rect.x, y: rect.y, w: rect.w, h: rect.h, words: words)
    }
}

extension SeekScan {
    init?(_ raw: Any?) {
        guard let raw = raw as? [String: Any], let monitor = SeekRect(raw["monitor"]) else { return nil }
        self.init(monitor: monitor, hits: (raw["hits"] as? [[String: Any]])?.compactMap(SeekHit.init) ?? [])
    }
}

/// Screen x of character index `i`. Word boxes when Vision sent them, otherwise an even split.
func charX(_ hit: SeekHit, _ i: Int) -> CGFloat {
    if let word = hit.words.last(where: { $0.at <= i }) {
        guard word.len > 0 else { return word.x }
        let span = min(i - word.at, word.len)
        return word.x + (CGFloat(span) / CGFloat(word.len)) * word.w
    }
    let n = CGFloat(max(hit.label.utf16.count, 1))
    return hit.x + (hit.w / n) * CGFloat(i)
}

func matchSpans(scan: SeekScan, query: String) -> [SeekSpan] {
    let needle = Array(query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased().utf16)
    if needle.isEmpty { return [] }
    var out: [SeekSpan] = []
    for hit in scan.hits {
        let text = Array(hit.label.lowercased().utf16)
        var i = 0
        while i + needle.count <= text.count {
            if text[i..<(i + needle.count)].elementsEqual(needle) {
                let x0 = charX(hit, i)
                let x1 = charX(hit, i + needle.count)
                out.append(SeekSpan(
                    id: out.count,
                    x: x0 - scan.monitor.x,
                    y: hit.y - scan.monitor.y,
                    w: max(x1 - x0, 4),
                    h: hit.h
                ))
            }
            i += 1
        }
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
    assert(spans[0].x == 20 && spans[0].y == 30 && spans[0].w == 20 && spans[0].h == 10)
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
