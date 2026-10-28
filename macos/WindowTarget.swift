import Foundation

/// Pure helpers for labeling the window a snap/move would affect.
/// Kept AppKit-free so `./test-swift.command` can cover it.
enum WindowTarget {
    /// `windowTitle` from AX; `appName` from `NSRunningApplication.localizedName`.
    /// Empty strings mean "unknown".
    static func label(windowTitle: String, appName: String) -> String? {
        let win = windowTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        let app = appName.trimmingCharacters(in: .whitespacesAndNewlines)
        if win.isEmpty { return app.isEmpty ? nil : app }
        if app.isEmpty || win.caseInsensitiveCompare(app) == .orderedSame { return win }
        return "\(win) · \(app)"
    }

    /// Stamp every `window` command's subtitle with the live target label.
    static func tag(_ commands: [Command], label: String?) -> [Command] {
        guard let label, !label.isEmpty else { return commands }
        return commands.map { cmd in
            guard cmd.kind == .window else { return cmd }
            var cmd = cmd
            cmd.subtitle = label
            return cmd
        }
    }
}
