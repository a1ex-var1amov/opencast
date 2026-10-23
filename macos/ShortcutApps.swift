import Foundation

/// A running app as the shortcut drill-down sees it.
struct ShortcutAppEntry: Equatable {
    let pid: pid_t
    let name: String
}

/// Which apps the "App Menu Shortcuts" list offers, and in what order.
/// Only apps with a menu bar are candidates — the caller knows the activation
/// policy, so this stays a pure ordering problem.
enum ShortcutApps {
    /// The app you were using comes first, then A→Z by name. Two instances of
    /// the same app collapse to one row (the one you were in wins).
    static func rows(_ entries: [ShortcutAppEntry], front: pid_t?) -> [ShortcutAppEntry] {
        var byName: [String: ShortcutAppEntry] = [:]
        var order: [String] = []
        for entry in entries {
            let key = entry.name.lowercased()
            guard let existing = byName[key] else {
                byName[key] = entry
                order.append(key)
                continue
            }
            // Replace a background instance with the one you were actually in.
            if existing.pid != front, entry.pid == front { byName[key] = entry }
        }
        let kept = order.compactMap { byName[$0] }
        return kept.sorted { a, b in
            if a.pid == front { return b.pid != front }
            if b.pid == front { return false }
            return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
        }
    }
}
