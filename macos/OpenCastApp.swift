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
        case actions(Command)
        case output(title: String, text: String)
        case chat(prompt: String?, system: String?, provider: String?, model: String?, autoSend: Bool)
        /// Script Filter rows for `command`; `filter` = filter locally as you type.
        case list(command: Command, items: [ListItem], filter: Bool)
    }

    struct ActionRow: Identifiable, Equatable {
        var id: String
        var title: String
        /// Shortcut spec (`cmd+shift+f`), usable from the result list without ⌘K.
        var key: String? = nil

        /// `⌘⇧F` for display.
        var keyLabel: String? {
            guard let key else { return nil }
            let parts = key.split(separator: "+").map(String.init)
            var out = ""
            if parts.contains("ctrl") { out += "⌃" }
            if parts.contains("alt") { out += "⌥" }
            if parts.contains("shift") { out += "⇧" }
            if parts.contains("cmd") { out += "⌘" }
            switch parts.last ?? "" {
            case "return": out += "↩"
            case "backspace": out += "⌫"
            case let k: out += k.uppercased()
            }
            return out
        }

        func matches(_ event: NSEvent) -> Bool {
            guard let key else { return false }
            let parts = Set(key.split(separator: "+").map(String.init))
            let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
            guard parts.contains("cmd") == flags.contains(.command),
                  parts.contains("shift") == flags.contains(.shift),
                  parts.contains("alt") == flags.contains(.option),
                  parts.contains("ctrl") == flags.contains(.control),
                  let last = key.split(separator: "+").last
            else { return false }
            switch last {
            case "return": return event.keyCode == 36 || event.keyCode == 76
            case "backspace": return event.keyCode == 51
            default: return event.charactersIgnoringModifiers?.lowercased() == String(last)
            }
        }
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
    /// First row drawn in the 8-row window. Moves only when the selection leaves it.
    var listStart = 0
    var busy = false
    var mode: Mode = .search
    var ready = false
    var focusTick = 0
    var toastText: String?
    var toastIsError = false
    var seekQuery = ""
    var seekIndex = 0
    var seekScan: SeekScan?
    /// Where the leader line to the current Click Text match starts (overlay coordinates).
    var seekLeaderFrom: CGPoint?
    /// Backtick has switched the island to click, before the scan comes back.
    var clicking = false
    var appIcons: [String: NSImage] = [:]
    var argDrafts: [String] = []
    var argField = 0
    var providers: [Provider] = []
    var providersReady = false
    var chatProviderId: String?
    /// Explicit model. Nil until an init payload or the user picks one; the menu shows `defaultModel`.
    var chatModel: String?
    var chatTurns: [ChatTurn] = []
    /// Saved-chat id (`chat:<uuid>`); new per chat, kept when resuming.
    var chatId = "chat:\(UUID().uuidString)"
    var chatAttachments: [ChatAttachment] = []
    /// A tool call waiting for Allow (Return) / Deny (Esc).
    var pendingApproval: ToolApproval?
    var chatInput = ""
    var chatStreaming = false
    /// `system`, `light`, or `dark`.
    var themePreference = "system"
    /// `island` or `spotlight`.
    var presentation = "island"
    /// Soft Spotlight expand in flight (footer hint).
    var searchingFiles = false
    /// Bumps when macOS light/dark flips so Appearance: System redraws.
    var appearanceEpoch = 0
    /// Bumps when a clipboard thumbnail finishes loading.
    var thumbTick = 0
    /// Dictation is listening.
    var dictating = false
    /// Preview pane on/off (⌘I by default).
    var previewEnabled = true
    /// Preview of the selected row, when it has one.
    var preview: PreviewData?
    private var previewTask: Task<Void, Never>?
    /// Return on a row of a panel-side list (menu items, schedule).
    var localListAction: ((Int) -> Void)?
    /// Script Filter display options (grid, columns, rerun).
    var listOptions = ListOptions()
    private var listRefreshTask: Task<Void, Never>?
    /// The list we came from when ⌘K opened a list row's actions.
    private var listBeforeActions: (command: Command, items: [ListItem], filter: Bool)?
    /// Image icons for list rows (`/path/x.png`, `fileicon:/Applications/X.app`).
    var imageIcons: [String: NSImage] = [:]
    /// `[keys]` from config, by binding name.
    var keymap: [String: KeyBinding] = [:]
    /// `theme_file` values (empty = defaults).
    var themeFile = ThemeFile()
    /// Whether `files` is one of the configured fallbacks.
    var fallbackFiles = true
    var calendarEnabled = true
    var fallbackThreshold = 3
    /// App that was frontmost when the panel opened (AX target for selection / snaps).
    private(set) var previousApp: NSRunningApplication?

    private var silentOutput: AiOutput?
    private var silentBuffer = ""
    private var chatTemperature: Double?
    private var chatMaxTokens: Int?

    private var chatRequestId: String?
    private var chatSystem: String?
    private var providerGen = 0

    private(set) var launcher: IslandPanel?
    /// The next blur (e.g. an open panel) must not hide the launcher.
    var keepOnBlurOnce = false
    private var seek: IslandPanel?
    private var screen: NSScreen?
    private var dropped = false
    private var readKeep = false
    private var keepOnBlur = false
    private var hiding = false
    private var searchGen = 0
    private var fileSearchTask: Task<Void, Never>?
    private var toastTask: Task<Void, Never>?
    private var resignObserver: NSObjectProtocol?
    private var appearanceObservers: [NSObjectProtocol] = []
    private var keyMonitor: Any?
    private var flagsMonitor: Any?

    var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var rows: [Row] {
        if case .list(let command, let items, let filter) = mode {
            return Self.listRows(command: command, items: items, filter: filter, query: trimmedQuery)
        }
        var rows = commands.map { Row.command($0) }
        if !trimmedQuery.isEmpty { rows.append(.ask) }
        return rows
    }

    /// Script Filter items as rows. Ids are `item:<index>` so Return can map back.
    static func listRows(command: Command, items: [ListItem], filter: Bool, query: String) -> [Row] {
        let q = query.lowercased()
        return items.enumerated().compactMap { (index, item) -> Row? in
            if filter, !q.isEmpty {
                let hay = (item.title + " " + (item.subtitle ?? "")).lowercased()
                guard hay.contains(q) else { return nil }
            }
            return .command(Command(
                id: "item:\(index)",
                title: item.title,
                subtitle: item.subtitle,
                icon: item.icon ?? command.icon,
                kind: .script,
                arguments: [],
                hotkey: nil
            ))
        }
    }

    var rowHeight: CGFloat { CGFloat(themeFile.rowHeight ?? 44) }

    /// Showing Script Filter rows.
    var isListMode: Bool {
        if case .list = mode { return true }
        return false
    }

    /// Fixed at 8 rows: resizing the panel on every keystroke (and animating it)
    /// made results appear late. Raycast keeps a fixed height too.
    var listHeight: CGFloat {
        12 + 8 * rowHeight
    }

    var islandHeight: CGFloat {
        if clicking || seekScan != nil { return 58 + 38 }
        switch mode {
        case .search, .list:
            return 58 + listHeight + 38
        case .actions(let command):
            let n = actionRows(for: command).count
            return 52 + CGFloat(min(max(n, 1), 10)) * 36 + 38
        case .args, .output, .chat:
            return 480
        }
    }

    var askSelected: Bool {
        guard rows.indices.contains(selected) else { return false }
        if case .ask = rows[selected] { return true }
        return false
    }

    var activeProvider: Provider? {
        providers.first { $0.id == chatProviderId }
    }

    /// Menu label. The stream body still sends `chatModel`, which stays nil until the user picks.
    var activeChatModel: String? {
        if let chatModel { return chatModel }
        let fallback = activeProvider?.defaultModel ?? ""
        return fallback.isEmpty ? nil : fallback
    }

    var lastAnswer: String? {
        chatTurns.reversed().first { $0.role == "assistant" && !$0.content.isEmpty }?.content
    }

    var verb: String? {
        guard rows.indices.contains(selected) else { return nil }
        switch rows[selected] {
        case .ask:
            return "Ask AI"
        case .command(let command):
            if case .list = mode { return "Run" }
            if command.kind == .calculator { return "Copy" }
            if !command.arguments.isEmpty { return "Continue" }
            if command.kind == .app { return "Open" }
            return "Run"
        }
    }

    /// Key event matches the `[keys]` binding `name` (falls back to `fallback` spec parts).
    func keyMatches(_ event: NSEvent, _ name: String) -> Bool {
        guard let b = keymap[name] ?? Self.defaultKeys[name] else { return false }
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        var mods = Set<String>()
        if flags.contains(.command) { mods.insert("cmd") }
        if flags.contains(.shift) { mods.insert("shift") }
        if flags.contains(.option) { mods.insert("alt") }
        if flags.contains(.control) { mods.insert("ctrl") }
        guard mods == b.mods else { return false }
        switch b.key {
        case "tab": return event.keyCode == 48
        case "return": return event.keyCode == 36 || event.keyCode == 76
        case "escape": return event.keyCode == 53
        case "space": return event.keyCode == 49
        case "up": return event.keyCode == 126
        case "down": return event.keyCode == 125
        case "left": return event.keyCode == 123
        case "right": return event.keyCode == 124
        case "backspace": return event.keyCode == 51
        case "delete": return event.keyCode == 117
        default:
            if b.key.hasPrefix("f"), let n = Int(b.key.dropFirst()), (1 ... 20).contains(n) {
                return event.keyCode == Self.fKeyCode(n)
            }
            return event.charactersIgnoringModifiers?.lowercased() == b.key
        }
    }

    /// Same defaults as `Keys::default()` in core; used until the first settings load.
    static let defaultKeys: [String: KeyBinding] = {
        let specs: [(String, [String], String)] = [
            ("ask_ai", [], "tab"), ("click_text", [], "`"), ("actions", ["cmd"], "k"),
            ("pin", ["cmd", "shift"], "p"), ("paste_answer", ["cmd"], "return"),
            ("copy_answer", ["cmd", "shift"], "c"), ("new_chat", ["cmd"], "n"),
            ("next", ["ctrl"], "n"), ("previous", ["ctrl"], "p"),
            ("dictate", ["cmd"], "d"), ("quick_look", ["cmd"], "y"), ("preview", ["cmd"], "i"),
            ("large_type", ["cmd"], "l"),
        ]
        var out: [String: KeyBinding] = [:]
        for (name, mods, key) in specs {
            if let b = KeyBinding(["name": name, "mods": mods, "key": key]) { out[name] = b }
        }
        return out
    }()

    private static func fKeyCode(_ n: Int) -> UInt16 {
        let codes: [UInt16] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90]
        return codes[n - 1]
    }

    func install() {
        seekSpanCheck()
        let launcher = IslandPanel()
        let host = ClearHost(rootView: LauncherView(model: self))
        host.wantsLayer = true
        host.layer?.backgroundColor = NSColor.clear.cgColor
        launcher.contentView = host
        launcher.sharingType = .none
        self.launcher = launcher

        let seek = IslandPanel()
        seek.refusesKey = true
        seek.sharingType = .none
        let seekHost = ClearHost(rootView: SeekView(model: self))
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
        // LSUIElement: Distributed note is the reliable day/night signal.
        appearanceObservers = [
            DistributedNotificationCenter.default.addObserver(
                forName: Notification.Name("AppleInterfaceThemeChangedNotification"),
                object: nil,
                queue: .main
            ) { _ in
                MainActor.assumeIsolated { Model.shared.systemAppearanceDidChange() }
            },
        ]
        flagsMonitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { event in
            MainActor.assumeIsolated { Model.shared.shiftChanged(event) }
            return event
        }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let handled = MainActor.assumeIsolated { Model.shared.handleKey(event) }
            return handled ? nil : event
        }
        applyPanelAppearance()
    }

    /// Keep AppKit text fields / materials on the same light/dark as Appearance.
    func applyPanelAppearance() {
        let appearance = Theme.nsAppearance(preference: themePreference)
        for panel in [launcher, seek].compactMap({ $0 }) {
            panel.appearance = appearance
            panel.contentView?.appearance = appearance
        }
    }

    func systemAppearanceDidChange() {
        appearanceEpoch += 1
        guard themePreference == "system" else { return }
        applyPanelAppearance()
    }

    func accept(_ raw: String) {
        let obj = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any]
        switch obj?["type"] as? String {
        case "show":
            show()
        case "hide":
            // emit already stored VISIBLE = false. Do not call back into Rust.
            // A hide while Seek is up is Esc / seekClose. The hide emitted just
            // before seek runs first, while seekScan is still nil.
            if seekScan != nil {
                seekScan = nil
                seekQuery = ""
                seekIndex = 0
            }
            hide(fromRust: true)
        case "result":
            if let result = obj?["result"] as? [String: Any] {
                apply(ExecOutcome(result))
            }
        case "ai":
            if CompareController.shared.handle(obj) { return }
            applyAI(obj)
        case "progress":
            // Long multi-step commands (pipelines) report each step while the panel is up.
            if launcher?.isVisible == true, let text = obj?["text"] as? String {
                flash(text, error: false, thenHide: false)
            }
        case "seek":
            if let scan = SeekScan(obj?["scan"]) {
                presentSeek(scan)
            }
        default:
            break
        }
    }

    func show() {
        if clicking || seekScan != nil { return }
        if !readKeep {
            keepOnBlur = ProcessInfo.processInfo.environment["OPENCAST_KEEP_ON_BLUR"] != nil
            readKeep = true
        }
        if case .args = mode { mode = .search }
        if case .output = mode { mode = .search }
        guard let launcher else { return }
        if !NSApp.isActive, let front = WindowManager.frontApp() {
            previousApp = front
        }
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
        refreshSettings()
    }

    func hide(fromRust: Bool = false) {
        // ExecResult hide is applied after the seek event. Leave the overlay up.
        if clicking || seekScan != nil { return }
        guard !hiding else { return }
        let up = launcher?.isVisible == true || seek?.isVisible == true || NSApp.isActive
        guard up else { return }
        hiding = true
        defer { hiding = false }
        WindowPreview.shared.hide()
        launcher?.orderOut(nil)
        seek?.orderOut(nil)
        NSApp.hide(nil)
        if !fromRust {
            Task.detached { _ = Engine.call(["op": "hidden"]) }
        }
    }

    func launcherResignedKey() {
        if clicking || seekScan != nil { return }
        if keepOnBlur { return }
        if keepOnBlurOnce {
            keepOnBlurOnce = false
            return
        }
        hide()
    }

    func noteContentChanged() {
        guard let launcher, launcher.isVisible, let screen else { return }
        let final = frame(on: screen, height: islandHeight)
        guard launcher.frame.integral != final.integral else { return }
        // Snap, don't animate: an animated resize delays when the rows become visible.
        launcher.setFrame(final, display: true)
    }

    func search() {
        searchStarted = CFAbsoluteTimeGetCurrent()
        if case .list(let command, _, let filter) = mode {
            if filter {
                // Rows derive from `query`; nothing to fetch.
                if selected >= rows.count { selected = 0; listStart = 0 }
                alignList()
                noteContentChanged()
            } else {
                rerunList(command, query: trimmedQuery)
            }
            return
        }
        searchGen += 1
        let gen = searchGen
        let q = query
        fileSearchTask?.cancel()
        searchingFiles = false
        if Self.isPathQuery(q) {
            let hidden = showHiddenFiles
            Task.detached {
                let raw = Engine.call(["op": "listPath", "query": q.trimmingCharacters(in: .whitespaces), "hidden": hidden])
                let rows = (raw["commands"] as? [[String: Any]] ?? []).compactMap(Command.init)
                await MainActor.run { Model.shared.deliverSearch(rows, gen: gen) }
            }
            return
        }
        Task.detached {
            let result = Engine.call(["op": "search", "query": q])
            guard let raw = result["commands"] as? [[String: Any]] else { return }
            let parsed = raw.compactMap(Command.init)
            await MainActor.run {
                let merged = ClipboardHistory.shared.merge(into: parsed, query: q)
                let windowIds = merged.filter { $0.kind == .window }.map(\.id)
                if !windowIds.isEmpty {
                    WindowGlyphs.load(windowIds, portrait: WindowManager.isPortraitScreen(for: Model.shared.previousApp))
                }
                Model.shared.deliverSearch(merged, gen: gen)
                Model.shared.scheduleFileSearch(query: q, inMenu: merged, gen: gen)
            }
        }
    }

    private var listRerunTask: Task<Void, Never>?

    /// `@opencast.filter false`: the script owns filtering; debounce and re-run with the query.
    private func rerunList(_ command: Command, query: String) {
        listRerunTask?.cancel()
        listRerunTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 120_000_000)
            guard !Task.isCancelled else { return }
            let outcome = await Task.detached {
                ExecOutcome(Engine.call(["op": "execute", "id": command.id, "args": [query], "action": "list"]))
            }.value
            guard !Task.isCancelled, case .list = mode else { return }
            if case .list(let c, let items, let filter, let options) = outcome {
                mode = .list(command: c, items: items, filter: filter)
                listOptions = options
                loadImageIcons(items)
                if selected >= rows.count { selected = 0; listStart = 0 }
                noteContentChanged()
            }
        }
    }

    /// Soft Spotlight after in-menu paint — never blocks the first result list.
    private func scheduleFileSearch(query: String, inMenu: [Command], gen: Int) {
        fileSearchTask?.cancel()
        let force = Self.forcesFileSearch(query)
        let soft = fallbackFiles && Self.shouldSoftFileSearch(query: query, inMenu: inMenu, threshold: fallbackThreshold)
        guard force || soft else {
            searchingFiles = false
            return
        }
        searchingFiles = true
        noteContentChanged()
        let delayNs: UInt64 = force ? 50_000_000 : 180_000_000
        fileSearchTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: delayNs)
            guard !Task.isCancelled, gen == searchGen else { return }
            let q = query
            let files: [Command] = await Task.detached {
                let raw = Engine.call(["op": "searchFiles", "query": q])
                let list = raw["commands"] as? [[String: Any]] ?? []
                return list.compactMap(Command.init)
            }.value
            guard !Task.isCancelled, gen == searchGen else { return }
            searchingFiles = false
            guard !files.isEmpty else {
                noteContentChanged()
                return
            }
            var seen = Set(commands.map(\.id))
            var next = commands
            for f in files where seen.insert(f.id).inserted {
                next.append(f)
            }
            deliverSearch(next, gen: gen)
        }
    }

    /// `file …` / `find …` / `content:` — user asked for disk explicitly.
    static func forcesFileSearch(_ query: String) -> Bool {
        let ql = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if ql.hasPrefix("content:") || ql.hasPrefix("in:") { return true }
        for p in ["files ", "file ", "find ", "documents ", "document ", "docs "] {
            if ql.hasPrefix(p) { return true }
        }
        return false
    }

    /// Soft expand when the query looks like a path/file, or in-menu hits are thin.
    static func shouldSoftFileSearch(query: String, inMenu: [Command], threshold: Int = 3) -> Bool {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard q.count >= 3 else { return false }
        // Clipboard / screenshots browse — mdfind is useless and slow.
        if ClipboardHistory.isBrowseQuery(q) || ClipboardHistory.isShotsQuery(q) { return false }
        if looksLikePathOrFile(q) { return true }
        let strong = inMenu.filter { cmd in
            switch cmd.kind {
            case .calculator, .clipboard: return false
            default:
                return !cmd.id.hasPrefix("emoji:") && !cmd.id.hasPrefix("note:")
            }
        }.count
        return strong < max(1, threshold - 1)
    }

    static func looksLikePathOrFile(_ q: String) -> Bool {
        if q.hasPrefix("~") || q.hasPrefix("/") { return true }
        if q.contains("/") { return true }
        // `report.pdf`, `notes.txt`
        guard let last = q.split(separator: "/").last else { return false }
        let parts = last.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count >= 2, let ext = parts.last, (1 ... 5).contains(ext.count) else { return false }
        return ext.allSatisfy { $0.isLetter || $0.isNumber }
    }

    func activate(_ index: Int) {
        guard rows.indices.contains(index) else { return }
        if case .list(let command, let items, _) = mode, case .command(let row) = rows[index] {
            guard let i = Int(row.id.dropFirst("item:".count)), items.indices.contains(i) else { return }
            if command.id.hasPrefix("local:") {
                localListAction?(Int(items[i].arg) ?? i)
                return
            }
            let item = items[i]
            // Alfred `valid: false`: Return completes instead of running.
            if !item.valid {
                if let auto = item.autocomplete { query = auto }
                return
            }
            execute(id: command.id, args: [item.arg], action: "run", variables: item.variables)
            return
        }
        switch rows[index] {
        case .ask:
            openPlainChat(prompt: trimmedQuery)
        case .command(let command):
            if !command.arguments.isEmpty {
                showArgs(command)
            } else {
                perform(command.id)
            }
        }
    }

    func handleKey(_ event: NSEvent) -> Bool {
        lastKeyDownAt = event.timestamp
        if let target = recordingHotkeyFor, launcher?.isKeyWindow == true {
            recordHotkey(event, for: target)
            return true
        }
        if clicking || seekScan != nil {
            guard launcher?.isKeyWindow == true || seek?.isKeyWindow == true else { return false }
            return seekKey(event)
        }
        guard launcher?.isKeyWindow == true else { return false }
        let mods = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if event.keyCode == 53, mods.isEmpty {
            escape()
            return true
        }
        switch mode {
        case .search, .list:
            return searchKey(event, mods)
        case .actions:
            return actionsKey(event, mods)
        case .args:
            return argsKey(event, mods)
        case .output:
            return outputKey(event, mods)
        case .chat:
            return chatKey(event, mods)
        }
    }

    private func actionsKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        guard case .actions(let command) = mode else { return false }
        let actions = actionRows(for: command)
        if !mods.isEmpty, let hit = actions.first(where: { $0.matches(event) }) {
            runAction(hit.id, for: command)
            return true
        }
        if (mods == .control && event.charactersIgnoringModifiers?.lowercased() == "n")
            || (mods.isEmpty && event.keyCode == 125)
        {
            if !actions.isEmpty {
                selected = min(selected + 1, actions.count - 1)
                noteContentChanged()
            }
            return true
        }
        if (mods == .control && event.charactersIgnoringModifiers?.lowercased() == "p")
            || (mods.isEmpty && event.keyCode == 126)
        {
            selected = max(selected - 1, 0)
            noteContentChanged()
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            if actions.indices.contains(selected) {
                runAction(actions[selected].id, for: command)
            }
            return true
        }
        return false
    }

    func showWarnings(_ warnings: [String]) {
        guard let first = warnings.first else { return }
        let more = warnings.count > 1 ? " (+\(warnings.count - 1) more)" : ""
        flash(first + more, error: true, thenHide: false)
    }

    private var searchStarted: CFAbsoluteTime = 0
    /// Generation whose rows are on screen. Lags `searchGen` while a search is in flight.
    private var deliveredGen = 0
    /// Return was pressed while the rows were stale: run the top hit when they land.
    private var pendingReturn = false

    private func deliverSearch(_ parsed: [Command], gen: Int) {
        guard gen == searchGen else { return }
        deliveredGen = gen
        let ms = Int((CFAbsoluteTimeGetCurrent() - searchStarted) * 1000)
        if ms > 25 { NSLog("opencast: search \"%@\" took %d ms", query, ms) }
        let missing = parsed.compactMap { command -> String? in
            guard command.kind == .app || command.kind == .file else { return nil }
            let path = command.kind == .app ? String(command.id.dropFirst(4)) : String(command.id.dropFirst(5))
            return appIcons[path] == nil ? path : nil
        }
        if !missing.isEmpty { loadIcons(missing) }
        var parsed = parsed
        if trimmedQuery.isEmpty, calendarEnabled, Schedule.shared.authorized {
            Schedule.shared.refresh()
            if let e = Schedule.shared.next(within: 60), let i = Schedule.shared.upcoming.firstIndex(of: e) {
                let join = Schedule.meetingURL(e) != nil
                parsed.insert(Command(
                    id: "event:\(i)",
                    title: (join ? "Join: " : "Next: ") + (e.title ?? "Event"),
                    subtitle: Schedule.timeRange(e),
                    icon: join ? "🎥" : "📅",
                    kind: .builtin,
                    arguments: [],
                    hotkey: nil
                ), at: 0)
            }
        }
        commands = parsed
        if selected >= rows.count {
            selected = 0
            listStart = 0
        }
        alignList()
        noteContentChanged()
        if pendingReturn {
            pendingReturn = false
            selected = 0
            listStart = 0
            activate(0)
        }
        updatePreview()
    }

    /// Icon loading touches disk; keep it off the main thread.
    func loadIcons(_ paths: [String]) {
        Task.detached(priority: .userInitiated) {
            var loaded: [String: NSImage] = [:]
            for path in paths {
                let image = NSWorkspace.shared.icon(forFile: path)
                // Keep a retina-sized rep; SwiftUI scales to the row. 24pt size made icons soft.
                image.size = NSSize(width: 64, height: 64)
                loaded[path] = image
            }
            let done = loaded
            await MainActor.run {
                for (k, v) in done where Model.shared.appIcons[k] == nil {
                    Model.shared.appIcons[k] = v
                }
            }
        }
    }

    /// Warm the icon cache for every app once, right after start.
    func prefetchAppIcons() {
        Task.detached(priority: .utility) {
            let raw = Engine.call(["op": "apps"])
            let paths = raw["paths"] as? [String] ?? []
            await MainActor.run { Model.shared.loadIcons(paths) }
        }
    }

    private func startClick() {
        guard !clicking, !busy else { return }
        clicking = true
        seekQuery = ""
        seekIndex = 0
        noteContentChanged()
        focusTick += 1
        execute(id: "seek", args: [])
    }

    private func cancelClick() {
        clicking = false
        seekScan = nil
        seekQuery = ""
        seekIndex = 0
        seek?.orderOut(nil)
        noteContentChanged()
        focusTick += 1
    }

    func perform(_ id: String) {
        if confirmDestructive, Self.destructive[id] != nil, pendingConfirm != id {
            let title = Self.destructive[id] ?? "Continue"
            pendingConfirm = id
            let items = [
                ListItem(title: title, subtitle: "Return confirms", icon: "⚠️", arg: "0"),
                ListItem(title: "Cancel", subtitle: nil, icon: "✕", arg: "1"),
            ]
            showLocalList(id: "confirm", title: "\(title)?", icon: "⚠️", items: items, action: { i in
                let m = Model.shared
                if i == 0 {
                    m.backToSearch(clearQuery: true)
                    m.perform(id)
                } else {
                    m.pendingConfirm = nil
                    m.backToSearch(clearQuery: false)
                }
            })
            return
        }
        pendingConfirm = nil
        if id.hasPrefix("event:"), let i = Int(id.dropFirst("event:".count)) {
            Schedule.shared.open(index: i)
            apply(.hide)
            return
        }
        if id == "seek" {
            startClick()
            return
        }
        if id == "builtin:paste-latest-screenshot" {
            busy = true
            let ok = ClipboardHistory.shared.pasteLatestScreenshot() != nil
            Task.detached {
                if ok {
                    _ = Engine.call(["op": "pasteFront"])
                }
                await MainActor.run {
                    Model.shared.busy = false
                    if ok {
                        Model.shared.apply(.hide)
                    } else {
                        Model.shared.flash("No screenshots found", error: true, thenHide: false)
                    }
                }
            }
            return
        }
        if id.hasPrefix("clip:") || id.hasPrefix("shot:") {
            busy = true
            let ok = ClipboardHistory.shared.restoreToPasteboard(id)
            let fallback = ok ? nil : ClipboardHistory.shared.text(for: id)
            Task.detached {
                if ok {
                    _ = Engine.call(["op": "pasteFront"])
                } else if let text = fallback {
                    _ = Engine.call(["op": "paste", "text": text])
                }
                await MainActor.run {
                    Model.shared.busy = false
                    Model.shared.apply(.hide)
                }
            }
            return
        }
        execute(id: id, args: [])
    }

    /// Run a command row (Return, or the "Run" action).
    func activateCommand(_ command: Command) {
        if !command.arguments.isEmpty {
            showArgs(command)
        } else {
            perform(command.id)
        }
    }

    func openPlainChatPublic(prompt: String?) { openPlainChat(prompt: prompt) }

    func execute(
        id: String, args: [String], action: String? = nil, output: String? = nil,
        variables: [String: String] = [:], selection: String? = nil
    ) {
        guard !busy else { return }
        busy = true
        var body: [String: Any] = ["op": "execute", "id": id, "args": args]
        if !variables.isEmpty { body["variables"] = variables }
        if let app = previousApp {
            body["frontApp"] = app.localizedName ?? ""
            // AX read of the selection: instant, no clipboard juggling. Only AI commands use it.
            if selection == nil, id.hasPrefix("ai:"), let text = WindowManager.selectedText(of: app) {
                body["selection"] = text
            }
        }
        if let selection { body["selection"] = selection }
        body["displays"] = WindowManager.displaySignature()
        // Ranking learns which command you pick for what you typed.
        if !isListMode, !trimmedQuery.isEmpty { body["query"] = trimmedQuery }
        if let action { body["action"] = action }
        if let output { body["output"] = output }
        let request = body
        Task.detached {
            let outcome = ExecOutcome(Engine.call(request))
            await MainActor.run {
                Model.shared.busy = false
                Model.shared.apply(outcome)
            }
        }
    }

    /// Hide, let focus return to the previous app, then run `work` on the main actor.
    private func afterHide(_ work: @escaping @MainActor () -> Void) {
        query = ""
        mode = .search
        hide()
        // AX addresses the window by pid, so there is no need to wait for focus
        // to return to that app; run right after the panel is ordered out.
        work()
    }

    func snap(action: String, frac: [Double]?) {
        // From a hotkey (panel down) the front app is the target; from the panel it is the one we covered.
        let fromPanel = launcher?.isVisible == true || NSApp.isActive
        let target = fromPanel ? previousApp : nil
        let hud = !fromPanel && windowHud
        afterHide {
            do {
                let frame = try WindowManager.apply(action: action, frac: frac, app: target)
                if hud, frame != nil { WindowHUD.shared.show(action: action, on: frame) }
            } catch {
                Model.shared.show()
                Model.shared.flash("\(error)", error: true, thenHide: false)
            }
        }
    }

    private func captureLayout(name: String) {
        afterHide {
            do {
                let apps = try WindowManager.captureLayout()
                let displays = WindowManager.displaySignature()
                Model.shared.confirmLayout(name: name, apps: apps, displays: displays, excluded: [], selected: 0)
            } catch {
                Model.shared.show()
                Model.shared.flash("\(error)", error: true, thenHide: false)
            }
        }
    }

    /// Review a captured layout before saving: Return on an app toggles it, Return on Save saves.
    private func confirmLayout(name: String, apps: [WindowManager.AppWindows], displays: String, excluded: Set<Int>, selected sel: Int) {
        let kept = apps.indices.filter { !excluded.contains($0) }
        var items = [ListItem(title: "Save “\(name)”", subtitle: "\(kept.count) of \(apps.count) apps · Return on an app to leave it out", icon: "💾", arg: "0")]
        for (i, a) in apps.enumerated() {
            var item = ListItem(
                title: a.app,
                subtitle: a.windows.count == 1 ? "1 window" : "\(a.windows.count) windows",
                icon: excluded.contains(i) ? "⬜️" : "✅",
                arg: String(i + 1)
            )
            item.markdown = a.windows.map { w in "- \(w.title.isEmpty ? "Untitled" : w.title) — \(Int(w.w))×\(Int(w.h)) at \(Int(w.x)),\(Int(w.y))" }.joined(separator: "\n")
            items.append(item)
        }
        showLocalList(id: "save-layout", title: "Save Layout", icon: "🪟", items: items, action: { i in
            let m = Model.shared
            guard i > 0 else {
                let chosen = kept.map { apps[$0] }
                guard !chosen.isEmpty,
                      let data = try? JSONEncoder().encode(chosen),
                      let json = try? JSONSerialization.jsonObject(with: data)
                else {
                    m.flash("Nothing to save", error: true, thenHide: false)
                    return
                }
                Task.detached {
                    let r = Engine.call(["op": "layoutSave", "name": name, "displays": displays, "apps": json])
                    await MainActor.run {
                        if let text = r["text"] as? String {
                            m.flash(text, error: false, thenHide: true)
                        } else {
                            m.flash(r["error"] as? String ?? "Could not save the layout", error: true, thenHide: false)
                        }
                    }
                }
                return
            }
            var ex = excluded
            if ex.contains(i - 1) { ex.remove(i - 1) } else { ex.insert(i - 1) }
            m.confirmLayout(name: name, apps: apps, displays: displays, excluded: ex, selected: i)
        })
        selected = min(sel, items.count - 1)
        updatePreview()
    }

    private func applyLayout(name: String, apps: [WindowManager.AppWindows]) {
        afterHide {
            do {
                let missing = try WindowManager.applyLayout(apps)
                guard !missing.isEmpty else {
                    Model.shared.show()
                    Model.shared.flash("Restored “\(name)”", error: false, thenHide: true)
                    return
                }
                WindowManager.launch(missing)
                Task { @MainActor in
                    // Place each app as soon as it has a window (up to 12 s), not after a fixed delay.
                    var left = missing
                    let deadline = Date().addingTimeInterval(12)
                    while !left.isEmpty, Date() < deadline {
                        try? await Task.sleep(nanoseconds: 300_000_000)
                        let waiting = WindowManager.notReady(left)
                        let ready = left.filter { e in !waiting.contains { $0.app == e.app && $0.bundleId == e.bundleId } }
                        if !ready.isEmpty {
                            // A fresh window can still be animating in; one short beat, then place it.
                            try? await Task.sleep(nanoseconds: 250_000_000)
                            _ = try? WindowManager.applyLayout(ready)
                        }
                        left = waiting
                    }
                    Model.shared.show()
                    if left.isEmpty {
                        Model.shared.flash("Restored “\(name)”", error: false, thenHide: true)
                    } else {
                        let names = left.map(\.app).joined(separator: ", ")
                        Model.shared.flash("Restored “\(name)” · still starting: \(names)", error: false, thenHide: true)
                    }
                }
            } catch {
                Model.shared.show()
                Model.shared.flash("\(error)", error: true, thenHide: false)
            }
        }
    }

    func apply(_ outcome: ExecOutcome) {
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
            if clicking, seekScan == nil { cancelClick() }
            flash(message, error: true, thenHide: false)
        case .output(let title, let text):
            outputMarkdown = false
            mode = .output(title: title, text: text)
            noteContentChanged()
        case .markdown(let title, let text):
            outputMarkdown = true
            mode = .output(title: title, text: text)
            noteContentChanged()
        case .chat(let prompt, let system, let provider, let model, let autoSend, let temperature, let maxTokens, let output, let preset):
            chatTemperature = temperature
            chatMaxTokens = maxTokens
            chatPreset = preset
            if output != .chat, let prompt, !prompt.isEmpty {
                runSilentAi(prompt: prompt, system: system, provider: provider, model: model, output: output)
            } else {
                openChat(prompt: prompt, system: system, provider: provider, model: model, autoSend: autoSend)
            }
        case .window(let action, let frac):
            snap(action: action, frac: frac)
        case .layout(let op, let name, let apps):
            if op == "capture" { captureLayout(name: name) } else { applyLayout(name: name, apps: apps) }
        case .list(let command, let items, let filter, let options):
            mode = .list(command: command, items: items, filter: filter)
            listOptions = options
            listBeforeActions = nil
            query = ""
            selected = 0
            listStart = 0
            loadImageIcons(items)
            startListRerun(command: command, every: options.rerun)
            noteContentChanged()
            focusTick += 1
            updatePreview()
        case .quit:
            NSApp.terminate(nil)
        case .ui(let action, let args):
            handleUi(action, args: args)
        case .resumeChat(let id, let provider, let model, let system, let turns):
            resumeChat(id: id, provider: provider, model: model, system: system, turns: turns)
        case .args(let command):
            showArgs(command)
        case .failure(let message):
            if clicking, seekScan == nil { cancelClick() }
            flash(message, error: true, thenHide: false)
        case .query(let q):
            mode = .search
            query = q
            selected = 0
            listStart = 0
            noteContentChanged()
            focusTick += 1
            if ready { search() }
        case .theme(let theme):
            themePreference = theme
            applyPanelAppearance()
            noteContentChanged()
            flash("Appearance: \(theme.capitalized)", error: false, thenHide: false)
        case .presentation(let mode):
            presentation = mode
            noteContentChanged()
            flash("Presentation: \(mode.capitalized)", error: false, thenHide: false)
        case .stayAwake(let mode):
            StayAwakeController.shared.apply(mode)
            let label: String = switch mode {
            case "indefinite": "Indefinite"
            case "30m": "30 minutes"
            case "1h": "1 hour"
            case "3h": "3 hours"
            default: "Off"
            }
            flash("Stay Awake: \(label)", error: false, thenHide: false)
        case .runSelected:
            activate(selected)
        case .doEscape:
            escape()
        case .hint(let text):
            flash(text, error: false, thenHide: false)
        case .askAi:
            openPlainChat(prompt: trimmedQuery.isEmpty ? nil : trimmedQuery)
        case .clickMode:
            // Seek event attaches the overlay. Keep the island up.
            if !clicking {
                clicking = true
                seekQuery = ""
                seekIndex = 0
                noteContentChanged()
                focusTick += 1
            }
        }
    }

    func refreshSettings() {
        Task.detached {
            let raw = Engine.call(["op": "settings"])
            await MainActor.run {
                if let theme = raw["theme"] as? String {
                    Model.shared.themePreference = theme
                    Model.shared.applyPanelAppearance()
                }
                if let presentation = raw["presentation"] as? String {
                    Model.shared.presentation = presentation
                }
                if let keys = raw["keymap"] as? [[String: Any]] {
                    var map: [String: KeyBinding] = [:]
                    for k in keys.compactMap(KeyBinding.init) { map[k.name] = k }
                    Model.shared.keymap = map
                }
                Model.shared.themeFile = (raw["themeFile"] as? [String: Any]).map(ThemeFile.init) ?? ThemeFile()
                Theme.file = Model.shared.themeFile
                if let f = raw["fallbackFiles"] as? Bool { Model.shared.fallbackFiles = f }
                if let c = raw["calendar"] as? Bool { Model.shared.calendarEnabled = c }
                if let t = raw["fallbackThreshold"] as? Int { Model.shared.fallbackThreshold = t }
                if let v = raw["shiftQuickLook"] as? Bool { Model.shared.shiftQuickLook = v }
                if let v = raw["confirmDestructive"] as? Bool { Model.shared.confirmDestructive = v }
                if let v = raw["windowHud"] as? Bool { Model.shared.windowHud = v }
                let drag = raw["dragSnap"] as? Bool ?? false
                if drag && !WindowManager.accessibilityTrusted {
                    Model.shared.flash("Drag to snap needs Accessibility access", error: true, thenHide: false)
                }
                DragSnap.shared.setEnabled(drag)
                WindowGlyphs.reset()
                if let f = raw["focus"] as? [String: Any] {
                    Model.shared.focusApps = f["apps"] as? [String] ?? []
                    Model.shared.focusMinutes = f["minutes"] as? Int ?? 25
                    Model.shared.focusDomains = f["domains"] as? [String] ?? []
                    Model.shared.focusMenuBar = f["menuBar"] as? Bool ?? true
                }
                Model.shared.noteContentChanged()
            }
        }
    }

    /// Tab / Ask AI: a fresh chat with the default provider settings.
    /// Reopen a saved chat with its provider and model.
    private func resumeChat(id: String, provider: String?, model: String?, system: String?, turns: [ChatTurn]) {
        chatPreset = nil
        openChat(prompt: nil, system: system, provider: provider, model: model, autoSend: false)
        chatId = id
        chatTurns = turns
    }

    private func openPlainChat(prompt: String?) {
        chatTemperature = nil
        chatMaxTokens = nil
        chatPreset = nil
        openChat(prompt: prompt, system: nil, provider: nil, model: nil, autoSend: prompt != nil)
    }

    func openChat(prompt: String?, system: String?, provider: String?, model: String?, autoSend: Bool) {
        stopStream()
        providerGen += 1
        chatTurns = []
        chatId = "chat:\(UUID().uuidString)"
        chatAttachments = []
        pendingApproval = nil
        chatInput = ""
        chatSystem = system
        chatProviderId = provider
        chatModel = model
        providers = []
        providersReady = false
        mode = .chat(prompt: prompt, system: system, provider: provider, model: model, autoSend: autoSend)
        noteContentChanged()
        focusTick += 1
        if let prompt, !prompt.isEmpty {
            if autoSend { sendChat(prompt) } else { chatInput = prompt }
        }
        loadProviders()
    }

    private func showArgs(_ command: Command) {
        argDrafts = Array(repeating: "", count: command.arguments.count)
        argField = 0
        mode = .args(command)
        noteContentChanged()
        focusTick += 1
    }

    private func move(_ delta: Int) {
        let count = rows.count
        guard count > 0 else { return }
        if isListMode, listOptions.isGrid {
            selected = min(max(selected + delta, 0), count - 1)
            updatePreview()
            return
        }
        let next = (selected + delta + count) % count
        let wrapped = (delta > 0 && next < selected) || (delta < 0 && next > selected)
        selected = next
        if wrapped { listStart = delta > 0 ? 0 : max(0, count - 8) }
        alignList()
        updatePreview()
    }

    /// Keep `selected` inside the visible window, and keep that window full.
    private func alignList() {
        let count = rows.count
        let window = 8
        guard count > 0 else {
            listStart = 0
            return
        }
        if selected < listStart { listStart = selected }
        if selected >= listStart + window { listStart = selected - window + 1 }
        let maxStart = max(0, count - window)
        if listStart > maxStart { listStart = maxStart }
        if listStart < 0 { listStart = 0 }
    }

    private func escape() {
        if dictating {
            Dictation.shared.stop()
            return
        }
        switch mode {
        case .search:
            if query.isEmpty { hide() } else { query = "" }
        case .list:
            backToSearch(clearQuery: true)
        case .chat where pendingApproval != nil:
            answerApproval(false)
        case .chat where chatStreaming:
            // Cancel does not emit `done`. Drop the flag first so a late token is ignored.
            stopStream()
        case .chat:
            backToSearch(clearQuery: true)
        default:
            backToSearch(clearQuery: false)
        }
    }

    func backToSearch(clearQuery: Bool) {
        if case .actions = mode, let saved = listBeforeActions {
            // ⌘K from a list row: Esc returns to the list, not to search.
            listBeforeActions = nil
            mode = .list(command: saved.command, items: saved.items, filter: saved.filter)
            noteContentChanged()
            focusTick += 1
            return
        }
        listBeforeActions = nil
        listRefreshTask?.cancel()
        if case .chat = mode { stopStream() }
        localListAction = nil
        openWithFor = nil
        if clearQuery { query = "" }
        mode = .search
        noteContentChanged()
        focusTick += 1
    }

    func pickProvider(_ id: String) {
        chatProviderId = id
        chatModel = nil
    }

    func pickChatModel(_ id: String) {
        chatModel = id
    }

    func copyToClipboard(_ text: String, toast: String? = nil) {
        Task.detached {
            _ = Engine.call(["op": "copy", "text": text])
            guard let toast else { return }
            await MainActor.run { Model.shared.flash(toast, error: false, thenHide: false) }
        }
    }

    private func searchKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        if !isListMode, pathModeKey(event, mods) { return true }
        if !isListMode, bufferKey(event, mods) { return true }
        let grid = isListMode && listOptions.isGrid
        if grid, mods.isEmpty, event.keyCode == 123 || event.keyCode == 124 {
            move(event.keyCode == 124 ? 1 : -1)
            return true
        }
        if keyMatches(event, "next") || (mods.isEmpty && event.keyCode == 125) {
            move(grid ? listOptions.columns : 1)
            return true
        }
        if keyMatches(event, "previous") || (mods.isEmpty && event.keyCode == 126) {
            move(grid ? -listOptions.columns : -1)
            return true
        }
        if isListMode, case let (_, item)? = selectedListItem() {
            // Row actions with their own keys (incl. Alfred ⌘/⌥/⌃ + Return) before plain Return.
            if !mods.isEmpty, let hit = listItemRows(item).first(where: { $0.matches(event) }) {
                runListAction(hit.id)
                return true
            }
            if keyMatches(event, "actions") {
                openListActions()
                return true
            }
            if keyMatches(event, "quick_look"), let target = item.quicklook {
                quickLook(target)
                return true
            }
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            // Typed fast and hit Return before this query's rows arrived: wait for
            // them (a few ms) instead of running the previous query's top hit.
            if !isListMode, deliveredGen < searchGen, selected == 0 {
                pendingReturn = true
            } else {
                activate(selected)
            }
            return true
        }
        if case .list = mode {
            // Tab: autocomplete the selected item when it offers one.
            if event.keyCode == 48, mods.isEmpty, case .list(_, let items, _) = mode,
               rows.indices.contains(selected), case .command(let row) = rows[selected],
               let i = Int(row.id.dropFirst("item:".count)), let auto = items[safe: i]?.autocomplete
            {
                query = auto
                return true
            }
            return false
        }
        if keyMatches(event, "ask_ai") {
            if !trimmedQuery.isEmpty {
                openPlainChat(prompt: trimmedQuery)
            }
            return true
        }
        if keyMatches(event, "click_text") {
            startClick()
            return true
        }
        if keyMatches(event, "pin") {
            togglePinSelected()
            return true
        }
        if keyMatches(event, "actions") {
            openActions()
            return true
        }
        if keyMatches(event, "dictate") {
            toggleDictation()
            return true
        }
        if keyMatches(event, "large_type") {
            // The selected calculator/conversion answer, else what was typed.
            var text = trimmedQuery
            if rows.indices.contains(selected), case .command(let c) = rows[selected], c.kind == .calculator { text = c.title }
            if !text.isEmpty { handleUi("large-type", args: [text]) }
            return true
        }
        if keyMatches(event, "preview") {
            previewEnabled.toggle()
            updatePreview()
            return true
        }
        if rows.indices.contains(selected), case .command(let command) = rows[selected] {
            if keyMatches(event, "quick_look") {
                if let url = fileURL(for: command) { QuickLook.shared.show([url]) }
                return true
            }
            // Action shortcuts work straight from the list (no ⌘K needed).
            if !mods.isEmpty, let hit = actionRows(for: command).first(where: { $0.matches(event) }) {
                runAction(hit.id, for: command)
                return true
            }
        }
        return false
    }

    // MARK: - Script Filter rows (actions, detail, rerun, icons)

    /// The list shown now, or the one ⌘K came from.
    private var activeList: (command: Command, items: [ListItem], filter: Bool)? {
        if case .list(let c, let items, let f) = mode { return (c, items, f) }
        return listBeforeActions
    }

    /// The item behind a list row command (`item:<index>`).
    func listItem(for row: Command) -> (Command, ListItem)? {
        guard let list = activeList, let i = Int(row.id.dropFirst("item:".count)), list.items.indices.contains(i) else { return nil }
        return (list.command, list.items[i])
    }

    func selectedListItem() -> (Command, ListItem)? {
        guard rows.indices.contains(selected), case .command(let row) = rows[selected] else { return nil }
        return listItem(for: row)
    }

    /// The row whose actions are on screen (actions mode) or the selected one.
    private var listActionTarget: (Command, ListItem)? {
        if case .actions(let row) = mode { return listItem(for: row) }
        return selectedListItem()
    }

    func listItemRows(_ item: ListItem) -> [ActionRow] {
        var rows = [ActionRow(id: "item-run", title: item.valid ? "Run" : "Complete")]
        for (n, a) in item.actions.enumerated() where !(a.key ?? "").contains("fn") {
            rows.append(ActionRow(id: "item-act:\(n)", title: a.title, key: a.key))
        }
        rows.append(ActionRow(id: "item-copy", title: "Copy", key: "cmd+shift+c"))
        if item.quicklook != nil {
            rows.append(ActionRow(id: "item-ql", title: "Quick Look", key: "cmd+y"))
        }
        return rows
    }

    private func openListActions() {
        guard let list = activeList, rows.indices.contains(selected), case .command(let row) = rows[selected] else { return }
        listBeforeActions = list
        mode = .actions(row)
        selected = 0
        listStart = 0
        noteContentChanged()
        focusTick += 1
    }

    private func runListAction(_ id: String) {
        guard let list = activeList, case let (command, item)? = listActionTarget else { return }
        // Leave actions mode for the list again before running.
        if case .actions = mode {
            listBeforeActions = nil
            mode = .list(command: list.command, items: list.items, filter: list.filter)
            noteContentChanged()
        }
        if command.id.hasPrefix("local:") {
            let index = Int(item.arg) ?? 0
            switch id {
            case "item-run": localListAction?(index)
            case "item-copy": copyToClipboard(item.copy ?? item.title, toast: "Copied")
            case "item-ql": if let target = item.quicklook { quickLook(target) }
            default:
                if let n = Int(id.dropFirst("item-act:".count)), item.actions.indices.contains(n) {
                    localItemAction?(index, item.actions[n].arg)
                }
            }
            return
        }
        switch id {
        case "item-run":
            if !item.valid {
                if let auto = item.autocomplete { query = auto }
                return
            }
            execute(id: command.id, args: [item.arg], action: "run", variables: item.variables)
        case "item-copy":
            copyToClipboard(item.copy ?? item.arg, toast: "Copied")
        case "item-ql":
            if let target = item.quicklook { quickLook(target) }
        default:
            guard id.hasPrefix("item-act:"), let n = Int(id.dropFirst("item-act:".count)), item.actions.indices.contains(n) else { return }
            let action = item.actions[n]
            var vars = item.variables
            // Alfred modifier connections read this (see alfred.rs).
            if let key = action.key, key.hasSuffix("+return") {
                vars["OPENCAST_MOD"] = String(key.dropLast("+return".count))
            }
            execute(id: command.id, args: [action.arg], action: "run", variables: vars)
        }
    }

    private func quickLook(_ target: String) {
        let url: URL? = target.hasPrefix("http://") || target.hasPrefix("https://")
            ? URL(string: target)
            : URL(fileURLWithPath: (target as NSString).expandingTildeInPath)
        if let url { QuickLook.shared.show([url]) }
    }

    /// Alfred `rerun`: refresh the list every few seconds while it is open.
    private func startListRerun(command: Command, every seconds: Double?) {
        listRefreshTask?.cancel()
        guard let seconds, seconds > 0 else { return }
        listRefreshTask = Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
                guard !Task.isCancelled, case .list(let current, _, let filter) = mode, current.id == command.id else { return }
                let q = filter ? "" : trimmedQuery
                let outcome = await Task.detached {
                    ExecOutcome(Engine.call(["op": "execute", "id": command.id, "args": q.isEmpty ? [] : [q], "action": "list"]))
                }.value
                guard !Task.isCancelled, case .list(let still, _, _) = mode, still.id == command.id,
                      case .list(_, let items, let f, let options) = outcome else { return }
                mode = .list(command: still, items: items, filter: f)
                listOptions = options
                if selected >= rows.count { selected = max(0, rows.count - 1) }
                loadImageIcons(items)
                noteContentChanged()
                updatePreview()
            }
        }
    }

    /// Image paths and `fileicon:` icons for list rows, loaded off the main thread.
    private func loadImageIcons(_ items: [ListItem]) {
        let wanted = items.compactMap(\.icon).filter { Self.isImageIcon($0) && imageIcons[$0] == nil }
        guard !wanted.isEmpty else { return }
        Task.detached(priority: .userInitiated) {
            var loaded: [String: NSImage] = [:]
            for key in Set(wanted) {
                if key.hasPrefix("fileicon:") {
                    let img = NSWorkspace.shared.icon(forFile: (String(key.dropFirst("fileicon:".count)) as NSString).expandingTildeInPath)
                    img.size = NSSize(width: 64, height: 64)
                    loaded[key] = img
                } else if let img = NSImage(contentsOfFile: (key as NSString).expandingTildeInPath) {
                    loaded[key] = img
                }
            }
            let done = loaded
            await MainActor.run {
                for (k, v) in done { Model.shared.imageIcons[k] = v }
            }
        }
    }

    static func isImageIcon(_ icon: String) -> Bool {
        if icon.hasPrefix("fileicon:") { return true }
        guard icon.hasPrefix("/") || icon.hasPrefix("~") else { return false }
        let ext = (icon as NSString).pathExtension.lowercased()
        return ["png", "jpg", "jpeg", "gif", "icns", "pdf", "tiff", "heic", "webp", "svg"].contains(ext)
    }

    // MARK: - Preview pane

    /// Load the preview for the selected row (debounced; off the main thread).
    func updatePreview() {
        previewTask?.cancel()
        updateWindowPreview()
        if isListMode {
            // List rows preview their own markdown (or an image to Quick Look); nothing to fetch.
            guard previewEnabled, case let (command, item)? = selectedListItem() else {
                if preview != nil { preview = nil }
                return
            }
            let id = "\(command.id)#\(item.title)#\(item.arg)"
            if preview?.id == id { return }
            var image: NSImage?
            if let ql = item.quicklook, Self.isImageIcon(ql) { image = NSImage(contentsOfFile: (ql as NSString).expandingTildeInPath) }
            if item.markdown == nil, image == nil {
                if preview != nil { preview = nil }
                return
            }
            preview = PreviewData(id: id, title: item.title, text: item.markdown, image: image, path: nil)
            return
        }
        guard previewEnabled, rows.indices.contains(selected), case .command(let command) = rows[selected],
              PreviewLoader.wants(command)
        else {
            if preview != nil { preview = nil }
            return
        }
        if preview?.id == command.id { return }
        if command.kind == .clipboard {
            preview = ClipboardHistory.shared.preview(for: command.id, title: command.title)
            return
        }
        previewTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 40_000_000)
            guard !Task.isCancelled else { return }
            let data = await Task.detached { await PreviewLoader.load(command) }.value
            guard !Task.isCancelled else { return }
            Model.shared.preview = data
        }
    }

    /// Highlighting a window command outlines where the window will land.
    private func updateWindowPreview() {
        if isListMode, case let (command, item)? = selectedListItem(), command.id == "local:arrange-window",
           launcher?.isVisible == true, let i = Int(item.arg), arrangeShown.indices.contains(i),
           let r = WindowManager.previewTarget(action: arrangeShown[i], app: previousApp) {
            WindowPreview.shared.show(r)
            return
        }
        guard !isListMode, launcher?.isVisible == true, rows.indices.contains(selected),
              case .command(let command) = rows[selected], command.kind == .window,
              let r = WindowManager.previewTarget(action: String(command.id.dropFirst("window:".count)), app: previousApp)
        else {
            WindowPreview.shared.hide()
            return
        }
        WindowPreview.shared.show(r)
    }

    /// Window commands as a grid of region glyphs; the highlighted one is outlined on screen.
    static let arrangeActions = [
        "left-half", "right-half", "top-half", "bottom-half", "center-half", "maximize",
        "top-left", "top-right", "bottom-left", "bottom-right", "almost-maximize", "center",
        "first-third", "center-third", "last-third", "first-two-thirds", "last-two-thirds", "reasonable-size",
        "top-left-sixth", "top-center-sixth", "top-right-sixth",
        "bottom-left-sixth", "bottom-center-sixth", "bottom-right-sixth",
        "maximize-height", "maximize-width", "next-display", "previous-display", "restore", "undo",
    ]

    /// The same picker for a rotated (portrait) monitor: rows first.
    static let portraitArrangeActions = [
        "top-half", "middle-half", "bottom-half", "maximize", "almost-maximize", "center",
        "top-third", "middle-third", "bottom-third", "top-two-thirds", "bottom-two-thirds", "reasonable-size",
        "top-left", "top-right", "bottom-left", "bottom-right", "left-half", "right-half",
        "top-left-sixth", "top-right-sixth", "middle-left-sixth",
        "middle-right-sixth", "bottom-left-sixth", "bottom-right-sixth",
        "maximize-height", "maximize-width", "next-display", "previous-display", "restore", "undo",
    ]

    /// Actions in the open Arrange Window grid, by row.
    @ObservationIgnored var arrangeShown: [String] = []

    private func showArrangeWindow() {
        guard WindowManager.accessibilityTrusted else {
            flash("Window commands need Accessibility access", error: true, thenHide: false)
            return
        }
        // On a rotated monitor, the stacking commands come first.
        let portrait = WindowManager.isPortraitScreen(for: previousApp)
        let actions = portrait ? Self.portraitArrangeActions : Self.arrangeActions
        WindowGlyphs.load(actions.map { "window:" + $0 }, portrait: portrait)
        arrangeShown = actions
        let items = actions.enumerated().map { i, a in
            ListItem(title: WindowHUD.title(for: a), subtitle: nil, icon: WindowGlyphs.imagePath(for: "window:" + a) ?? "🪟", arg: String(i))
        }
        var options = ListOptions()
        options.view = "grid"
        options.columns = 6
        showLocalList(id: "arrange-window", title: "Arrange Window", icon: "🪟", items: items, action: { i in
            guard actions.indices.contains(i) else { return }
            Model.shared.snap(action: actions[i], frac: nil)
        }, options: options)
        loadImageIcons(items)
        updatePreview()
    }

    // MARK: - Panel-side features

    func handleUi(_ action: String, args: [String] = []) {
        let first = args.first ?? ""
        switch action {
        case "settings":
            hide()
            SettingsWindowController.shared.show()
        case "menu-items":
            showMenuItems()
        case "dictate":
            show()
            toggleDictation()
        case "schedule":
            showSchedule()
        case "contacts":
            ContactsSearch.show(in: self)
        case "reminders":
            RemindersBridge.show(in: self)
        case "new-reminder":
            RemindersBridge.add(title: first, when: args.count > 1 ? args[1] : "", model: self)
        case "define":
            if let text = DictionaryLookup.define(first) {
                mode = .output(title: first, text: text)
                noteContentChanged()
            } else {
                flash("No dictionary entry for “\(first)”", error: true, thenHide: false)
            }
        case "large-type":
            hide()
            LargeType.shared.show(first)
        case "media-play", "media-next", "media-previous":
            hide()
            MediaKeys.press(action)
        case "selection":
            showSelectionActions()
        case "focus-start":
            let minutes = Int(first.trimmingCharacters(in: .whitespaces)) ?? focusMinutes
            let goal = args.count > 1 ? args[1] : ""
            FocusController.shared.start(minutes: minutes, apps: focusApps, domains: focusDomains, goal: goal, menuBar: focusMenuBar, model: self)
        case "focus-pause":
            FocusController.shared.togglePause(model: self)
        case "focus-stop":
            if FocusController.shared.isActive {
                FocusController.shared.stop(announce: false)
                flash("Focus stopped", error: false, thenHide: true)
            } else {
                flash("No focus session running", error: false, thenHide: true)
            }
        case "browse-files":
            show()
            mode = .search
            query = UserDefaults.standard.string(forKey: "OpenCastLastPath") ?? "~/"
            focusTick += 1
        case "hide-others":
            let keep = previousApp
            for app in NSWorkspace.shared.runningApplications
            where app.activationPolicy == .regular && app != keep && app != NSRunningApplication.current {
                app.hide()
            }
            keep?.activate()
        case "arrange-window":
            showArrangeWindow()
        case "sticky-note":
            hide()
            let r = Engine.call(["op": "noteUpdate", "id": "", "title": "", "body": " "])
            if let id = r["id"] as? String {
                StickyNotes.shared.open(id: id, title: "", body: "")
            }
        case "dictate-anywhere":
            hide()
            GlobalDictation.shared.toggle()
        case "cheat-sheet":
            // From a hotkey the panel is down and the front app is the one to read.
            let target = (launcher?.isVisible == true || NSApp.isActive) ? previousApp : NSWorkspace.shared.frontmostApplication
            hide()
            guard WindowManager.accessibilityTrusted else {
                show()
                flash(WindowManager.permissionHint, error: true, thenHide: false)
                return
            }
            if let target { CheatSheet.shared.show(for: target) }
        case "page":
            askAboutPage(mode: first, question: args.count > 1 ? args[1] : "")
        case "compare":
            hide()
            CompareController.shared.show(prompt: first)
        case "spell":
            showSpelling(first)
        case "note-editor":
            hide()
            NotesEditorController.shared.show(noteId: first.isEmpty ? nil : first)
        default:
            flash("Unknown action \(action)", error: true, thenHide: false)
        }
    }

    /// `[focus]` from config (refreshed with the other settings).
    var focusApps: [String] = []
    var focusMinutes = 25
    var focusDomains: [String] = []
    var focusMenuBar = true

    // MARK: - File Buffer and path mode

    /// Files collected with ⌥↑; ⌘K and Act on Selection act on all of them.
    var fileBuffer: [URL] = []
    var showHiddenFiles = false
    /// Actions behind the ⌘K rows for the file currently shown.
    @ObservationIgnored var uaActions: [String: UAction] = [:]

    static func isPathQuery(_ q: String) -> Bool {
        let t = q.trimmingCharacters(in: .whitespaces)
        return t == "~" || t.hasPrefix("~/") || (t.hasPrefix("/") && !t.hasPrefix("//"))
    }

    func toggleBuffer(_ command: Command) {
        guard let url = fileURL(for: command) else { return }
        if let i = fileBuffer.firstIndex(of: url) {
            fileBuffer.remove(at: i)
        } else {
            fileBuffer.append(url)
        }
        noteContentChanged()
    }

    /// ⌥↑ add, ⌥↓ remove, ⌥← clear (only while the buffer has files).
    private func bufferKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        guard mods == .option else { return false }
        let row: Command? = rows.indices.contains(selected) ? { if case .command(let c) = rows[selected] { return c } else { return nil } }() : nil
        switch event.keyCode {
        case 126:
            guard let row, let url = fileURL(for: row), row.kind == .file || row.kind == .app || row.kind == .clipboard else { return false }
            if !fileBuffer.contains(url) { fileBuffer.append(url) }
            flash("File Buffer · \(fileBuffer.count) · ⌘K acts on all · ⌥← clears", error: false, thenHide: false)
            noteContentChanged()
            return true
        case 125:
            guard let row, let url = fileURL(for: row), fileBuffer.contains(url) else { return false }
            fileBuffer.removeAll { $0 == url }
            noteContentChanged()
            return true
        case 123:
            guard !fileBuffer.isEmpty else { return false }
            fileBuffer = []
            flash("File Buffer cleared", error: false, thenHide: false)
            noteContentChanged()
            return true
        default:
            return false
        }
    }

    /// Path mode: Tab enters the folder, ⌘↑ goes up, ⌘⇧. shows hidden files.
    private func pathModeKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        guard Self.isPathQuery(query) else { return false }
        let chars = event.charactersIgnoringModifiers?.lowercased()
        if mods == [.command, .shift], chars == "." || chars == ">" {
            showHiddenFiles.toggle()
            flash(showHiddenFiles ? "Showing hidden files" : "Hiding hidden files", error: false, thenHide: false)
            search()
            return true
        }
        if mods == .command, event.keyCode == 126 {
            var t = query.trimmingCharacters(in: .whitespaces)
            if t.hasSuffix("/") { t.removeLast() }
            guard let cut = t.lastIndex(of: "/") else { return true }
            query = String(t[...cut])
            return true
        }
        if mods.isEmpty, event.keyCode == 48 || event.keyCode == 124 && caretAtEnd {
            guard rows.indices.contains(selected), case .command(let c) = rows[selected], c.title.hasSuffix("/"),
                  let url = fileURL(for: c) else { return event.keyCode == 48 }
            let home = NSHomeDirectory()
            var p = url.path
            if p.hasPrefix(home) { p = "~" + p.dropFirst(home.count) }
            query = p + "/"
            UserDefaults.standard.set(query, forKey: "OpenCastLastPath")
            return true
        }
        return false
    }

    /// The search field's caret is after the last character (→ then means "enter folder").
    private var caretAtEnd: Bool {
        guard let editor = NSApp.keyWindow?.firstResponder as? NSTextView else { return true }
        let r = editor.selectedRange()
        return r.length == 0 && r.location >= (editor.string as NSString).length
    }

    // MARK: - Hotkeys, spelling, recent documents, profile

    var outputMarkdown = false
    var chatPreset: String?
    var shiftQuickLook = false
    var confirmDestructive = true
    var windowHud = true
    var pendingConfirm: String?
    var recordingHotkeyFor: Command?
    @ObservationIgnored var lastKeyDownAt: TimeInterval = 0
    @ObservationIgnored var shiftDownAt: TimeInterval?

    static let destructive: [String: String] = [
        "builtin:restart": "Restart",
        "builtin:shut-down": "Shut Down",
        "builtin:log-out": "Log Out",
        "builtin:quit-all": "Quit All Apps",
    ]

    /// ANSI key codes → the names the hotkey parser understands.
    private static let keyNames: [UInt16: String] = [
        0: "A", 1: "S", 2: "D", 3: "F", 4: "H", 5: "G", 6: "Z", 7: "X", 8: "C", 9: "V", 11: "B", 12: "Q",
        13: "W", 14: "E", 15: "R", 16: "Y", 17: "T", 18: "1", 19: "2", 20: "3", 21: "4", 22: "6", 23: "5",
        24: "Equal", 25: "9", 26: "7", 27: "Minus", 28: "8", 29: "0", 30: "BracketRight", 31: "O", 32: "U",
        33: "BracketLeft", 34: "I", 35: "P", 37: "L", 38: "J", 39: "Quote", 40: "K", 41: "Semicolon",
        42: "Backslash", 43: "Comma", 44: "Slash", 45: "N", 46: "M", 47: "Period", 50: "Backquote",
        36: "Enter", 48: "Tab", 49: "Space", 51: "Backspace", 117: "Delete",
        123: "Left", 124: "Right", 125: "Down", 126: "Up",
        122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7", 100: "F8", 101: "F9",
        109: "F10", 103: "F11", 111: "F12", 105: "F13", 107: "F14", 113: "F15", 106: "F16", 64: "F17",
        79: "F18", 80: "F19", 90: "F20",
    ]

    private func recordHotkey(_ event: NSEvent, for command: Command) {
        if event.keyCode == 53 {
            recordingHotkeyFor = nil
            flash("Cancelled", error: false, thenHide: false)
            return
        }
        guard let key = Self.keyNames[event.keyCode] else {
            flash("That key can't be a hotkey — try another", error: true, thenHide: false)
            return
        }
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        let isF = key.hasPrefix("F") && key.count <= 3 && Int(key.dropFirst()) != nil
        guard isF || flags.contains(.command) || flags.contains(.control) || flags.contains(.option) else {
            flash("Use ⌘, ⌃ or ⌥ (or an F-key)", error: true, thenHide: false)
            return
        }
        var parts: [String] = []
        if flags == [.command, .shift, .option, .control] {
            parts = ["Hyper"]
        } else {
            if flags.contains(.control) { parts.append("Ctrl") }
            if flags.contains(.option) { parts.append("Alt") }
            if flags.contains(.shift) { parts.append("Shift") }
            if flags.contains(.command) { parts.append("Cmd") }
        }
        recordingHotkeyFor = nil
        setHotkey((parts + [key]).joined(separator: "+"), for: command)
    }

    private func setHotkey(_ spec: String, for command: Command) {
        Task.detached {
            let r = Engine.call(["op": "setHotkey", "id": command.id, "key": spec])
            await MainActor.run {
                let m = Model.shared
                if let e = r["error"] as? String {
                    m.flash(e, error: true, thenHide: false)
                } else {
                    m.flash(spec.isEmpty ? "Hotkey removed" : "\(spec) → \(command.title)", error: false, thenHide: false)
                    if m.ready { m.search() }
                }
            }
        }
    }

    private func showSpelling(_ word: String) {
        let w = word.trimmingCharacters(in: .whitespaces)
        guard !w.isEmpty else { return }
        let checker = NSSpellChecker.shared
        let range = NSRange(location: 0, length: (w as NSString).length)
        let wrong = checker.checkSpelling(of: w, startingAt: 0).location != NSNotFound
        var words = checker.guesses(forWordRange: range, in: w, language: nil, inSpellDocumentWithTag: 0) ?? []
        words += checker.completions(forPartialWordRange: range, in: w, language: nil, inSpellDocumentWithTag: 0) ?? []
        var seen = Set<String>()
        words = words.filter { seen.insert($0.lowercased()).inserted && $0.lowercased() != w.lowercased() }
        var items: [ListItem] = []
        if !wrong { items.append(ListItem(title: w, subtitle: "Spelled correctly · Return copies", icon: "✅", arg: w)) }
        items += words.prefix(20).map { ListItem(title: $0, subtitle: "Return copies", icon: "🔤", arg: $0) }
        guard !items.isEmpty else {
            flash("No suggestions for “\(w)”", error: true, thenHide: false)
            return
        }
        let values = items.map(\.arg)
        for i in items.indices {
            items[i].arg = String(i)
            items[i].actions = [ItemAction(title: "Paste", arg: "paste", key: "cmd+return"), ItemAction(title: "Define", arg: "define", key: "cmd+d")]
        }
        showLocalList(id: "spell", title: wrong ? "“\(w)” — did you mean" : "Spelling", icon: "🔤", items: items, action: { i in
            guard values.indices.contains(i) else { return }
            Model.shared.copyToClipboard(values[i], toast: "Copied \(values[i])")
        }, itemAction: { i, arg in
            guard values.indices.contains(i) else { return }
            if arg == "paste" {
                Task.detached { _ = Engine.call(["op": "paste", "text": values[i]]) }
            } else {
                Model.shared.handleUi("define", args: [values[i]])
            }
        })
    }

    /// Recently used documents an app can open (Spotlight's last-used dates).
    private func showRecentDocuments(for command: Command) {
        guard command.kind == .app else { return }
        let appPath = String(command.id.dropFirst("app:".count))
        busy = true
        Task.detached {
            let docs = Model.recentDocuments(appPath: appPath)
            await MainActor.run {
                let m = Model.shared
                m.busy = false
                guard !docs.isEmpty else {
                    m.flash("No recent documents for \(command.title)", error: false, thenHide: false)
                    return
                }
                var items: [ListItem] = []
                for (i, d) in docs.enumerated() {
                    var item = ListItem(
                        title: (d.path as NSString).lastPathComponent,
                        subtitle: ((d.path as NSString).deletingLastPathComponent as NSString).abbreviatingWithTildeInPath
                            + " · " + d.used.formatted(.relative(presentation: .named)),
                        icon: "fileicon:\(d.path)",
                        arg: String(i)
                    )
                    item.quicklook = d.path
                    item.actions = [ItemAction(title: "Show in Finder", arg: "reveal", key: "cmd+shift+f")]
                    items.append(item)
                }
                m.showLocalList(id: "recent", title: "\(command.title) — Recent", icon: "🕘", items: items, action: { i in
                    guard docs.indices.contains(i) else { return }
                    NSWorkspace.shared.open(
                        [URL(fileURLWithPath: docs[i].path)],
                        withApplicationAt: URL(fileURLWithPath: appPath),
                        configuration: NSWorkspace.OpenConfiguration()
                    )
                    Model.shared.apply(.hide)
                }, itemAction: { i, _ in
                    guard docs.indices.contains(i) else { return }
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: docs[i].path)])
                    Model.shared.apply(.hide)
                })
            }
        }
    }

    nonisolated static func recentDocuments(appPath: String) -> [(path: String, used: Date)] {
        guard let info = Bundle(path: appPath)?.infoDictionary,
              let types = info["CFBundleDocumentTypes"] as? [[String: Any]] else { return [] }
        var utis = Set<String>()
        var exts = Set<String>()
        for t in types {
            for u in t["LSItemContentTypes"] as? [String] ?? [] where u != "public.data" && u != "public.item" && u != "public.content" {
                utis.insert(u)
            }
            for e in t["CFBundleTypeExtensions"] as? [String] ?? [] where e != "*" { exts.insert(e.lowercased()) }
        }
        var clauses = utis.prefix(20).map { "kMDItemContentTypeTree == '\($0)'" }
        clauses += exts.prefix(20).map { "kMDItemFSName == '*.\($0)'c" }
        guard !clauses.isEmpty else { return [] }
        let query = "kMDItemLastUsedDate >= $time.today(-60) && (\(clauses.joined(separator: " || ")))"
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/mdfind")
        p.arguments = ["-attr", "kMDItemLastUsedDate", query]
        let pipe = Pipe()
        p.standardOutput = pipe
        p.standardError = FileHandle.nullDevice
        guard (try? p.run()) != nil else { return [] }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        let fmt = DateFormatter()
        fmt.locale = Locale(identifier: "en_US_POSIX")
        fmt.dateFormat = "yyyy-MM-dd HH:mm:ss Z"
        var out: [(String, Date)] = []
        for line in String(decoding: data, as: UTF8.self).split(separator: "\n") {
            guard let r = line.range(of: "   kMDItemLastUsedDate = ") else { continue }
            let path = String(line[..<r.lowerBound])
            let value = String(line[r.upperBound...]).trimmingCharacters(in: .whitespaces)
            if let d = fmt.date(from: value), !path.contains("/Library/") { out.append((path, d)) }
        }
        return out.sorted { $0.1 > $1.1 }.prefix(25).map { (path: $0.0, used: $0.1) }
    }

    /// Chat ⌘⇧R: add what's in the input (or your last message) to profile.md.
    private func rememberInProfile() {
        let typed = chatInput.trimmingCharacters(in: .whitespacesAndNewlines)
        let text = typed.isEmpty ? (chatTurns.last { $0.role == "user" }?.content ?? "") : typed
        guard !text.isEmpty else {
            flash("Type what to remember, then ⌘⇧R", error: false, thenHide: false)
            return
        }
        let r = Engine.call(["op": "profileAppend", "text": text])
        if let e = r["error"] as? String {
            flash(e, error: true, thenHide: false)
        } else {
            if !typed.isEmpty { chatInput = "" }
            flash("Remembered in profile.md", error: false, thenHide: false)
        }
    }

    /// A lone Shift tap previews the selected row (`shift_quick_look = true`).
    func shiftChanged(_ event: NSEvent) {
        guard shiftQuickLook, launcher?.isKeyWindow == true else { return }
        let only = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if only == .shift {
            shiftDownAt = event.timestamp
            return
        }
        guard only.isEmpty, let down = shiftDownAt else { return }
        shiftDownAt = nil
        guard event.timestamp - down < 0.35, lastKeyDownAt < down else { return }
        switch mode {
        case .search, .list: break
        default: return
        }
        if isListMode, case let (_, item)? = selectedListItem(), let target = item.quicklook {
            quickLook(target)
        } else if rows.indices.contains(selected), case .command(let c) = rows[selected], let url = fileURL(for: c) {
            QuickLook.shared.show([url])
        }
    }

    // MARK: - Act on Selection

    /// Alfred's Universal Actions: what can be done with the selected text or files.
    func showSelectionActions() {
        // From a hotkey the panel is down: the app in front is the one with the selection.
        if launcher?.isVisible != true, !NSApp.isActive, let front = WindowManager.frontApp() {
            previousApp = front
        }
        let app = previousApp
        // AX read (main actor, fast); Finder's file selection needs AppleScript, off the main thread.
        let text = app.flatMap { WindowManager.selectedText(of: $0) }?.trimmingCharacters(in: .whitespacesAndNewlines)
        let isFinder = app?.bundleIdentifier == "com.apple.finder"
        busy = true
        Task.detached {
            let files = (text?.isEmpty ?? true) && isFinder ? Model.finderSelection() : []
            await MainActor.run {
                let m = Model.shared
                m.busy = false
                if !files.isEmpty {
                    m.showActions(for: .files(files))
                } else if let text, !text.isEmpty {
                    m.showActions(for: UAInput.from(text: text))
                } else if !m.fileBuffer.isEmpty {
                    m.showActions(for: .files(m.fileBuffer))
                } else {
                    m.show()
                    m.flash("Select some text or files first, then run Act on Selection (bind it to a hotkey)", error: true, thenHide: false)
                }
            }
        }
    }

    nonisolated static func finderSelection() -> [URL] {
        let source = """
        set out to {}
        tell application "Finder" to set sel to selection as alias list
        repeat with f in sel
            set end of out to POSIX path of f
        end repeat
        return out
        """
        var err: NSDictionary?
        guard let result = NSAppleScript(source: source)?.executeAndReturnError(&err) else { return [] }
        var urls: [URL] = []
        if result.numberOfItems > 0 {
            for i in 1 ... result.numberOfItems {
                if let p = result.atIndex(i)?.stringValue { urls.append(URL(fileURLWithPath: p)) }
            }
        } else if let p = result.stringValue {
            urls.append(URL(fileURLWithPath: p))
        }
        return urls
    }

    /// The shared action list for an input, as a panel list (Act on Selection).
    func showActions(for input: UAInput) {
        show()
        let actions = UniversalActions.actions(for: input, model: self)
        let items = actions.enumerated().map { i, a -> ListItem in
            var item = ListItem(title: a.title, subtitle: input.label, icon: a.icon, arg: String(i))
            if case .files(let f) = input, f.count == 1 { item.quicklook = f[0].path }
            return item
        }
        let title = switch input {
        case .text: "Selected Text"
        case .url: "Link"
        case .files(let f): f.count == 1 ? f[0].lastPathComponent : "\(f.count) Files"
        }
        showLocalList(id: "selection", title: title, icon: "✳️", items: items, action: { i in
            guard actions.indices.contains(i) else { return }
            actions[i].run()
        })
    }

    /// AI commands for Act on Selection (from the engine registry).
    var aiCommands: [Command] {
        (Engine.call(["op": "aiCommands"])["commands"] as? [[String: Any]] ?? []).compactMap(Command.init)
    }

    /// Row actions for panel-built lists (Contacts, Reminders): (item index, action arg).
    var localItemAction: ((Int, String) -> Void)?

    /// A list the panel builds itself; Return calls `action` with the item's index.
    func showLocalList(
        id: String, title: String, icon: String, items: [ListItem],
        action: @escaping (Int) -> Void,
        itemAction: ((Int, String) -> Void)? = nil,
        options: ListOptions = ListOptions()
    ) {
        let command = Command(id: "local:\(id)", title: title, subtitle: nil, icon: icon, kind: .builtin, arguments: [], hotkey: nil)
        show()
        localListAction = action
        localItemAction = itemAction
        listOptions = options
        mode = .list(command: command, items: items, filter: true)
        query = ""
        selected = 0
        listStart = 0
        noteContentChanged()
        focusTick += 1
        updatePreview()
    }

    func showMenuItems() {
        guard WindowManager.accessibilityTrusted else {
            flash(WindowManager.permissionHint, error: true, thenHide: false)
            return
        }
        guard let app = previousApp else {
            flash("No app to search — open the launcher from the app you want", error: true, thenHide: false)
            return
        }
        busy = true
        let pid = app.processIdentifier
        let appName = app.localizedName ?? "App"
        Task.detached {
            let found = MenuSearch.collect(pid: pid)
            await MainActor.run {
                let model = Model.shared
                model.busy = false
                guard !found.isEmpty else {
                    model.flash("\(appName) has no searchable menus", error: true, thenHide: false)
                    return
                }
                let items = found.enumerated().map { i, item in
                    ListItem(
                        title: item.title,
                        subtitle: item.shortcut.isEmpty ? item.path : "\(item.path)   \(item.shortcut)",
                        icon: "☰",
                        arg: String(i)
                    )
                }
                model.showLocalList(id: "menu", title: "\(appName) Menus", icon: "☰", items: items) { i in
                    guard found.indices.contains(i) else { return }
                    let target = Model.shared.previousApp
                    Model.shared.apply(.hide)
                    MenuSearch.press(found[i], in: target)
                }
            }
        }
    }

    func showSchedule() {
        let schedule = Schedule.shared
        guard schedule.authorized else {
            if schedule.notDetermined {
                schedule.requestAccess { granted in
                    if granted { Model.shared.showSchedule() } else {
                        Model.shared.flash("Calendar access is off (System Settings → Privacy & Security → Calendars)", error: true, thenHide: false)
                    }
                }
            } else {
                flash("Calendar access is off (System Settings → Privacy & Security → Calendars)", error: true, thenHide: false)
            }
            return
        }
        schedule.refresh(force: true)
        let items = schedule.listItems()
        guard !items.isEmpty else {
            flash("Nothing else on your calendar today or tomorrow", error: false, thenHide: false)
            return
        }
        showLocalList(id: "schedule", title: "My Schedule", icon: "📅", items: items) { i in
            Schedule.shared.open(index: i)
            Model.shared.apply(.hide)
        }
    }

    func toggleDictation() {
        let chatting: Bool = { if case .chat = mode { return true } else { return false } }()
        let base = chatting ? chatInput : ""
        Dictation.shared.toggle(onText: { text in
            let m = Model.shared
            if chatting {
                m.chatInput = base.isEmpty ? text : base + " " + text
            } else {
                m.query = text
            }
        }, onEnd: {
            Model.shared.focusTick += 1
        })
    }

    private func openActions() {
        guard rows.indices.contains(selected),
              case .command(let command) = rows[selected]
        else { return }
        openWithFor = nil
        mode = .actions(command)
        selected = 0
        listStart = 0
        noteContentChanged()
        focusTick += 1
    }

    /// "Open With…" sub-list for this command id, when showing it.
    private var openWithFor: String?
    private var openWithApps: [URL] = []

    func actionRows(for command: Command) -> [ActionRow] {
        if command.id.hasPrefix("item:"), case let (_, item)? = listItem(for: command) {
            return listItemRows(item)
        }
        if openWithFor == command.id {
            return openWithApps.map { ActionRow(id: "openwith:\($0.path)", title: FileManager.default.displayName(atPath: $0.path)) }
        }
        if command.kind == .clipboard {
            var rows = [
                ActionRow(id: "paste", title: "Paste", key: nil),
                ActionRow(id: "copy-clip", title: "Copy to Clipboard", key: "cmd+c"),
                ActionRow(id: "clip-pin", title: "Pin / Unpin", key: "cmd+shift+p"),
                ActionRow(id: "clip-delete", title: "Delete", key: "ctrl+x"),
                ActionRow(id: "clip-delete-all", title: "Delete All (keep pinned)"),
            ]
            if command.id.hasPrefix("clip:"), ClipboardHistory.shared.text(for: command.id) != nil {
                rows.insert(ActionRow(id: "paste-plain", title: "Paste as Plain Text", key: "cmd+shift+v"), at: 1)
            }
            if ClipboardHistory.shared.ocrText(for: command.id) != nil {
                rows.insert(ActionRow(id: "copy-ocr", title: "Copy Text from Image"), at: 1)
            }
            if command.id.hasPrefix("clip:"), ClipboardHistory.shared.text(for: command.id) != nil {
                rows.append(ActionRow(id: "save-snippet", title: "Save as Snippet…", key: "cmd+s"))
            }
            if ClipboardHistory.shared.fileURL(for: command.id) != nil {
                rows.insert(ActionRow(id: "quick-look", title: "Quick Look", key: "cmd+y"), at: 1)
            }
            return rows
        }
        let id = command.id
        var rows: [ActionRow] = []
        switch command.kind {
        case .app:
            rows = [
                ActionRow(id: "run", title: "Open"),
                ActionRow(id: "reveal", title: "Show in Finder", key: "cmd+shift+f"),
                ActionRow(id: "quick-look", title: "Quick Look", key: "cmd+y"),
                ActionRow(id: "copy-path", title: "Copy Path", key: "cmd+shift+c"),
            ]
            rows.append(ActionRow(id: "recent-docs", title: "Recent Documents", key: "cmd+r"))
            if runningApp(for: command) != nil {
                rows.append(ActionRow(id: "quit-app", title: "Quit \(command.title)", key: "cmd+q"))
                rows.append(ActionRow(id: "hide-app", title: "Hide \(command.title)", key: "cmd+h"))
            }
        case .file:
            // Same actions as Act on Selection; with files in the buffer they act on all of them.
            guard let url = fileURL(for: command) else { break }
            var files = fileBuffer
            if !files.contains(url) { files.append(url) }
            let actions = UniversalActions.actions(for: .files(files), model: self)
            uaActions = Dictionary(actions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            rows = actions.map { ActionRow(id: $0.id, title: $0.title, key: $0.key) }
            rows.append(ActionRow(id: "buffer-add", title: fileBuffer.contains(url) ? "Remove from File Buffer" : "Add to File Buffer", key: "alt+up"))
        case .snippet:
            rows = [
                ActionRow(id: "run", title: "Paste"),
                ActionRow(id: "copy-preview", title: "Copy Text", key: "cmd+shift+c"),
            ]
        case .ai:
            rows = [
                ActionRow(id: "run", title: "Run"),
                ActionRow(id: "run-chat", title: "Run in Chat", key: "cmd+return"),
                ActionRow(id: "edit-config", title: "Edit in config.toml", key: "cmd+e"),
            ]
        case .script:
            rows = [
                ActionRow(id: "run", title: "Run"),
                ActionRow(id: "edit-script", title: "Edit Script", key: "cmd+e"),
                ActionRow(id: "reveal-script", title: "Show in Finder", key: "cmd+shift+f"),
            ]
        case .quicklink:
            rows = [
                ActionRow(id: "run", title: "Open"),
                ActionRow(id: "copy-preview", title: "Copy URL", key: "cmd+shift+c"),
                ActionRow(id: "edit-config", title: "Edit in config.toml", key: "cmd+e"),
            ]
        case .shortcut:
            let name = String(id.dropFirst("sc:".count))
            rows = [
                ActionRow(id: "run", title: "Run"),
                ActionRow(id: "run-with-clipboard", title: "Run with Clipboard as Input", key: "cmd+shift+v"),
                ActionRow(id: "edit-shortcut", title: "Edit \(name) in Shortcuts", key: "cmd+e"),
            ]
        case .mcp:
            rows = [
                ActionRow(id: "run", title: "Run Tool"),
                ActionRow(id: "copy-tool", title: "Copy Tool Name", key: "cmd+shift+c"),
            ]
        default:
            if id.hasPrefix("note:") {
                rows = [
                    ActionRow(id: "run", title: "Open Note"),
                    ActionRow(id: "paste-note", title: "Paste into \(previousApp?.localizedName ?? "Front App")", key: "cmd+shift+v"),
                    ActionRow(id: "copy-preview", title: "Copy Note", key: "cmd+shift+c"),
                    ActionRow(id: "delete-note", title: "Delete Note", key: "ctrl+x"),
                ]
            } else if id.hasPrefix("chat:") {
                rows = [
                    ActionRow(id: "run", title: "Continue Chat"),
                    ActionRow(id: "copy-preview", title: "Copy Transcript", key: "cmd+shift+c"),
                    ActionRow(id: "delete-chat", title: "Delete Chat", key: "ctrl+x"),
                ]
            } else {
                rows = [ActionRow(id: "run", title: "Run")]
            }
        }
        rows.append(ActionRow(id: "pin", title: "Pin / Unpin", key: "cmd+shift+p"))
        if Self.takesHotkey(command.id) {
            if let hk = command.hotkey {
                rows.append(ActionRow(id: "record-hotkey", title: "Change Hotkey (\(hk))…"))
                rows.append(ActionRow(id: "remove-hotkey", title: "Remove Hotkey"))
            } else {
                rows.append(ActionRow(id: "record-hotkey", title: "Record Hotkey…"))
            }
        }
        rows.append(ActionRow(id: "copy-title", title: "Copy Title"))
        rows.append(ActionRow(id: "copy-id", title: "Copy Command Id"))
        return rows
    }

    /// Registry commands can have global hotkeys; transient rows cannot.
    static func takesHotkey(_ id: String) -> Bool {
        let transient = ["clip:", "shot:", "item:", "local:", "note:", "chat:", "emoji:", "file:", "bm:", "calc:", "event:", "define:", "spell:", "quit:"]
        return !transient.contains { id.hasPrefix($0) }
    }

    private func runningApp(for command: Command) -> NSRunningApplication? {
        guard command.kind == .app else { return nil }
        let path = String(command.id.dropFirst(4))
        return NSWorkspace.shared.runningApplications.first { $0.bundleURL?.path == path }
    }

    /// A file URL behind the row, for Quick Look / Finder actions.
    func fileURL(for command: Command) -> URL? {
        if command.kind == .clipboard { return ClipboardHistory.shared.fileURL(for: command.id) }
        if command.id.hasPrefix("app:") { return URL(fileURLWithPath: String(command.id.dropFirst(4))) }
        if command.id.hasPrefix("file:") { return URL(fileURLWithPath: String(command.id.dropFirst(5))) }
        return nil
    }

    func runAction(_ actionId: String, for command: Command) {
        if actionId.hasPrefix("item-") {
            runListAction(actionId)
            return
        }
        if actionId.hasPrefix("ua:"), let a = uaActions[actionId] {
            mode = .search
            a.run()
            return
        }
        if actionId == "buffer-add" {
            mode = .search
            toggleBuffer(command)
            return
        }
        if actionId.hasPrefix("openwith:"), let file = fileURL(for: command) {
            let app = URL(fileURLWithPath: String(actionId.dropFirst("openwith:".count)))
            NSWorkspace.shared.open([file], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
            openWithFor = nil
            apply(.hide)
            return
        }
        switch actionId {
        case "run":
            mode = .search
            activateCommand(command)
        case "open-with":
            guard let file = fileURL(for: command) else { break }
            openWithApps = Array(NSWorkspace.shared.urlsForApplications(toOpen: file).prefix(12))
            openWithFor = command.id
            mode = .actions(command)
            selected = 0
            noteContentChanged()
            return
        case "quick-look":
            if let url = fileURL(for: command) { QuickLook.shared.show([url]) }
            mode = .search
        case "quit-app":
            runningApp(for: command)?.terminate()
            mode = .search
            flash("Quit \(command.title)", error: false, thenHide: false)
        case "hide-app":
            runningApp(for: command)?.hide()
            mode = .search
        case "copy-file":
            if let url = fileURL(for: command) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.writeObjects([url as NSURL])
                flash("Copied file", error: false, thenHide: false)
            }
            mode = .search
        case "terminal":
            if let url = fileURL(for: command) {
                var isDir: ObjCBool = false
                FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
                let dir = isDir.boolValue ? url : url.deletingLastPathComponent()
                if let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") {
                    NSWorkspace.shared.open([dir], withApplicationAt: terminal, configuration: NSWorkspace.OpenConfiguration())
                }
            }
            apply(.hide)
            return
        case "trash":
            if let url = fileURL(for: command) {
                NSWorkspace.shared.recycle([url]) { _, error in
                    Task { @MainActor in
                        Model.shared.flash(error == nil ? "Moved to Trash" : "Could not move to Trash", error: error != nil, thenHide: false)
                        if Model.shared.ready { Model.shared.search() }
                    }
                }
            }
            mode = .search
        case "ask-file":
            guard let url = fileURL(for: command) else { break }
            let name = url.lastPathComponent
            Task.detached {
                let text = Attachments.text(of: url)
                await MainActor.run {
                    Model.shared.openPlainChatPublic(prompt: nil)
                    if let text { Model.shared.attach(ChatAttachment(name: name, text: text)) }
                }
            }
            return
        case "copy-preview":
            Task.detached {
                let raw = Engine.call(["op": "preview", "id": command.id])
                let text = ((raw["preview"] as? [String: Any])?["text"] as? String) ?? ""
                await MainActor.run { Model.shared.copyToClipboard(text, toast: "Copied") }
            }
            mode = .search
        case "run-chat":
            mode = .search
            execute(id: command.id, args: [], output: "chat")
            return
        case "edit-config":
            Task.detached {
                let dir = Engine.call(["op": "configDir"])["path"] as? String ?? ""
                NSWorkspace.shared.open(URL(fileURLWithPath: dir).appendingPathComponent("config.toml"))
            }
            apply(.hide)
            return
        case "edit-script", "reveal-script":
            Task.detached {
                let raw = Engine.call(["op": "preview", "id": command.id])
                guard let path = (raw["preview"] as? [String: Any])?["path"] as? String else { return }
                await MainActor.run {
                    if actionId == "reveal-script" {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                    } else {
                        let p = Process()
                        p.executableURL = URL(fileURLWithPath: "/usr/bin/open")
                        p.arguments = ["-t", path]
                        try? p.run()
                    }
                    Model.shared.apply(.hide)
                }
            }
            return
        case "run-with-clipboard":
            let input = NSPasteboard.general.string(forType: .string) ?? ""
            mode = .search
            execute(id: command.id, args: [input])
            return
        case "edit-shortcut":
            let name = String(command.id.dropFirst("sc:".count))
            if let enc = name.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed),
               let url = URL(string: "shortcuts://open-shortcut?name=\(enc)")
            {
                NSWorkspace.shared.open(url)
            }
            apply(.hide)
            return
        case "copy-tool":
            let parts = command.id.split(separator: ":")
            copyToClipboard(parts.count >= 3 ? String(parts[2]) : command.id, toast: "Copied tool name")
            mode = .search
        case "paste-note":
            mode = .search
            let id = command.id
            Task.detached {
                let p = Engine.call(["op": "preview", "id": id])["preview"] as? [String: Any]
                guard let text = p?["text"] as? String else { return }
                _ = Engine.call(["op": "paste", "text": text])
            }
        case "delete-note":
            Task.detached {
                _ = Engine.call(["op": "noteDelete", "id": command.id])
                await MainActor.run {
                    Model.shared.flash("Deleted note", error: false, thenHide: false)
                    if Model.shared.ready { Model.shared.search() }
                }
            }
            mode = .search
        case "delete-chat":
            Task.detached {
                _ = Engine.call(["op": "chatDelete", "id": command.id])
                await MainActor.run {
                    Model.shared.flash("Deleted chat", error: false, thenHide: false)
                    if Model.shared.ready { Model.shared.search() }
                }
            }
            mode = .search
        case "pin":
            Task.detached {
                let r = Engine.call(["op": "pin", "id": command.id])
                let pinned = r["pinned"] as? Bool ?? false
                await MainActor.run {
                    Model.shared.mode = .search
                    Model.shared.flash(pinned ? "Pinned" : "Unpinned", error: false, thenHide: false)
                    if Model.shared.ready { Model.shared.search() }
                }
            }
        case "copy-title":
            copyToClipboard(command.title, toast: "Copied title")
            mode = .search
        case "copy-id":
            copyToClipboard(command.id, toast: "Copied id")
            mode = .search
        case "record-hotkey":
            mode = .search
            recordingHotkeyFor = command
            flash("Press the new hotkey for “\(command.title)” · Esc cancels", error: false, thenHide: false)
        case "remove-hotkey":
            mode = .search
            setHotkey("", for: command)
        case "recent-docs":
            showRecentDocuments(for: command)
        case "save-snippet":
            guard let text = ClipboardHistory.shared.text(for: command.id) else { return }
            let form = Command(
                id: "builtin:save-snippet", title: "Save as Snippet", subtitle: nil, icon: "📎", kind: .builtin,
                arguments: [
                    Argument(placeholder: "keyword (;sig)", optional: false, secure: false),
                    Argument(placeholder: "text", optional: false, secure: false),
                ],
                hotkey: nil
            )
            showArgs(form)
            if argDrafts.count > 1 { argDrafts[1] = text }
        case "copy-path":
            if let url = fileURL(for: command) {
                copyToClipboard(url.path, toast: "Copied path")
            }
            mode = .search
        case "reveal":
            if let url = fileURL(for: command) {
                NSWorkspace.shared.activateFileViewerSelecting([url])
            }
            mode = .search
            hide()
        case "paste":
            mode = .search
            perform(command.id)
        case "paste-plain":
            mode = .search
            if let text = ClipboardHistory.shared.text(for: command.id) {
                busy = true
                Task.detached {
                    _ = Engine.call(["op": "paste", "text": text])
                    await MainActor.run {
                        Model.shared.busy = false
                        Model.shared.apply(.hide)
                    }
                }
            }
        case "copy-clip":
            if ClipboardHistory.shared.copyOnly(command.id) {
                flash("Copied", error: false, thenHide: false)
            }
            mode = .search
        case "copy-ocr":
            if let ocr = ClipboardHistory.shared.ocrText(for: command.id) {
                copyToClipboard(ocr, toast: "Copied text from image")
            }
            mode = .search
        case "clip-pin":
            if let pinned = ClipboardHistory.shared.togglePin(command.id) {
                flash(pinned ? "Pinned" : "Unpinned", error: false, thenHide: false)
                search()
            }
            mode = .search
        case "clip-delete":
            ClipboardHistory.shared.delete(command.id)
            flash("Deleted", error: false, thenHide: false)
            mode = .search
            search()
        case "clip-delete-all":
            ClipboardHistory.shared.deleteAll(keepPinned: true)
            flash("Cleared clipboard history", error: false, thenHide: false)
            mode = .search
            search()
        default:
            mode = .search
        }
        noteContentChanged()
    }

    private func togglePinSelected() {
        guard rows.indices.contains(selected),
              case .command(let command) = rows[selected]
        else { return }
        Task.detached {
            let r = Engine.call(["op": "pin", "id": command.id])
            let pinned = r["pinned"] as? Bool ?? false
            await MainActor.run {
                Model.shared.flash(pinned ? "Pinned" : "Unpinned", error: false, thenHide: false)
                if Model.shared.ready { Model.shared.search() }
            }
        }
    }

    /// Deep link: `opencast://run/<id>` or `opencast://search?q=…`.
    func handleDeepLink(_ url: URL) {
        switch DeepLink.parse(url) {
        case .ignored:
            return
        case .importAlfred(let path):
            // Finder › Open With › OpenCast on an .alfredworkflow / .alfredsnippets.
            show()
            execute(id: "builtin:import-alfred", args: [path])
        case .search(let q):
            show()
            query = q
            selected = 0
            listStart = 0
            noteContentChanged()
            focusTick += 1
            if ready { search() }
        case .run(let id):
            show()
            if ready {
                perform(id)
            } else {
                // A link can arrive before the engine has loaded its commands.
                Task { @MainActor in
                    for _ in 0..<40 where !Model.shared.ready {
                        try? await Task.sleep(nanoseconds: 50_000_000)
                    }
                    Model.shared.perform(id)
                }
            }
        }
    }

    private func argsKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            submitArgs()
            return true
        }
        if mods.isEmpty, event.keyCode == 51, argField == 0, argDrafts.first?.isEmpty != false {
            backToSearch(clearQuery: false)
            return true
        }
        if mods.isEmpty, event.keyCode == 48, argDrafts.count > 1 {
            argField = (argField + 1) % argDrafts.count
            focusTick += 1
            return true
        }
        return false
    }

    private func outputKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        let chars = event.charactersIgnoringModifiers?.lowercased()
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            copyOutput()
            return true
        }
        if mods == .command, chars == "c" {
            if textHasSelection { return false }
            copyOutput()
            return true
        }
        return false
    }

    private func chatKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        if pendingApproval != nil, mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            answerApproval(true)
            return true
        }
        let chars = event.charactersIgnoringModifiers?.lowercased()
        if mods == .command, chars == "o" {
            attachFile()
            return true
        }
        if mods == [.command, .shift], chars == "v" {
            attachClipboard()
            return true
        }
        if mods == [.command, .shift], chars == "s" {
            attachScreen()
            return true
        }
        if keyMatches(event, "paste_answer") {
            pasteLastAnswer()
            return true
        }
        if keyMatches(event, "copy_answer") {
            copyAnswer()
            return true
        }
        if mods == [.command, .shift], event.charactersIgnoringModifiers?.lowercased() == "r" {
            rememberInProfile()
            return true
        }
        if keyMatches(event, "new_chat") {
            newChat()
            return true
        }
        if keyMatches(event, "dictate") {
            toggleDictation()
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            if fieldIsComposing { return false }
            sendChat(chatInput)
            return true
        }
        return false
    }

    private func submitArgs() {
        guard case .args(let command) = mode else { return }
        if let missing = command.arguments.enumerated().first(where: { index, spec in
            !spec.optional && draft(index).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        })?.offset {
            argField = missing
            focusTick += 1
            return
        }
        let args = command.arguments.indices.map { draft($0) }
        execute(id: command.id, args: args)
    }

    private func draft(_ index: Int) -> String {
        argDrafts.indices.contains(index) ? argDrafts[index] : ""
    }

    func sendChat(_ text: String) {
        let content = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty, !chatStreaming else { return }
        let temperature = chatTemperature
        let maxTokens = chatMaxTokens
        let history = chatTurns.filter { turn in
            !(turn.role == "assistant" && (turn.error != nil || turn.content.isEmpty))
        }
        let attached = chatAttachments
        chatAttachments = []
        let context = attached.map { "<attachment name=\"\($0.name)\">\n\($0.text)\n</attachment>" }.joined(separator: "\n\n")
        let userTurn = ChatTurn(role: "user", content: content, context: context, attachmentNames: attached.map(\.name))
        let messages = (history + [userTurn]).map { ["role": $0.role, "content": $0.wireContent] }
        let id = UUID().uuidString
        chatRequestId = id
        chatStreaming = true
        chatTurns = history + [userTurn, ChatTurn(role: "assistant", content: "")]
        chatInput = ""
        let provider = chatProviderId
        let model = chatModel
        let system = chatSystem
        let preset = chatPreset
        Task.detached {
            var body: [String: Any] = [
                "op": "stream",
                "requestId": id,
                "messages": messages,
            ]
            if let preset { body["preset"] = preset }
            if let provider { body["provider"] = provider }
            if let model { body["model"] = model }
            if let system { body["system"] = system }
            if let temperature { body["temperature"] = temperature }
            if let maxTokens { body["maxTokens"] = maxTokens }
            let result = Engine.call(body)
            if let error = result["error"] as? String {
                await MainActor.run { Model.shared.failStream(id, error) }
            }
        }
    }

    /// `output = replace | paste | copy`: stream without opening chat, then act.
    private func runSilentAi(prompt: String, system: String?, provider: String?, model: String?, output: AiOutput) {
        stopStream()
        let id = UUID().uuidString
        chatRequestId = id
        chatStreaming = true
        silentOutput = output
        silentBuffer = ""
        busy = true
        flash("Thinking…", error: false, thenHide: false)
        let temperature = chatTemperature
        let maxTokens = chatMaxTokens
        Task.detached {
            var body: [String: Any] = [
                "op": "stream",
                "requestId": id,
                "messages": [["role": "user", "content": prompt]],
                // A rewrite/paste command: no tools, no profile.
                "tools": false,
                "profile": false,
            ]
            if let provider { body["provider"] = provider }
            if let model { body["model"] = model }
            if let system { body["system"] = system }
            if let temperature { body["temperature"] = temperature }
            if let maxTokens { body["maxTokens"] = maxTokens }
            let result = Engine.call(body)
            if let error = result["error"] as? String {
                await MainActor.run { Model.shared.failStream(id, error) }
            }
        }
    }

    private func finishSilentAi(error: String?) {
        let output = silentOutput
        let text = silentBuffer.trimmingCharacters(in: .whitespacesAndNewlines)
        silentOutput = nil
        silentBuffer = ""
        busy = false
        toastText = nil
        if let error {
            flash(error, error: true, thenHide: false)
            return
        }
        guard !text.isEmpty else {
            flash("The model returned nothing", error: true, thenHide: false)
            return
        }
        switch output {
        case .copy:
            copyToClipboard(text, toast: "Copied answer")
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 900_000_000)
                self.apply(.hide)
            }
        default:
            // replace / paste: ⌘V over the still-selected text in the previous app.
            query = ""
            mode = .search
            Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
        }
    }

    private func loadProviders() {
        let gen = providerGen
        Task.detached {
            let result = Engine.call(["op": "providers"])
            let parsed = (result["providers"] as? [[String: Any]])?.compactMap(Provider.init) ?? []
            await MainActor.run {
                guard Model.shared.providerGen == gen, case .chat = Model.shared.mode else { return }
                Model.shared.providers = parsed
                Model.shared.providersReady = true
                if Model.shared.chatProviderId == nil {
                    Model.shared.chatProviderId = parsed.first { $0.isDefault }?.id ?? parsed.first?.id
                }
            }
        }
    }

    private func applyAI(_ obj: [String: Any]?) {
        guard let requestId = obj?["requestId"] as? String, requestId == chatRequestId,
              let event = obj?["event"] as? [String: Any],
              let kind = event["type"] as? String
        else { return }
        if silentOutput != nil {
            switch kind {
            case "delta":
                silentBuffer += event["text"] as? String ?? ""
            case "error":
                endStream()
                finishSilentAi(error: event["text"] as? String ?? "Error")
            case "done":
                endStream()
                finishSilentAi(error: nil)
            default:
                break
            }
            return
        }
        switch kind {
        case "delta":
            guard !chatTurns.isEmpty else { return }
            var turns = chatTurns
            turns[turns.count - 1].content += event["text"] as? String ?? ""
            chatTurns = turns
        case "tool":
            guard !chatTurns.isEmpty else { return }
            var turns = chatTurns
            turns[turns.count - 1].tools.append(event["text"] as? String ?? "tool")
            chatTurns = turns
            if pendingApproval?.callId == event["callId"] as? String { pendingApproval = nil }
        case "approve":
            let args: String = {
                guard let a = event["arguments"], let data = try? JSONSerialization.data(withJSONObject: a, options: [.prettyPrinted, .sortedKeys]) else { return "{}" }
                return String(data: data, encoding: .utf8) ?? "{}"
            }()
            pendingApproval = ToolApproval(
                requestId: requestId,
                callId: event["callId"] as? String ?? "",
                title: event["text"] as? String ?? "tool",
                arguments: args
            )
            NSSound(named: "Tink")?.play()
        case "error":
            if !chatTurns.isEmpty {
                var turns = chatTurns
                turns[turns.count - 1].error = event["text"] as? String ?? "Error"
                chatTurns = turns
            }
            pendingApproval = nil
            endStream()
        case "done":
            pendingApproval = nil
            endStream()
            saveChat()
        default:
            break
        }
    }

    private func failStream(_ id: String, _ message: String) {
        guard chatRequestId == id else { return }
        if silentOutput != nil {
            endStream()
            finishSilentAi(error: message)
            return
        }
        if !chatTurns.isEmpty {
            var turns = chatTurns
            turns[turns.count - 1].error = message
            chatTurns = turns
        }
        endStream()
    }

    private func endStream() {
        chatRequestId = nil
        chatStreaming = false
        focusTick += 1
    }

    /// Clears the streaming flag, then asks Rust to abort. No `done` follows.
    private func stopStream() {
        let id = chatRequestId
        chatRequestId = nil
        chatStreaming = false
        if silentOutput != nil {
            silentOutput = nil
            silentBuffer = ""
            busy = false
        }
        guard let id else { return }
        Task.detached { _ = Engine.call(["op": "cancel", "requestId": id]) }
    }

    /// Answer the pending tool approval.
    func answerApproval(_ allow: Bool) {
        guard let a = pendingApproval else { return }
        pendingApproval = nil
        Task.detached {
            _ = Engine.call(["op": "approveTool", "requestId": a.requestId, "callId": a.callId, "allow": allow])
        }
    }

    /// Persist the current chat (after each finished answer).
    private func saveChat() {
        let turns = chatTurns.filter { !$0.content.isEmpty && $0.error == nil }
        guard turns.contains(where: { $0.role == "assistant" }) else { return }
        let messages: [[String: Any]] = turns.map { ["role": $0.role, "content": $0.wireContent] }
        var chat: [String: Any] = [
            "id": chatId,
            "title": "",
            "updated": Int(Date().timeIntervalSince1970),
            "messages": messages,
        ]
        if let p = chatProviderId { chat["provider"] = p }
        if let m = activeChatModel { chat["model"] = m }
        if let s = chatSystem { chat["system"] = s }
        let body: [String: Any] = ["op": "chatSave", "chat": chat]
        Task.detached { _ = Engine.call(body) }
    }

    func attach(_ attachment: ChatAttachment) {
        chatAttachments.append(attachment)
        focusTick += 1
    }

    func attachFile() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        keepOnBlurOnce = true
        guard panel.runModal() == .OK else { return }
        for url in panel.urls {
            Task.detached {
                let text = Attachments.text(of: url)
                await MainActor.run {
                    if let text {
                        Model.shared.attach(ChatAttachment(name: url.lastPathComponent, text: text))
                    } else {
                        Model.shared.flash("Could not read \(url.lastPathComponent)", error: true, thenHide: false)
                    }
                }
            }
        }
        launcher?.makeKeyAndOrderFront(nil)
    }

    func attachClipboard() {
        if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
            attach(ChatAttachment(name: "Clipboard", text: text))
            return
        }
        if let data = NSPasteboard.general.data(forType: .png) ?? NSPasteboard.general.data(forType: .tiff) {
            Task.detached {
                let text = Attachments.ocr(data: data) ?? ""
                await MainActor.run {
                    if text.isEmpty {
                        Model.shared.flash("No text in the clipboard image", error: true, thenHide: false)
                    } else {
                        Model.shared.attach(ChatAttachment(name: "Clipboard image", text: text))
                    }
                }
            }
            return
        }
        flash("The clipboard is empty", error: true, thenHide: false)
    }

    /// OCR the screen under the panel (needs Screen Recording).
    func attachScreen() {
        let target = previousApp?.localizedName ?? "Screen"
        launcher?.alphaValue = 0
        Task.detached {
            try? await Task.sleep(nanoseconds: 120_000_000)
            let text = Attachments.screenText()
            await MainActor.run {
                Model.shared.launcher?.alphaValue = 1
                if let text, !text.isEmpty {
                    Model.shared.attach(ChatAttachment(name: "\(target) (screen)", text: text))
                } else {
                    Model.shared.flash("No text on screen (Screen Recording permission?)", error: true, thenHide: false)
                }
            }
        }
    }

    private func newChat() {
        stopStream()
        chatTurns = []
        focusTick += 1
    }

    private func copyOutput() {
        guard case .output(_, let text) = mode else { return }
        copyToClipboard(text, toast: "Copied output")
    }

    private func copyAnswer() {
        guard let text = lastAnswer else { return }
        copyToClipboard(text, toast: "Copied answer")
    }

    private func pasteLastAnswer() {
        guard let text = lastAnswer else { return }
        Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
    }

    private var fieldIsComposing: Bool {
        (NSApp.keyWindow?.firstResponder as? NSTextView)?.hasMarkedText() == true
    }

    private var textHasSelection: Bool {
        guard let view = NSApp.keyWindow?.firstResponder as? NSTextView else { return false }
        return view.selectedRange().length > 0
    }

    func flash(_ text: String, error: Bool, thenHide: Bool) {
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

    var seekSpans: [SeekSpan] {
        guard let seekScan else { return [] }
        return matchSpans(scan: seekScan, query: seekQuery)
    }

    var seekCursor: Int {
        let n = seekSpans.count
        guard n > 0 else { return 0 }
        return ((seekIndex % n) + n) % n
    }

    var seekCount: String {
        let n = seekSpans.count
        if n == 0 {
            return seekQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "" : "0 of 0"
        }
        return "\(seekCursor + 1) of \(n)"
    }

    private func presentSeek(_ scan: SeekScan) {
        guard let seek else { return }
        clicking = true
        seekScan = scan
        let anchor = NSScreen.screens.max(by: { $0.frame.maxY < $1.frame.maxY }) ?? NSScreen.main!
        let x = scan.monitor.x
        let y = anchor.frame.maxY - scan.monitor.y - scan.monitor.h
        seek.setFrame(NSRect(x: x, y: y, width: scan.monitor.w, height: scan.monitor.h), display: true)
        // The line starts under the search field when it is on this display.
        if let f = launcher?.frame, f.midX >= x, f.midX <= x + scan.monitor.w, f.minY >= y, f.minY <= y + scan.monitor.h {
            seekLeaderFrom = CGPoint(x: f.midX - x, y: y + scan.monitor.h - f.minY)
        } else {
            seekLeaderFrom = nil
        }
        seek.orderFront(nil)
        launcher?.orderFront(nil)
    }

    private func endSeek() {
        cancelClick()
    }

    private func seekKey(_ event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection([.command, .shift, .option, .control])
        if event.keyCode == 53, mods.isEmpty {
            endSeek()
            return true
        }
        if seekScan == nil {
            // OCR still running: swallow Return/Tab so we do not activate a command.
            return event.keyCode == 36 || event.keyCode == 76 || event.keyCode == 48
        }
        if event.keyCode == 48, mods.isEmpty || mods == .shift {
            stepSeek(mods == .shift ? -1 : 1)
            return true
        }
        if mods.isEmpty, event.characters == ";" {
            stepSeek(1)
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            clickSeek()
            return true
        }
        return false
    }

    private func stepSeek(_ delta: Int) {
        let n = seekSpans.count
        guard n > 0 else { return }
        seekIndex = ((seekIndex + delta) % n + n) % n
    }

    private func clickSeek() {
        guard let scan = seekScan else { return }
        let spans = seekSpans
        let cursor = seekCursor
        guard spans.indices.contains(cursor) else { return }
        let match = spans[cursor]
        let x = Double(scan.monitor.x + match.x + match.w / 2)
        let y = Double(scan.monitor.y + match.y + match.h / 2)
        clicking = false
        seekScan = nil
        seekQuery = ""
        seekIndex = 0
        seek?.orderOut(nil)
        hide(fromRust: true)
        Task.detached { _ = Engine.call(["op": "seekClick", "x": x, "y": y]) }
    }

    private func frame(on screen: NSScreen, height: CGFloat) -> NSRect {
        let width: CGFloat = CGFloat(themeFile.width ?? 760)
        let x = screen.frame.midX - width / 2
        let y: CGFloat = presentation == "spotlight"
            ? screen.frame.midY - height / 2
            : screen.frame.maxY - height
        return NSRect(x: x, y: y, width: width, height: height)
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
        layer?.backgroundColor = NSColor.clear.cgColor
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
            alert.informativeText = "This copy quit: two launchers would fight over the same hotkeys. Use the one already in your menu bar."
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

    /// First run: walk through permissions once.
    @MainActor
    static func onboardIfNeeded() {
        let key = "OpenCastOnboarded"
        guard !UserDefaults.standard.bool(forKey: key) else { return }
        UserDefaults.standard.set(true, forKey: key)
        if !AXIsProcessTrusted() {
            SettingsWindowController.shared.show(tab: .permissions)
        }
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
