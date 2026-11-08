// Live AppKit checks for Settings. Compiled by ./test-appkit.command.
// These create real windows — Bool flags in panel-logic tests missed Tahoe.
import AppKit
import Foundation

@main
enum AppKitSettingsTests {
    static func main() {
        var checks = 0
        var failures = 0

        func check(_ ok: Bool, _ what: String) {
            checks += 1
            if ok {
                print("  ok   \(what)")
            } else {
                failures += 1
                print(" FAIL  \(what)")
            }
        }

        func pump(_ seconds: TimeInterval = 0.2) {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: seconds))
        }

        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)

        // MARK: Keyable window (plain NSWindow cannot key from accessory)

        let settings = SettingsKeyWindow(
            contentRect: NSRect(x: 80, y: 80, width: 240, height: 160),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        settings.isReleasedWhenClosed = false
        settings.title = "OpenCast Settings Test"
        check(settings.canBecomeKey, "SettingsKeyWindow can become key")
        check(settings.canBecomeMain, "SettingsKeyWindow can become main")
        check(SettingsPresentation.windowLevel == .normal,
              "Settings window level is normal (not floating over other apps)")

        // MARK: Island still up (Tahoe status-item click does not hide it)

        let island = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 120, height: 80),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        island.isReleasedWhenClosed = false
        island.isOpaque = false
        island.level = NSWindow.Level(rawValue: SettingsPresentation.islandLevel)
        island.orderFrontRegardless()
        pump(0.05)
        check(island.isVisible, "island is up before Settings (the Tahoe menu-bar case)")

        SettingsPresentation.prepareApp(app)
        SettingsPresentation.present(settings, hiding: [island])
        pump()

        check(!island.isVisible, "present() orders the island out so it cannot cover Settings")
        check(settings.isVisible, "Settings is visible from .accessory after present()")
        check(settings.level == .normal, "presented Settings stays a normal-level window")

        // MARK: hide() then show() (builtin:settings / handleUi)

        app.hide(nil)
        pump(0.05)
        SettingsPresentation.prepareApp(app)
        SettingsPresentation.present(settings, hiding: [])
        pump()
        check(settings.isVisible, "Settings is visible after NSApp.hide (handleUi settings path)")
        check(app.isHidden == false, "prepareApp unhides the accessory app")

        // MARK: Source gate — StatusBar must not assumeIsolated / Task after menu

        let statusBar = (try? String(contentsOfFile: "macos/StatusBar.swift", encoding: .utf8)) ?? ""
        check(!statusBar.contains("MainActor.assumeIsolated {"),
              "StatusBar.swift does not call MainActor.assumeIsolated (traps on Tahoe)")
        check(!statusBar.contains("Task { @MainActor"),
              "StatusBar.swift does not use Task { @MainActor } after menu hops")
        check(statusBar.contains("SettingsWindowController.shared.show()"),
              "menu Settings calls SettingsWindowController.show inline")
        check(!MenuBarSymbol.usesAssumeIsolatedAfterMenu,
              "policy: no assumeIsolated after AppKit menu hops")
        check(!MenuBarSymbol.usesTaskAfterMenu,
              "policy: no Task after AppKit menu hops")

        settings.close()
        island.close()

        print()
        if failures == 0 {
            print("\(checks)/\(checks) AppKit Settings checks passed")
            exit(0)
        } else {
            print("\(failures)/\(checks) AppKit Settings checks failed")
            exit(1)
        }
    }
}
