import AppKit
import ApplicationServices

/// Search the front app's menu bar (like Alfred's / Raycast's "Search Menu
/// Items") through the Accessibility API, and press the chosen item.
enum MenuSearch {
    struct Item: @unchecked Sendable {
        var title: String
        /// `File › Export › PDF…`
        var path: String
        /// `⌘⇧E`, empty when none.
        var shortcut: String
        var element: AXUIElement
    }

    /// Walk the menu bar of `pid` (skipping the Apple menu). Safe off the main thread.
    static func collect(pid: pid_t, maxItems: Int = 3000) -> [Item] {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 1.0)
        guard let bar: AXUIElement = attribute(app, kAXMenuBarAttribute) else { return [] }
        let tops: [AXUIElement] = attribute(bar, kAXChildrenAttribute) ?? []
        var out: [Item] = []
        for (i, top) in tops.enumerated() where i > 0 { // 0 = Apple menu
            let title: String = attribute(top, kAXTitleAttribute) ?? ""
            guard !title.isEmpty else { continue }
            for menu in (attribute(top, kAXChildrenAttribute) as [AXUIElement]?) ?? [] {
                walk(menu, path: [title], depth: 0, into: &out, max: maxItems)
            }
            if out.count >= maxItems { break }
        }
        return out
    }

    private static func walk(_ menu: AXUIElement, path: [String], depth: Int, into out: inout [Item], max: Int) {
        guard depth < 5, out.count < max else { return }
        let children: [AXUIElement] = attribute(menu, kAXChildrenAttribute) ?? []
        for child in children {
            let title: String = attribute(child, kAXTitleAttribute) ?? ""
            guard !title.isEmpty else { continue } // separators
            let submenus: [AXUIElement] = attribute(child, kAXChildrenAttribute) ?? []
            if let sub = submenus.first {
                walk(sub, path: path + [title], depth: depth + 1, into: &out, max: max)
                continue
            }
            let enabled: Bool = attribute(child, kAXEnabledAttribute) ?? true
            guard enabled else { continue }
            out.append(Item(
                title: title,
                path: (path + [title]).joined(separator: " › "),
                shortcut: shortcut(of: child),
                element: child
            ))
        }
    }

    private static func shortcut(of item: AXUIElement) -> String {
        guard let char: String = attribute(item, kAXMenuItemCmdCharAttribute), !char.isEmpty else { return "" }
        let mods: Int = attribute(item, kAXMenuItemCmdModifiersAttribute) ?? 0
        // kAXMenuItemModifier*: 1 shift, 2 option, 4 control, 8 = no command key.
        var s = ""
        if mods & 4 != 0 { s += "⌃" }
        if mods & 2 != 0 { s += "⌥" }
        if mods & 1 != 0 { s += "⇧" }
        if mods & 8 == 0 { s += "⌘" }
        return s + char.uppercased()
    }

    /// Bring the app forward and press the item.
    @MainActor
    static func press(_ item: Item, in app: NSRunningApplication?) {
        app?.activate()
        let element = item.element
        Task.detached {
            try? await Task.sleep(nanoseconds: 80_000_000)
            AXUIElementPerformAction(element, kAXPressAction as CFString)
        }
    }

    private static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success, let value else { return nil }
        return value as? T
    }
}
