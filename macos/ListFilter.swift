import Foundation

/// Typing inside a panel-side list: menus, spaces, schedule, Script Filters.
/// Those rows are built here, not by the engine, so the engine's fuzzy search
/// never sees them. Without this, a list filtered "New" out of "New Browser
/// Window" would also be the only way to find it — one substring, no ranking.
/// This scores the same idea the Rust search uses: an exact substring wins,
/// otherwise every query character has to appear in order, word starts count
/// for extra and skipped characters cost.
enum ListFilter {
    /// Score for one row; nil means the row does not match at all.
    static func score(title: String, subtitle: String?, hotkey: String?, query: String) -> Int? {
        let q = normalize(query)
        if q.isEmpty { return 0 }
        var best: Int?
        // A key equivalent is part of the row: "⌘t" or "cmd t" should find it.
        for (hay, base) in [(normalize(title), 1000), (normalize(subtitle ?? ""), 400), (normalize(hotkey ?? ""), 200)]
        where !hay.isEmpty {
            if let exact = exactScore(hay, q, base: base) { best = max(best ?? 0, exact) }
            if let seq = sequenceScore(hay, q, base: base - 250) { best = max(best ?? 0, seq) }
        }
        return best
    }

    /// Rows that match, best first; equal scores keep the order they arrived in,
    /// so a list the author already ranked (spaces, recents) stays ranked.
    /// Indices point back into the original array: callers map a row to its data.
    static func order<T>(_ items: [T], query: String, hotkey: (T) -> String? = { _ in nil },
                         key: (T) -> (title: String, subtitle: String?)) -> [(index: Int, item: T)] {
        guard !normalize(query).isEmpty else {
            return items.enumerated().map { ($0.offset, $0.element) }
        }
        let scored = items.enumerated().compactMap { (i, item) -> (score: Int, index: Int, item: T)? in
            let (title, subtitle) = key(item)
            guard let s = score(title: title, subtitle: subtitle, hotkey: hotkey(item), query: query) else { return nil }
            return (s, i, item)
        }
        return scored.sorted { a, b in a.score != b.score ? a.score > b.score : a.index < b.index }
            .map { ($0.index, $0.item) }
    }

    /// Convenience for a list of plain strings.
    static func order(_ items: [String], query: String) -> [(index: Int, item: String)] {
        order(items, query: query, key: { ($0, nil) })
    }

    private static func normalize(_ s: String) -> String {
        s.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// An exact substring: earlier is better, a whole-word or whole-row match
    /// beats a match in the middle of a word.
    private static func exactScore(_ hay: String, _ q: String, base: Int) -> Int? {
        guard let r = hay.range(of: q) else { return nil }
        let pos = hay.distance(from: hay.startIndex, to: r.lowerBound)
        var score = base + 200 - pos * 8
        let atStart = r.lowerBound == hay.startIndex
        if atStart { score += 120 }
        if atStart && r.upperBound == hay.endIndex { score += 250 }
        if atStart || isBreak(before: r.lowerBound, in: hay) { score += 60 }
        return score
    }

    /// Every query character appears in order. Word starts are worth extra —
    /// "nbrw" for "New Browser Window" is how people actually type.
    private static func sequenceScore(_ hay: String, _ q: String, base: Int) -> Int? {
        var score = base
        var gaps = 0
        var from = hay.startIndex
        var previous: String.Index?
        for ch in q {
            guard let hit = hay[from...].firstIndex(of: ch) else { return nil }
            if let prev = previous {
                gaps += max(0, hay.distance(from: prev, to: hit) - 1)
            }
            if isBreak(before: hit, in: hay) { score += 45 }
            previous = hit
            from = hay.index(after: hit)
        }
        return score - gaps * 4
    }

    /// Word boundary: the start of the row, or after a separator.
    private static func isBreak(before i: String.Index, in hay: String) -> Bool {
        if i == hay.startIndex { return true }
        let prev = hay[hay.index(before: i)]
        return " ·-_/,()[]{}'\"|:".contains(prev)
    }
}
