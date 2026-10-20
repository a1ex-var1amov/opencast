import SwiftUI

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
            ForEach(spans) { span in
                hit(span, current: span.id == cursor)
            }
        }
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
