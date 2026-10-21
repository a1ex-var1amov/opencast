import AppKit

/// Amphetamine-lite: `caffeinate -i` plus a menu-bar state dot.
@MainActor
final class StayAwakeController: NSObject {
    static let shared = StayAwakeController()

    private(set) var mode = "off"
    private var process: Process?
    private var endsAt: Date?
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
        stopProcess()
        self.mode = mode
        endsAt = nil
        switch mode {
        case "off":
            break
        case "indefinite":
            start(args: ["-i"])
        case "30m":
            endsAt = Date().addingTimeInterval(30 * 60)
            start(args: ["-i", "-t", "1800"])
        case "1h":
            endsAt = Date().addingTimeInterval(60 * 60)
            start(args: ["-i", "-t", "3600"])
        case "3h":
            endsAt = Date().addingTimeInterval(3 * 60 * 60)
            start(args: ["-i", "-t", "10800"])
        default:
            self.mode = "off"
        }
        rebuildMenu()
        refreshButton()
        scheduleTick()
    }

    func shutdown() {
        stopProcess()
        mode = "off"
        endsAt = nil
        tick?.invalidate()
        tick = nil
        if let item = statusItem {
            NSStatusBar.system.removeStatusItem(item)
            statusItem = nil
        }
    }

    private func start(args: [String]) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/caffeinate")
        p.arguments = args
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        p.terminationHandler = { [weak self] proc in
            Task { @MainActor in
                guard let self, self.process == proc else { return }
                self.process = nil
                if self.mode != "off" {
                    self.mode = "off"
                    self.endsAt = nil
                    self.rebuildMenu()
                    self.refreshButton()
                    self.scheduleTick()
                }
            }
        }
        do {
            try p.run()
            process = p
        } catch {
            mode = "off"
            endsAt = nil
            process = nil
        }
    }

    private func stopProcess() {
        guard let p = process else { return }
        process = nil
        if p.isRunning { p.terminate() }
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
        apply(mode)
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
