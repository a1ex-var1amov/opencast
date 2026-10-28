import AppKit
import IOKit.pwr_mgt

/// Amphetamine-lite: an IOKit power assertion (no `caffeinate` child process)
/// plus a menu-bar state dot.
///
/// Not `@MainActor`-isolated so AppKit can invoke `@objc` menu actions on Tahoe.
final class StayAwakeController: NSObject {
    static let shared = StayAwakeController()

    private(set) var mode = "off"
    private var assertion: IOPMAssertionID = 0
    private var endsAt: Date?
    private var endTimer: Timer?
    private var statusItem: NSStatusItem?
    private var tick: Timer?

    func install() {
        guard statusItem == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.imagePosition = .imageOnly
        item.button?.toolTip = "OpenCast Stay Awake"
        statusItem = item
        rebuildMenu()
        refreshButton()
    }

    func apply(_ mode: String) {
        release()
        self.mode = mode
        endsAt = nil
        let seconds: TimeInterval? = switch mode {
        case "off": nil
        case "indefinite": 0
        case "30m": 30 * 60
        case "1h": 60 * 60
        case "3h": 3 * 60 * 60
        default: nil
        }
        if let seconds {
            if seconds > 0 { endsAt = Date().addingTimeInterval(seconds) }
            acquire(seconds: seconds)
        } else {
            self.mode = "off"
        }
        rebuildMenu()
        refreshButton()
        scheduleTick()
    }

    func shutdown() {
        release()
        mode = "off"
        endsAt = nil
        tick?.invalidate()
        tick = nil
        if let item = statusItem {
            NSStatusBar.system.removeStatusItem(item)
            statusItem = nil
        }
    }

    /// `kIOPMAssertionTypePreventUserIdleSystemSleep` is what `caffeinate -i` takes.
    private func acquire(seconds: TimeInterval) {
        var id: IOPMAssertionID = 0
        let reason = "OpenCast Stay Awake" as CFString
        let status = seconds > 0
            ? IOPMAssertionCreateWithDescription(
                kIOPMAssertionTypePreventUserIdleSystemSleep as CFString, reason, nil, nil, nil, seconds,
                kIOPMAssertionTimeoutActionRelease as CFString, &id
            )
            : IOPMAssertionCreateWithName(
                kIOPMAssertionTypePreventUserIdleSystemSleep as CFString, IOPMAssertionLevel(kIOPMAssertionLevelOn), reason, &id
            )
        guard status == kIOReturnSuccess else {
            mode = "off"
            endsAt = nil
            return
        }
        assertion = id
        endTimer?.invalidate()
        endTimer = nil
        if seconds > 0 {
            endTimer = Timer.scheduledTimer(withTimeInterval: seconds, repeats: false) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.mode != "off" else { return }
                    self.release()
                    self.mode = "off"
                    self.endsAt = nil
                    self.rebuildMenu()
                    self.refreshButton()
                    self.scheduleTick()
                }
            }
        }
    }

    private func release() {
        endTimer?.invalidate()
        endTimer = nil
        guard assertion != 0 else { return }
        IOPMAssertionRelease(assertion)
        assertion = 0
    }

    private func scheduleTick() {
        tick?.invalidate()
        tick = nil
        guard mode != "off", endsAt != nil else { return }
        tick = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.rebuildMenu()
                self?.refreshButton()
            }
        }
    }

    private func refreshButton() {
        guard let button = statusItem?.button else { return }
        let on = mode != "off"
        // Always template so contentTintColor applies; non-template fills vanish on the menu bar.
        let image = NSImage(systemSymbolName: on ? "circle.fill" : "circle", accessibilityDescription: "Stay Awake")
        image?.isTemplate = true
        button.image = image
        button.contentTintColor = on ? .controlAccentColor : .secondaryLabelColor
        button.toolTip = on ? "Stay Awake: \(label(for: mode))" : "Stay Awake: Off"
    }

    private func rebuildMenu() {
        let menu = NSMenu()
        if mode != "off" {
            var title = "Stay Awake: \(label(for: mode))"
            if let endsAt {
                let left = max(0, endsAt.timeIntervalSinceNow)
                let m = Int(left) / 60
                title += m > 0 ? " · \(m)m left" : " · ending"
            }
            let header = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            header.isEnabled = false
            menu.addItem(header)
            menu.addItem(.separator())
        }
        add(menu, title: "Off", mode: "off")
        add(menu, title: "Indefinite", mode: "indefinite")
        add(menu, title: "30 minutes", mode: "30m")
        add(menu, title: "1 hour", mode: "1h")
        add(menu, title: "3 hours", mode: "3h")
        statusItem?.menu = menu
    }

    private func add(_ menu: NSMenu, title: String, mode: String) {
        let item = NSMenuItem(title: title, action: #selector(pick(_:)), keyEquivalent: "")
        item.target = self
        item.representedObject = mode
        item.state = self.mode == mode ? .on : .off
        menu.addItem(item)
    }

    @objc private func pick(_ sender: NSMenuItem) {
        guard let mode = sender.representedObject as? String else { return }
        // DispatchQueue.main — not Task { @MainActor }: Tahoe drops those from @objc.
        DispatchQueue.main.async { self.apply(mode) }
    }

    private func label(for mode: String) -> String {
        switch mode {
        case "indefinite": "Indefinite"
        case "30m": "30 minutes"
        case "1h": "1 hour"
        case "3h": "3 hours"
        default: "Off"
        }
    }
}
