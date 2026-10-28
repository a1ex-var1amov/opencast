import AppKit

/// Persistent menu-bar entry for OpenCast itself (not Focus / Stay Awake).
///
/// Not `@MainActor`-isolated: AppKit invokes `@objc` menu actions off that
/// isolation on some macOS versions (Tahoe), which left every item inert.
final class OpenCastStatusBar: NSObject, NSMenuDelegate {
    static let shared = OpenCastStatusBar()

    private var statusItem: NSStatusItem?

    func install() {
        guard statusItem == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        // Multicolor SF Symbols ignore menu-bar template ink unless monochrome.
        // Do not set contentTintColor — .labelColor follows the app appearance and
        // paints black while system items stay white on a dark menu bar.
        let conf = NSImage.SymbolConfiguration(pointSize: 15, weight: .medium)
            .applying(.preferringMonochrome())
        let image = NSImage(systemSymbolName: "sparkles", accessibilityDescription: "OpenCast")?
            .withSymbolConfiguration(conf)
            ?? NSImage(size: NSSize(width: 16, height: 16))
        image.isTemplate = true
        item.button?.image = image
        item.button?.contentTintColor = nil
        item.button?.imagePosition = .imageOnly
        item.button?.toolTip = "OpenCast"
        let menu = NSMenu()
        menu.delegate = self
        item.menu = menu
        statusItem = item
        // Menu filled on first open: install runs before opencast_start.
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        rebuild(menu)
    }

    private func rebuild(_ menu: NSMenu) {
        menu.removeAllItems()
        let open = NSMenuItem(
            title: "Open (\(Self.hotkeyLabel()))",
            action: #selector(openPanel),
            keyEquivalent: ""
        )
        open.target = self
        menu.addItem(open)
        let settings = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: "")
        settings.target = self
        menu.addItem(settings)
        let doctor = NSMenuItem(title: "OpenCast Doctor", action: #selector(runDoctor), keyEquivalent: "")
        doctor.target = self
        menu.addItem(doctor)
        let keys = NSMenuItem(title: "OpenCast Keys", action: #selector(openKeys), keyEquivalent: "")
        keys.target = self
        menu.addItem(keys)
        let welcome = NSMenuItem(title: "Welcome…", action: #selector(openWelcome), keyEquivalent: "")
        welcome.target = self
        menu.addItem(welcome)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit", action: #selector(quitApp), keyEquivalent: "")
        quit.target = self
        menu.addItem(quit)
    }

    /// `Alt+Space` → `⌥Space`; falls back when config is not ready yet.
    static func hotkeyLabel() -> String {
        let general = Engine.call(["op": "configGet"])["general"] as? [String: Any]
        let raw = (general?["hotkey"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let raw, !raw.isEmpty else { return "⌥Space" }
        let parts = raw.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
        var out = ""
        if parts.contains("ctrl") || parts.contains("control") { out += "⌃" }
        if parts.contains("alt") || parts.contains("option") { out += "⌥" }
        if parts.contains("shift") { out += "⇧" }
        if parts.contains("cmd") || parts.contains("command") { out += "⌘" }
        let key = parts.last.map { String($0) } ?? "space"
        switch key {
        case "space": out += "Space"
        case "return", "enter": out += "↩"
        case "tab": out += "Tab"
        case "escape", "esc": out += "Esc"
        default: out += key.uppercased()
        }
        return out.isEmpty ? "⌥Space" : out
    }

    /// Run after the menu finishes dismissing. `afterDelay: 0` is the AppKit
    /// hop that survives Tahoe; Swift `Task` / `MainActor.assumeIsolated` do not.
    private func afterMenu(_ sel: Selector) {
        NSObject.cancelPreviousPerformRequests(withTarget: self, selector: sel, object: nil)
        perform(sel, with: nil, afterDelay: 0)
    }

    @objc private func openPanel() { afterMenu(#selector(openPanelNow)) }
    @objc private func openSettings() { afterMenu(#selector(openSettingsNow)) }
    @objc private func runDoctor() { afterMenu(#selector(runDoctorNow)) }
    @objc private func openKeys() { afterMenu(#selector(openKeysNow)) }
    @objc private func openWelcome() { afterMenu(#selector(openWelcomeNow)) }
    @objc private func quitApp() { afterMenu(#selector(quitAppNow)) }

    @objc private func openPanelNow() {
        Task { @MainActor in Model.shared.show() }
    }

    @objc private func openSettingsNow() {
        // SettingsWindowController is deliberately not @MainActor.
        SettingsWindowController.shared.show()
    }

    @objc private func runDoctorNow() {
        Task { @MainActor in Model.shared.perform("builtin:doctor") }
    }

    @objc private func openKeysNow() {
        Task { @MainActor in Model.shared.perform("builtin:shortcuts") }
    }

    @objc private func openWelcomeNow() {
        Task { @MainActor in Model.shared.perform("builtin:welcome") }
    }

    @objc private func quitAppNow() {
        NSApp.terminate(nil)
    }
}
