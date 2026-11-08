import AppKit
import IOKit.pwr_mgt

/// Amphetamine-lite: an IOKit power assertion (no `caffeinate` child process).
/// Lives in the OpenCast status menu as a submenu (one icon, not two).
///
/// Not `@MainActor`-isolated so AppKit can invoke `@objc` menu actions on Tahoe.
final class StayAwakeController: NSObject {
    static let shared = StayAwakeController()

    private(set) var mode = "off"
    private var assertion: IOPMAssertionID = 0
    private var endsAt: Date?
    private var endTimer: Timer?

    /// Title for the OpenCast menu entry: state at a glance, countdown when timed.
    var menuTitle: String {
        guard mode != "off" else { return "Stay Awake" }
        var title = "Stay Awake: \(label(for: mode))"
        if let endsAt {
            let m = Int(max(0, endsAt.timeIntervalSinceNow)) / 60
            title += m > 0 ? " · \(m)m left" : " · ending"
        }
        return title
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
    }

    func shutdown() {
        release()
        mode = "off"
        endsAt = nil
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
                guard let self, self.mode != "off" else { return }
                self.release()
                self.mode = "off"
                self.endsAt = nil
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

    /// Submenu for the OpenCast status menu. Built fresh on every open
    /// (`menuNeedsUpdate`), so the checkmark and countdown are always current.
    func submenu() -> NSMenu {
        let menu = NSMenu()
        add(menu, title: "Off", mode: "off")
        add(menu, title: "Indefinite", mode: "indefinite")
        add(menu, title: "30 minutes", mode: "30m")
        add(menu, title: "1 hour", mode: "1h")
        add(menu, title: "3 hours", mode: "3h")
        return menu
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
        // Inline on main — async can be lost during menu teardown on Tahoe.
        if Thread.isMainThread {
            apply(mode)
        } else {
            DispatchQueue.main.async { self.apply(mode) }
        }
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
