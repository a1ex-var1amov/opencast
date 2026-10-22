import Foundation

/// Which snippet (if any) the typed text just completed. Pure so the matching
/// rules can be tested without a keyboard tap: `./test-swift.command`.
enum SnippetMatch {
    /// The longest keyword `buffer` ends with.
    ///
    /// Longest wins on purpose: with `;a` and `;ia` both defined, typing `;ia`
    /// ends both. Iterating the dictionary used to pick whichever came first,
    /// so `;a` could expand and leave an `i` behind.
    static func longest(_ buffer: String, keywords: some Sequence<String>) -> String? {
        guard !buffer.isEmpty else { return nil }
        var best: String?
        for k in keywords where !k.isEmpty && buffer.hasSuffix(k) {
            if best == nil || k.count > best!.count { best = k }
        }
        return best
    }

    /// Keep the typed buffer short: enough to match the longest keyword plus one
    /// word's worth, so a long stream of keystrokes costs nothing.
    static func trim(_ buffer: String, longestKeyword: Int) -> String {
        let limit = max(longestKeyword, 1) * 2
        guard buffer.count > limit else { return buffer }
        return String(buffer.suffix(max(longestKeyword, 1)))
    }
}
