import AppKit
import SwiftUI

// ponytail: App bootstrap / panel host types peeled from OpenCastApp.swift — move-only.
final class IslandPanel: NSPanel {
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 200),
            styleMask: [.borderless],
            backing: .buffered,
            defer: true
        )
        isOpaque = false
        // Hair of alpha — fully clear backing can drop SwiftUI fills on Tahoe.
        backgroundColor = NSColor.black.withAlphaComponent(OverlayPanelPolicy.backingAlpha)
        hasShadow = true
        level = NSWindow.Level(rawValue: 25)
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = false
        isReleasedWhenClosed = false
        isMovableByWindowBackground = false
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        acceptsMouseMovedEvents = true
    }

    /// The match overlay must not take the key window from the search island.
    var refusesKey = false

    override var canBecomeKey: Bool { !refusesKey }
    override var canBecomeMain: Bool { true }
}

final class ClearHost<Content: View>: NSHostingView<Content> {
    override var isOpaque: Bool { false }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        wantsLayer = true
        layer?.isOpaque = false
        // Match IslandPanel: full clear can erase SwiftUI fills on Tahoe overlays.
        layer?.backgroundColor = NSColor.black.withAlphaComponent(OverlayPanelPolicy.backingAlpha).cgColor
        if #available(macOS 13.0, *) {
            sizingOptions = []
        }
    }
}

/// One copy at a time. Two panels mean two global-hotkey grabbers, and the
/// second copy's hotkeys would silently do nothing; they also fight over the
/// usage/pins files. The lock is held by the fd until the process exits.
enum SingleInstance {
    private static var fd: Int32 = -1

    /// Takes the lock, or returns false when another OpenCast holds it.
    static func acquire() -> Bool {
        let dir = ProcessInfo.processInfo.environment["OPENCAST_CONFIG_DIR"]
            ?? (NSHomeDirectory() as NSString).appendingPathComponent(".config/opencast")
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let path = (dir as NSString).appendingPathComponent(".instance.lock")
        let handle = open(path, O_RDWR | O_CREAT | O_CLOEXEC, S_IRUSR | S_IWUSR)
        guard handle >= 0 else { return true } // no lockfile possible: do not block the app
        if flock(handle, LOCK_EX | LOCK_NB) != 0 {
            close(handle)
            return false
        }
        fd = handle
        return true
    }
}

@main
struct OpenCastApp {
    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.setActivationPolicy(.accessory)
        app.delegate = delegate
        guard SingleInstance.acquire() else {
            FileHandle.standardError.write("OpenCast is already running — this copy is quitting\n".data(using: .utf8) ?? Data())
            NSApp.activate(ignoringOtherApps: true)
            let alert = NSAlert()
            alert.messageText = "OpenCast is already running"
            // Engine is not started yet on this path; default hotkey matches examples/config.toml.
            alert.informativeText = "OpenCast is already running. Press ⌥Space to open it."
            alert.alertStyle = .warning
            alert.runModal()
            return
        }
        app.run()
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // LSUIElement / .accessory has no default menu; Edit items make Cmd+C/V/X/A work in fields.
        Self.installEditMenu()
        MainActor.assumeIsolated {
            Model.shared.install()
            OpenCastStatusBar.shared.install()
            StayAwakeController.shared.install()
            ClipboardHistory.shared.install()
        }
        Task.detached {
            opencast_start(opencastCallback)
            let dir = Engine.call(["op": "configDir"])["path"] as? String ?? ""
            let settings = Engine.call(["op": "settings"])
            await MainActor.run {
                if let p = settings["presentation"] as? String { Model.shared.presentation = p }
                Model.shared.refreshSettings()
                Model.shared.ready = true
                // First show happens here, with the right presentation (the engine used to show too early).
                Model.shared.show()
                // Don't rely on LauncherView.onChange(of: ready): if the view mounts after
                // ready is already true (common on slower Macs), that change never fires and
                // the list stays on "No results" until something else triggers search.
                Model.shared.search(immediate: true)
                Model.shared.checkHotkeys()
                Model.shared.prefetchAppIcons()
                SnippetExpander.shared.reload()
                ConfigWatcher.shared.start(configDir: dir)
                if Schedule.shared.authorized { Schedule.shared.refresh(force: true) }
                AppDelegate.onboardIfNeeded()
                if settings["checkUpdates"] as? Bool == true { AppDelegate.weeklyUpdateCheck() }
            }
            let warnings = Engine.call(["op": "warnings"])["warnings"] as? [String] ?? []
            await MainActor.run { Model.shared.showWarnings(warnings) }
        }
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        MainActor.assumeIsolated {
            for url in urls {
                Model.shared.handleDeepLink(url)
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        MainActor.assumeIsolated {
            StayAwakeController.shared.shutdown()
            ClipboardHistory.shared.shutdown()
        }
        // Undo the Caps Lock remap (hyper key) and anything else that outlives us.
        _ = Engine.call(["op": "shutdown"])
    }

    /// First run: panel-first tips (hotkey, Accessibility, optional AI).
    @MainActor
    static func onboardIfNeeded() {
        let key = "OpenCastOnboarded"
        guard !UserDefaults.standard.bool(forKey: key) else { return }
        // Mark when shown (covers Esc skip via the same key).
        UserDefaults.standard.set(true, forKey: key)
        Model.shared.showWelcome()
    }

    /// Opt-in (`check_updates = true`): at most once a week, quietly.
    @MainActor
    static func weeklyUpdateCheck() {
        let key = "OpenCastLastUpdateCheck"
        let last = UserDefaults.standard.double(forKey: key)
        guard Date().timeIntervalSince1970 - last > 7 * 86_400 else { return }
        UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: key)
        Task.detached {
            let r = Engine.call(["op": "updateCheck"])
            guard let version = r["newer"] as? String else { return }
            await MainActor.run {
                Model.shared.flash("OpenCast \(version) is available — run Check for Updates", error: false, thenHide: false)
            }
        }
    }

    /// ponytail: Edit menu only — enough for text-field key equivalents; full app menu if we leave accessory.
    private static func installEditMenu() {
        let main = NSMenu()
        let edit = NSMenuItem()
        let submenu = NSMenu(title: "Edit")
        submenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        submenu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        submenu.addItem(.separator())
        submenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        submenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        submenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        submenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        edit.submenu = submenu
        main.addItem(edit)
        NSApp.mainMenu = main
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
