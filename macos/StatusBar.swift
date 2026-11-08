import AppKit

/// The one OpenCast menu-bar item: a star that fills in while Stay Awake is
/// on (submenu to pick a duration), Focus as a countdown badge plus Pause /
/// Stop entries while a session runs.
///
/// Not `@MainActor`-isolated: AppKit invokes `@objc` menu actions off that
/// isolation on some macOS versions (Tahoe), which left every item inert.
final class OpenCastStatusBar: NSObject, NSMenuDelegate {
    static let shared = OpenCastStatusBar()

    private var statusItem: NSStatusItem?

    /// Focus session shown on the one OpenCast item: minutes beside the glyph,
    /// Pause / Stop in the menu. Pushed by FocusController; nil when idle.
    struct FocusBadge {
        let minutesLeft: Int
        let paused: Bool
        let goal: String
    }

    private var focus: FocusBadge?
    private var awake = false

    func setFocus(_ badge: FocusBadge?) {
        focus = badge
        refresh()
    }

    /// Pushed by StayAwakeController on every mode change.
    func setAwake(_ on: Bool) {
        awake = on
        refresh()
    }

    /// Same star always: outline idle, filled while Stay Awake is on.
    /// Focus adds its countdown as the title; width grows only then.
    private func refresh() {
        guard let item = statusItem, let button = item.button else { return }
        button.image = MenuBarSymbol.statusImage(awake: awake)
        var tips = ["OpenCast"]
        if awake { tips.append(StayAwakeController.shared.menuTitle) }
        if let focus {
            button.title = focus.paused ? "\(focus.minutesLeft)m paused" : "\(focus.minutesLeft)m"
            button.imagePosition = .imageLeading
            tips.append(focus.goal.isEmpty ? "Focus" : "Focus: \(focus.goal)")
        } else {
            button.title = ""
            button.imagePosition = .imageOnly
        }
        item.length = focus == nil ? NSStatusItem.squareLength : NSStatusItem.variableLength
        button.toolTip = tips.joined(separator: " · ")
    }

    func install() {
        guard statusItem == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        // Never force .labelColor — see MenuBarSymbol.forcesAppLabelTint.
        precondition(!MenuBarSymbol.forcesAppLabelTint)
        precondition(!MenuBarSymbol.usesTaskAfterMenu)
        precondition(!MenuBarSymbol.usesAssumeIsolatedAfterMenu)
        item.button?.image = MenuBarSymbol.statusImage(awake: awake)
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
        if let f = focus {
            var title = f.goal.isEmpty ? "Focus" : "Focus: \(f.goal)"
            title += f.paused ? " · paused, \(f.minutesLeft)m left" : " · \(f.minutesLeft)m left"
            let header = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            header.isEnabled = false
            menu.addItem(header)
            let pause = NSMenuItem(
                title: f.paused ? "Resume Focus" : "Pause Focus",
                action: #selector(FocusMenuTarget.pause),
                keyEquivalent: ""
            )
            pause.target = FocusMenuTarget.shared
            menu.addItem(pause)
            let stop = NSMenuItem(title: "Stop Focus", action: #selector(FocusMenuTarget.stop), keyEquivalent: "")
            stop.target = FocusMenuTarget.shared
            menu.addItem(stop)
            menu.addItem(.separator())
        }
        // One icon: Stay Awake rides along as a submenu instead of its own status item.
        let awake = NSMenuItem(title: StayAwakeController.shared.menuTitle, action: nil, keyEquivalent: "")
        awake.submenu = StayAwakeController.shared.submenu()
        menu.addItem(awake)
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
        // afterDelay already left menu tracking. MainActor.assumeIsolated traps
        // here on Tahoe (AppKit @objc ≠ MainActor); Task is dropped. Main-queue
        // async is the hop onto Model's actor without trapping.
        DispatchQueue.main.async { Model.shared.show() }
    }

    @objc private func openSettingsNow() {
        // SettingsWindowController is deliberately not @MainActor.
        SettingsWindowController.shared.show()
    }

    @objc private func runDoctorNow() {
        DispatchQueue.main.async { Model.shared.perform("builtin:doctor") }
    }

    @objc private func openKeysNow() {
        DispatchQueue.main.async { Model.shared.perform("builtin:shortcuts") }
    }

    @objc private func openWelcomeNow() {
        DispatchQueue.main.async { Model.shared.perform("builtin:welcome") }
    }

    @objc private func quitAppNow() {
        NSApp.terminate(nil)
    }
}
