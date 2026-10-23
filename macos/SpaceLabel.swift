import Foundation

// Labels for Mission Control spaces. Kept free of AppKit so it is covered by
// test-swift.command.

enum SpaceLabels {
    /// Menu-bar helpers, wallpaper and our own panel make the list unreadable.
    static let noise: Set<String> = [
        "Window Server", "WindowManager", "Wallpaper", "Dock", "Control Center", "Siri",
        "Spotlight", "SystemUIServer", "TextInputMenuAgent", "Notification Center",
        "UserNotificationCenter", "OpenCast", "Finder", "CoreLocationAgent", "Dock Helper",
    ]

    /// Window owners worth showing: deduped, in the order they were listed.
    static func visibleApps(_ names: [String]) -> [String] {
        var seen = Set<String>()
        return names.filter { !noise.contains($0) && seen.insert($0).inserted }
    }

    /// One line under a space: what runs there, capped so the row stays readable.
    static func summary(_ names: [String], fullscreen: Bool) -> String {
        let keep = visibleApps(names)
        if keep.isEmpty { return fullscreen ? "" : "Empty" }
        if keep.count > 4 { return keep.prefix(4).joined(separator: ", ") + " +\(keep.count - 4)" }
        return keep.joined(separator: ", ")
    }
}

/// Why a space command could not run; the panel shows `message`.
struct SpacesError: Error, CustomStringConvertible {
    let message: String
    var description: String { message }
}
