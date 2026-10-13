import AppKit
import Observation
import QuartzCore
import SwiftUI

/// C callback. Copies the JSON before returning. `hide` finishes on the main
/// thread before this returns and must not call back into Rust.
func opencastCallback(_ ptr: UnsafePointer<CChar>?) {
    guard let ptr else { return }
    let raw = String(cString: ptr)
    let hide = (try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any])?["type"] as? String == "hide"
    let deliver = {
        MainActor.assumeIsolated {
            Model.shared.accept(raw)
        }
    }
    if hide {
        if Thread.isMainThread { deliver() } else { DispatchQueue.main.sync(execute: deliver) }
    } else {
        DispatchQueue.main.async(execute: deliver)
    }
}

@MainActor
@Observable
final class Model {
    static let shared = Model()

    enum Mode: Equatable {
        case search
        case args(Command)
        case output(title: String, text: String)
        case chat(prompt: String?, autoSend: Bool)
    }

    enum Row: Identifiable, Equatable {
        case command(Command)
        case ask

        var id: String {
            switch self {
            case .command(let command): command.id
            case .ask: "ask"
            }
        }
    }

    var query = ""
    var commands: [Command] = []
    var selected = 0
    var busy = false
    var mode: Mode = .search
    var ready = false
    var focusTick = 0
    var toastText: String?
    var toastIsError = false
    var seekVisible = false
    var appIcons: [String: NSImage] = [:]
    /// Raw callback JSON kept for later tasks.
    var stashedSeek: String?
    var stashedAI: String?

    private var launcher: IslandPanel?
    private var seek: IslandPanel?
    private var screen: NSScreen?
    private var dropped = false
    private var readKeep = false
    private var keepOnBlur = false
    private var hiding = false
    private var searchGen = 0
    private var toastTask: Task<Void, Never>?
    private var resignObserver: NSObjectProtocol?
    private var keyMonitor: Any?

    var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var rows: [Row] {
        var rows = commands.map { Row.command($0) }
        if !trimmedQuery.isEmpty { rows.append(.ask) }
        return rows
    }

    var listHeight: CGFloat {
        12 + CGFloat(min(max(rows.count, 1), 8)) * 44
    }

    var islandHeight: CGFloat {
        if case .search = mode {
            return 58 + listHeight + 38
        }
        return 480
    }

    var stubTitle: String {
        switch mode {
        case .search: ""
        case .args(let command): command.title
        case .output(let title, _): title
        case .chat(let prompt, _):
            if let prompt, !prompt.isEmpty { prompt } else { "AI Chat" }
        }
    }

    var askSelected: Bool {
        guard rows.indices.contains(selected) else { return false }
        if case .ask = rows[selected] { return true }
        return false
    }

    var verb: String? {
        guard rows.indices.contains(selected) else { return nil }
        switch rows[selected] {
        case .ask:
            return "Ask AI"
        case .command(let command):
            if command.kind == .calculator { return "Copy" }
            if !command.arguments.isEmpty { return "Continue" }
            if command.kind == .app { return "Open" }
            return "Run"
        }
    }

    func install() {
        let launcher = IslandPanel()
        let host = ClearHost(rootView: LauncherView(model: self))
        host.wantsLayer = true
        host.layer?.backgroundColor = NSColor.clear.cgColor
        launcher.contentView = host
        self.launcher = launcher

        let seek = IslandPanel()
        let seekHost = ClearHost(rootView: Text("Seek").frame(maxWidth: .infinity, maxHeight: .infinity))
        seekHost.wantsLayer = true
        seekHost.layer?.backgroundColor = NSColor.clear.cgColor
        seek.contentView = seekHost
        self.seek = seek

        resignObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.didResignKeyNotification,
            object: launcher,
            queue: .main
        ) { _ in
            MainActor.assumeIsolated { Model.shared.launcherResignedKey() }
        }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let handled = MainActor.assumeIsolated { Model.shared.handleKey(event) }
            return handled ? nil : event
        }
    }

    func accept(_ raw: String) {
        let obj = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any]
        switch obj?["type"] as? String {
        case "show":
            show()
        case "hide":
            // emit already stored VISIBLE = false. Do not call back into Rust.
            hide(fromRust: true)
        case "result":
            if let result = obj?["result"] as? [String: Any] {
                apply(ExecOutcome(result))
            }
        case "ai":
            stashedAI = raw
        case "seek":
            stashedSeek = raw
        default:
            break
        }
    }

    func show() {
        if !readKeep {
            keepOnBlur = ProcessInfo.processInfo.environment["OPENCAST_KEEP_ON_BLUR"] != nil
            readKeep = true
        }
        if case .args = mode { mode = .search }
        if case .output = mode { mode = .search }
        guard let launcher else { return }
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.main
        guard let screen else { return }
        self.screen = screen
        let final = frame(on: screen, height: islandHeight)
        if !dropped {
            var start = final
            start.origin.y = screen.frame.maxY
            launcher.setFrame(start, display: false)
            dropped = true
            NSApp.activate(ignoringOtherApps: true)
            launcher.makeKeyAndOrderFront(nil)
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.16
                ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
                launcher.animator().setFrame(final, display: true)
            }
        } else {
            launcher.setFrame(final, display: true)
            NSApp.activate(ignoringOtherApps: true)
            launcher.makeKeyAndOrderFront(nil)
        }
        focusTick += 1
    }

    func hide(fromRust: Bool = false) {
        guard !hiding else { return }
        let up = launcher?.isVisible == true || seek?.isVisible == true || NSApp.isActive
        guard up else { return }
        hiding = true
        defer { hiding = false }
        launcher?.orderOut(nil)
        seek?.orderOut(nil)
        seekVisible = false
        NSApp.hide(nil)
        if !fromRust {
            Task.detached { _ = Engine.call(["op": "hidden"]) }
        }
    }

    func launcherResignedKey() {
        if seekVisible { return }
        if keepOnBlur { return }
        hide()
    }

    func noteContentChanged() {
        guard let launcher, launcher.isVisible, let screen else { return }
        let final = frame(on: screen, height: islandHeight)
        guard launcher.frame.integral != final.integral else { return }
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.16
            launcher.animator().setFrame(final, display: true)
        }
    }

    func search() {
        searchGen += 1
        let gen = searchGen
        let q = query
        Task.detached {
            let result = Engine.call(["op": "search", "query": q])
            guard let raw = result["commands"] as? [[String: Any]] else { return }
            let parsed = raw.compactMap(Command.init)
            await MainActor.run {
                Model.shared.deliverSearch(parsed, gen: gen)
            }
        }
    }

    func activate(_ index: Int) {
        guard rows.indices.contains(index) else { return }
        switch rows[index] {
        case .ask:
            openChat(prompt: trimmedQuery, autoSend: true)
        case .command(let command):
            if !command.arguments.isEmpty {
                mode = .args(command)
                noteContentChanged()
            } else {
                perform(command.id)
            }
        }
    }

    func handleKey(_ event: NSEvent) -> Bool {
        guard launcher?.isKeyWindow == true else { return false }
        let mods = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if event.keyCode == 53, mods.isEmpty {
            escape()
            return true
        }
        guard case .search = mode else { return false }
        let chars = event.charactersIgnoringModifiers?.lowercased()
        if (mods == .control && chars == "n") || (mods.isEmpty && event.keyCode == 125) {
            move(1)
            return true
        }
        if (mods == .control && chars == "p") || (mods.isEmpty && event.keyCode == 126) {
            move(-1)
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            activate(selected)
            return true
        }
        if mods.isEmpty, event.keyCode == 48 {
            if !trimmedQuery.isEmpty { openChat(prompt: trimmedQuery, autoSend: true) }
            return true
        }
        return false
    }

    func showWarnings(_ warnings: [String]) {
        guard let first = warnings.first else { return }
        let more = warnings.count > 1 ? " (+\(warnings.count - 1) more)" : ""
        flash(first + more, error: true, thenHide: false)
    }

    private func deliverSearch(_ parsed: [Command], gen: Int) {
        guard gen == searchGen else { return }
        for command in parsed where command.kind == .app {
            let path = String(command.id.dropFirst(4))
            guard appIcons[path] == nil else { continue }
            let image = NSWorkspace.shared.icon(forFile: path)
            image.size = NSSize(width: 24, height: 24)
            appIcons[path] = image
        }
        commands = parsed
        if selected >= rows.count { selected = 0 }
        noteContentChanged()
    }

    private func perform(_ id: String) {
        guard !busy else { return }
        busy = true
        Task.detached {
            let outcome = ExecOutcome(Engine.call(["op": "execute", "id": id, "args": []]))
            await MainActor.run {
                Model.shared.busy = false
                Model.shared.apply(outcome)
            }
        }
    }

    private func apply(_ outcome: ExecOutcome) {
        switch outcome {
        case .hide:
            query = ""
            mode = .search
            hide()
        case .toast(let text):
            query = ""
            mode = .search
            noteContentChanged()
            flash(text, error: false, thenHide: true)
        case .error(let message):
            flash(message, error: true, thenHide: false)
        case .output(let title, let text):
            mode = .output(title: title, text: text)
            noteContentChanged()
        case .chat(let prompt, let autoSend):
            openChat(prompt: prompt, autoSend: autoSend)
        case .args(let command):
            mode = .args(command)
            noteContentChanged()
        case .failure(let message):
            flash(message, error: true, thenHide: false)
        }
    }

    private func openChat(prompt: String?, autoSend: Bool) {
        mode = .chat(prompt: prompt, autoSend: autoSend)
        noteContentChanged()
    }

    private func move(_ delta: Int) {
        let count = rows.count
        guard count > 0 else { return }
        selected = (selected + delta + count) % count
    }

    private func escape() {
        if case .search = mode {
            if query.isEmpty { hide() } else { query = "" }
        } else {
            mode = .search
            noteContentChanged()
            focusTick += 1
        }
    }

    private func flash(_ text: String, error: Bool, thenHide: Bool) {
        toastText = text
        toastIsError = error
        toastTask?.cancel()
        let ms = error ? 4_000 : 1_300
        toastTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(ms))
            guard !Task.isCancelled else { return }
            self.toastText = nil
            if thenHide { self.hide() }
        }
    }

    private func frame(on screen: NSScreen, height: CGFloat) -> NSRect {
        let room = Theme.shadowRoom
        return NSRect(
            x: screen.frame.midX - 380 - room,
            y: screen.frame.maxY - height - room,
            width: 760 + room * 2,
            height: height + room
        )
    }
}

final class IslandPanel: NSPanel {
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 200),
            styleMask: [.borderless],
            backing: .buffered,
            defer: true
        )
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
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

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

final class ClearHost<Content: View>: NSHostingView<Content> {
    override var isOpaque: Bool { false }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        wantsLayer = true
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
    }
}

@main
struct OpenCastApp {
    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.setActivationPolicy(.accessory)
        app.delegate = delegate
        app.run()
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        MainActor.assumeIsolated {
            Model.shared.install()
        }
        Task.detached {
            opencast_start(opencastCallback)
            await MainActor.run { Model.shared.ready = true }
            let warnings = Engine.call(["op": "warnings"])["warnings"] as? [String] ?? []
            await MainActor.run { Model.shared.showWarnings(warnings) }
        }
    }
}
