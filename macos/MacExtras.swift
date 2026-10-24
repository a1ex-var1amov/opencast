import AppKit
import Contacts
import CoreServices
import EventKit
import SwiftUI

// MARK: - Contacts

/// "Search Contacts": every contact as a row; Return opens the card,
/// ⌘K emails, calls, messages or copies.
@MainActor
enum ContactsSearch {
    struct Person: Sendable {
        var id: String
        var name: String
        var org: String
        var emails: [String]
        var phones: [String]
    }

    static func load(_ done: @escaping @MainActor ([Person]?) -> Void) {
        let store = CNContactStore()
        let fetch: @Sendable () -> Void = {
            let keys: [CNKeyDescriptor] = [
                CNContactGivenNameKey, CNContactFamilyNameKey, CNContactOrganizationNameKey,
                CNContactEmailAddressesKey, CNContactPhoneNumbersKey, CNContactIdentifierKey,
            ] as [CNKeyDescriptor]
            var people: [Person] = []
            let request = CNContactFetchRequest(keysToFetch: keys)
            request.sortOrder = .userDefault
            try? store.enumerateContacts(with: request) { c, _ in
                let name = [c.givenName, c.familyName].filter { !$0.isEmpty }.joined(separator: " ")
                people.append(Person(
                    id: c.identifier,
                    name: name.isEmpty ? c.organizationName : name,
                    org: name.isEmpty ? "" : c.organizationName,
                    emails: c.emailAddresses.map { $0.value as String },
                    phones: c.phoneNumbers.map { $0.value.stringValue }
                ))
            }
            let found = people.filter { !$0.name.isEmpty }
            Task { @MainActor in done(found) }
        }
        switch CNContactStore.authorizationStatus(for: .contacts) {
        case .authorized:
            DispatchQueue.global(qos: .userInitiated).async(execute: fetch)
        case .notDetermined:
            store.requestAccess(for: .contacts) { granted, _ in
                if granted {
                    DispatchQueue.global(qos: .userInitiated).async(execute: fetch)
                } else {
                    Task { @MainActor in done(nil) }
                }
            }
        default:
            done(nil)
        }
    }

    static func show(in model: Model) {
        model.busy = true
        load { people in
            model.busy = false
            guard let people else {
                model.permissionToast("Contacts", needs: "Contacts")
                return
            }
            let items: [ListItem] = people.enumerated().map { i, p in
                var item = ListItem(
                    title: p.name,
                    subtitle: [p.org, p.emails.first ?? "", p.phones.first ?? ""].filter { !$0.isEmpty }.joined(separator: " · "),
                    icon: "person.crop.circle",
                    arg: String(i)
                )
                var actions: [ItemAction] = []
                if let e = p.emails.first { actions.append(ItemAction(title: "Email \(e)", arg: "mailto:\(e)", key: "cmd+e")) }
                if let ph = p.phones.first {
                    let digits = ph.filter { "+0123456789".contains($0) }
                    actions.append(ItemAction(title: "Call \(ph)", arg: "tel:\(digits)", key: "cmd+shift+return"))
                    actions.append(ItemAction(title: "Message \(ph)", arg: "sms:\(digits)", key: "cmd+m"))
                    actions.append(ItemAction(title: "Copy Phone", arg: "copy:\(ph)", key: "cmd+shift+p"))
                    actions.append(ItemAction(title: "Large Type Number", arg: "large:\(ph)", key: "cmd+l"))
                }
                if let e = p.emails.first { actions.append(ItemAction(title: "Copy Email", arg: "copy:\(e)", key: "cmd+shift+e")) }
                item.actions = actions
                item.copy = p.emails.first ?? p.phones.first ?? p.name
                var md = "# \(p.name)\n\n"
                if !p.org.isEmpty { md += "\(p.org)\n\n" }
                for e in p.emails { md += "- \(e)\n" }
                for ph in p.phones { md += "- \(ph)\n" }
                item.markdown = md
                return item
            }
            model.showLocalList(id: "contacts", title: "Contacts", icon: "person.crop.circle", items: items, action: { i in
                guard people.indices.contains(i) else { return }
                NSWorkspace.shared.open(URL(string: "addressbook://\(people[i].id)")!)
                model.apply(.hide)
            }, itemAction: { _, arg in
                if arg.hasPrefix("large:") {
                    model.handleUi("large-type", args: [String(arg.dropFirst(6))])
                } else if arg.hasPrefix("copy:") {
                    model.copyToClipboard(String(arg.dropFirst(5)), toast: "Copied")
                } else if let url = URL(string: arg) {
                    NSWorkspace.shared.open(url)
                    model.apply(.hide)
                }
            })
        }
    }
}

// MARK: - Reminders

@MainActor
enum RemindersBridge {
    static let store = EKEventStore()

    static func withAccess(_ model: Model, _ then: @escaping @MainActor () -> Void) {
        switch EKEventStore.authorizationStatus(for: .reminder) {
        case .fullAccess:
            then()
        case .notDetermined:
            store.requestFullAccessToReminders { granted, _ in
                Task { @MainActor in
                    if granted { then() } else { denied(model) }
                }
            }
        default:
            denied(model)
        }
    }

    private static func denied(_ model: Model) {
        model.permissionToast("Reminders", needs: "Reminders")
    }

    static func show(in model: Model) {
        withAccess(model) {
            model.busy = true
            let predicate = store.predicateForIncompleteReminders(withDueDateStarting: nil, ending: nil, calendars: nil)
            store.fetchReminders(matching: predicate) { found in
                let list = (found ?? []).sorted { a, b in
                    let da = a.dueDateComponents.flatMap { Calendar.current.date(from: $0) } ?? .distantFuture
                    let db = b.dueDateComponents.flatMap { Calendar.current.date(from: $0) } ?? .distantFuture
                    return da < db
                }
                let rows: [(id: String, title: String, due: Date?, list: String, notes: String)] = list.map {
                    ($0.calendarItemIdentifier, $0.title ?? "Untitled", $0.dueDateComponents.flatMap { Calendar.current.date(from: $0) }, $0.calendar?.title ?? "", $0.notes ?? "")
                }
                Task { @MainActor in
                    model.busy = false
                    guard !rows.isEmpty else {
                        model.flash("No open reminders", error: false, thenHide: false)
                        return
                    }
                    let now = Date()
                    let items: [ListItem] = rows.enumerated().map { i, r in
                        var due = ""
                        if let d = r.due {
                            due = d.formatted(date: .abbreviated, time: .shortened)
                            if d < now { due = "\u{26A0}\u{FE0E} overdue · " + due }
                        }
                        var item = ListItem(
                            title: r.title,
                            subtitle: [due, r.list].filter { !$0.isEmpty }.joined(separator: " · "),
                            icon: "checklist",
                            arg: String(i)
                        )
                        item.actions = [
                            ItemAction(title: "Open in Reminders", arg: "open", key: "cmd+o"),
                            ItemAction(title: "Delete Reminder", arg: "delete", key: "ctrl+x"),
                        ]
                        if !r.notes.isEmpty { item.markdown = "# \(r.title)\n\n\(r.notes)" }
                        return item
                    }
                    model.showLocalList(id: "reminders", title: "Reminders — Return completes", icon: "checklist", items: items, action: { i in
                        guard rows.indices.contains(i), let r = store.calendarItem(withIdentifier: rows[i].id) as? EKReminder else { return }
                        r.isCompleted = true
                        try? store.save(r, commit: true)
                        model.flash("Done: \(rows[i].title)", error: false, thenHide: false)
                        show(in: model)
                    }, itemAction: { i, arg in
                        guard rows.indices.contains(i) else { return }
                        if arg == "open" {
                            NSWorkspace.shared.open(URL(fileURLWithPath: "/System/Applications/Reminders.app"))
                            model.apply(.hide)
                        } else if arg == "delete", let r = store.calendarItem(withIdentifier: rows[i].id) as? EKReminder {
                            try? store.remove(r, commit: true)
                            model.flash("Deleted: \(rows[i].title)", error: false, thenHide: false)
                            show(in: model)
                        }
                    })
                }
            }
        }
    }

    /// `when` is free text ("tomorrow 9am", "friday 5pm", "in 2 hours").
    static func add(title: String, when: String, model: Model) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else {
            model.flash("Type what to be reminded of", error: true, thenHide: false)
            return
        }
        withAccess(model) {
            let r = EKReminder(eventStore: store)
            r.title = title
            r.calendar = store.defaultCalendarForNewReminders()
            var note = ""
            if let date = parseDate(when.isEmpty ? title : when) {
                r.dueDateComponents = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: date)
                r.addAlarm(EKAlarm(absoluteDate: date))
                note = " · " + date.formatted(date: .abbreviated, time: .shortened)
            }
            do {
                try store.save(r, commit: true)
                model.flash("Reminder added\(note)", error: false, thenHide: true)
            } catch {
                model.flash("Could not save the reminder: \(error.localizedDescription)", error: true, thenHide: false)
            }
        }
    }

    static func parseDate(_ text: String) -> Date? {
        guard !text.isEmpty, let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.date.rawValue) else { return nil }
        return detector.firstMatch(in: text, range: NSRange(text.startIndex..., in: text))?.date
    }
}

// MARK: - Dictionary

enum DictionaryLookup {
    /// The system dictionary's entry for `word` (same as Look Up / Dictionary.app).
    static func define(_ word: String) -> String? {
        let w = word.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !w.isEmpty else { return nil }
        let range = CFRange(location: 0, length: (w as NSString).length)
        guard let def = DCSCopyTextDefinition(nil, w as CFString, range)?.takeRetainedValue() else { return nil }
        // Entries run sense after sense; give each numbered sense its own line.
        var text = def as String
        for marker in [" 1 ", " 2 ", " 3 ", " 4 ", " 5 ", " 6 ", " 7 ", " 8 ", " 9 ", " PHRASES ", " DERIVATIVES ", " ORIGIN "] {
            text = text.replacingOccurrences(of: marker, with: "\n\n" + marker.trimmingCharacters(in: .whitespaces) + " ")
        }
        return text
    }
}

// MARK: - Media keys

enum MediaKeys {
    /// NX_KEYTYPE_PLAY / NEXT / PREVIOUS: works with whatever app owns Now Playing.
    static func press(_ action: String) {
        let key: Int32 = switch action {
        case "media-next": 17
        case "media-previous": 18
        default: 16
        }
        for down in [true, false] {
            let flags = NSEvent.ModifierFlags(rawValue: down ? 0xA00 : 0xB00)
            let data1 = Int((key << 16) | ((down ? 0xA : 0xB) << 8))
            let event = NSEvent.otherEvent(
                with: .systemDefined, location: .zero, modifierFlags: flags, timestamp: 0,
                windowNumber: 0, context: nil, subtype: 8, data1: data1, data2: -1
            )
            event?.cgEvent?.post(tap: .cghidEventTap)
        }
    }
}

// MARK: - Large Type

/// Alfred-style Large Type: the text huge and centered on the screen. Any key or click closes it.
@MainActor
final class LargeType {
    static let shared = LargeType()
    private var window: NSWindow?
    private var monitor: Any?

    func show(_ text: String) {
        close()
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, let screen = NSScreen.main else { return }
        let w = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        w.level = .screenSaver
        w.backgroundColor = .clear
        w.isOpaque = false
        w.ignoresMouseEvents = false
        w.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        w.contentView = NSHostingView(rootView: LargeTypeView(text: text) { LargeType.shared.close() })
        w.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        window = w
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .leftMouseDown]) { _ in
            MainActor.assumeIsolated { LargeType.shared.close() }
            return nil
        }
    }

    func close() {
        if let m = monitor { NSEvent.removeMonitor(m) }
        monitor = nil
        window?.orderOut(nil)
        window = nil
    }
}

private struct LargeTypeView: View {
    var text: String
    var close: () -> Void

    var body: some View {
        ZStack {
            Color.black.opacity(0.55).ignoresSafeArea()
            Text(text)
                .font(.system(size: text.count < 12 ? 180 : text.count < 40 ? 110 : 64, weight: .bold, design: .rounded))
                .minimumScaleFactor(0.2)
                .multilineTextAlignment(.center)
                .foregroundStyle(.white)
                .padding(40)
                .background(.black.opacity(0.75), in: RoundedRectangle(cornerRadius: 28, style: .continuous))
                .padding(60)
                .textSelection(.enabled)
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: close)
    }
}

// MARK: - Focus

/// Focus sessions: the `[focus]` apps are hidden at the start and every time
/// they come forward, and `[focus] domains` are closed when they open in the
/// front browser tab, until the timer ends or Stop Focus. Pause lifts the
/// blocks and freezes the timer. A menu bar item shows the countdown.
@MainActor
final class FocusController {
    static let shared = FocusController()
    private(set) var endsAt: Date?
    /// Seconds left while paused.
    private(set) var pausedLeft: TimeInterval?
    private(set) var goal = ""
    private var blocked: Set<String> = []
    private var domains: [String] = []
    private var observer: NSObjectProtocol?
    private var timer: Task<Void, Never>?
    private var tabWatch: Task<Void, Never>?
    private var statusItem: NSStatusItem?
    private var tick: Task<Void, Never>?
    private var menuBar = true

    var isActive: Bool { isPaused || (endsAt.map { $0 > Date() } ?? false) }
    var isPaused: Bool { pausedLeft != nil }

    var minutesLeft: Int {
        let left = pausedLeft ?? endsAt?.timeIntervalSinceNow ?? 0
        return max(0, Int(ceil(left / 60)))
    }

    func start(minutes: Int, apps: [String], domains: [String] = [], goal: String = "", menuBar: Bool = true, model: Model) {
        stop(announce: false)
        let minutes = max(1, min(minutes, 600))
        blocked = Set(apps.map { $0.lowercased() })
        self.domains = domains
        self.goal = goal.trimmingCharacters(in: .whitespacesAndNewlines)
        self.menuBar = menuBar
        run(for: TimeInterval(minutes * 60))
        model.noteContentChanged()
        var parts = ["Focus for \(minutes) min"]
        if !self.goal.isEmpty { parts.append("“\(self.goal)”") }
        if !apps.isEmpty { parts.append("\(apps.count) apps hidden") }
        if !domains.isEmpty { parts.append("\(domains.count) sites blocked") }
        model.flash(parts.joined(separator: " · "), error: false, thenHide: true)
    }

    /// Start (or resume) the blocks and the countdown for `seconds`.
    private func run(for seconds: TimeInterval) {
        pausedLeft = nil
        endsAt = Date().addingTimeInterval(seconds)
        for app in NSWorkspace.shared.runningApplications where isBlocked(app) {
            app.hide()
        }
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { note in
            let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            MainActor.assumeIsolated {
                let f = FocusController.shared
                guard let app, f.isActive, !f.isPaused, f.isBlocked(app) else { return }
                app.hide()
                Model.shared.flash("Focus: \(app.localizedName ?? "That app") is off for \(f.minutesLeft) more min", error: false, thenHide: false)
            }
        }
        timer = Task { @MainActor in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            guard !Task.isCancelled else { return }
            FocusController.shared.stop(announce: true)
        }
        if !domains.isEmpty {
            tabWatch = Task { @MainActor in
                while !Task.isCancelled {
                    FocusController.shared.checkFrontTab()
                    try? await Task.sleep(nanoseconds: 2_000_000_000)
                }
            }
        }
        showStatusItem()
    }

    /// Lift blocks and stop the clock; again to carry on.
    func togglePause(model: Model) {
        if let left = pausedLeft {
            run(for: left)
            model.flash("Focus resumed · \(minutesLeft) min left", error: false, thenHide: true)
            return
        }
        guard let endsAt, endsAt > Date() else {
            model.flash("No focus session running", error: false, thenHide: true)
            return
        }
        let left = endsAt.timeIntervalSinceNow
        clearRun()
        pausedLeft = left
        updateStatusItem()
        model.flash("Focus paused · \(minutesLeft) min left", error: false, thenHide: true)
    }

    private func clearRun() {
        timer?.cancel()
        timer = nil
        tabWatch?.cancel()
        tabWatch = nil
        if let o = observer { NSWorkspace.shared.notificationCenter.removeObserver(o) }
        observer = nil
    }

    func stop(announce: Bool) {
        let was = isActive
        clearRun()
        endsAt = nil
        pausedLeft = nil
        blocked = []
        domains = []
        let doneGoal = goal
        goal = ""
        hideStatusItem()
        if announce, was {
            NSSound(named: "Glass")?.play()
            Model.shared.show()
            Model.shared.flash(doneGoal.isEmpty ? "Focus session done — nice work" : "Focus done: “\(doneGoal)” — nice work", error: false, thenHide: false)
        }
    }

    func isBlocked(_ app: NSRunningApplication) -> Bool {
        guard let name = app.localizedName?.lowercased() else { return false }
        return blocked.contains(name) || blocked.contains(app.bundleIdentifier?.lowercased() ?? "\u{0}")
    }

    // MARK: Sites

    /// Close the front browser tab when it shows a blocked site.
    private func checkFrontTab() {
        guard !isPaused, let app = NSWorkspace.shared.frontmostApplication, let bid = app.bundleIdentifier,
              let family = PageContext.browsers[bid] else { return }
        let target = "application id \"\(bid)\""
        let urlScript: String
        let closeScript: String
        switch family {
        case .safari:
            urlScript = "tell \(target) to return URL of front document"
            closeScript = "tell \(target) to close current tab of front window"
        case .chromium:
            urlScript = "tell \(target) to return URL of active tab of front window"
            closeScript = "tell \(target) to close active tab of front window"
        }
        var err: NSDictionary?
        guard let url = NSAppleScript(source: urlScript)?.executeAndReturnError(&err).stringValue,
              let host = URL(string: url)?.host,
              Self.matches(host: host, domains: domains) else { return }
        _ = NSAppleScript(source: closeScript)?.executeAndReturnError(&err)
        Model.shared.show()
        Model.shared.flash("Focus: \(host) is blocked for \(minutesLeft) more min", error: false, thenHide: true)
    }

    /// Same rule as the engine's `domain_blocked`: exact host or a subdomain.
    static func matches(host: String, domains: [String]) -> Bool {
        var h = host.lowercased()
        if h.hasPrefix("www.") { h.removeFirst(4) }
        return domains.contains { d in
            var d = d.lowercased().trimmingCharacters(in: .whitespaces)
            for p in ["*.", "www."] where d.hasPrefix(p) { d.removeFirst(p.count) }
            return !d.isEmpty && (h == d || h.hasSuffix("." + d))
        }
    }

    // MARK: Menu bar

    private func showStatusItem() {
        guard menuBar else { return }
        if statusItem == nil {
            let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
            let menu = NSMenu()
            let pause = NSMenuItem(title: "Pause / Resume Focus", action: #selector(FocusMenuTarget.pause), keyEquivalent: "")
            pause.target = FocusMenuTarget.shared
            let stop = NSMenuItem(title: "Stop Focus", action: #selector(FocusMenuTarget.stop), keyEquivalent: "")
            stop.target = FocusMenuTarget.shared
            menu.addItem(pause)
            menu.addItem(stop)
            item.menu = menu
            statusItem = item
        }
        updateStatusItem()
        tick?.cancel()
        tick = Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 15_000_000_000)
                FocusController.shared.updateStatusItem()
            }
        }
    }

    private func updateStatusItem() {
        guard let button = statusItem?.button else { return }
        let left = minutesLeft
        let symbol = isPaused ? "pause.fill" : "target"
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "Focus")
        image?.isTemplate = true
        button.image = image
        button.title = "\(left)m"
        button.imagePosition = .imageLeading
        button.toolTip = goal.isEmpty ? "Focus" : "Focus: \(goal)"
    }

    private func hideStatusItem() {
        tick?.cancel()
        tick = nil
        if let item = statusItem { NSStatusBar.system.removeStatusItem(item) }
        statusItem = nil
    }
}

/// Target for the Focus menu bar item's menu.
@MainActor
final class FocusMenuTarget: NSObject {
    static let shared = FocusMenuTarget()

    @objc func pause() {
        FocusController.shared.togglePause(model: Model.shared)
    }

    @objc func stop() {
        FocusController.shared.stop(announce: false)
    }
}
