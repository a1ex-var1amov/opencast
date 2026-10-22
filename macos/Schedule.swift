import AppKit
import EventKit

/// "My Schedule": today's and tomorrow's events from the Calendar database
/// (EventKit, on-device), with one-key "join" for video meetings.
@MainActor
final class Schedule {
    static let shared = Schedule()

    private let store = EKEventStore()
    private(set) var upcoming: [EKEvent] = []
    private var lastRefresh = Date.distantPast

    var authorized: Bool {
        EKEventStore.authorizationStatus(for: .event) == .fullAccess
    }

    var notDetermined: Bool {
        EKEventStore.authorizationStatus(for: .event) == .notDetermined
    }

    func requestAccess(_ done: @escaping @MainActor (Bool) -> Void) {
        store.requestFullAccessToEvents { granted, _ in
            Task { @MainActor in
                if granted { Schedule.shared.refresh(force: true) }
                done(granted)
            }
        }
    }

    /// Events from now until the end of tomorrow. Cheap enough to call on every show.
    func refresh(force: Bool = false) {
        guard authorized else { return }
        guard force || Date().timeIntervalSince(lastRefresh) > 60 else { return }
        lastRefresh = Date()
        let cal = Calendar.current
        let start = cal.startOfDay(for: Date())
        guard let end = cal.date(byAdding: .day, value: 2, to: start) else { return }
        let predicate = store.predicateForEvents(withStart: start, end: end, calendars: nil)
        upcoming = store.events(matching: predicate)
            .filter { !$0.isAllDay && $0.endDate > Date() && $0.status != .canceled }
            .sorted { $0.startDate < $1.startDate }
    }

    /// The event that is on now or starts within `minutes`.
    func next(within minutes: Double = 60) -> EKEvent? {
        upcoming.first { $0.startDate.timeIntervalSinceNow < minutes * 60 }
    }

    func listItems() -> [ListItem] {
        upcoming.enumerated().map { index, e in
            var subtitle = Self.timeRange(e)
            if let loc = e.location, !loc.isEmpty, Self.meetingURL(e) == nil { subtitle += " · \(loc)" }
            if Self.meetingURL(e) != nil { subtitle += " · Return to join" }
            return ListItem(title: e.title ?? "Untitled", subtitle: subtitle, icon: Self.meetingURL(e) != nil ? "🎥" : "📅", arg: String(index))
        }
    }

    func open(index: Int) {
        guard upcoming.indices.contains(index) else { return }
        open(upcoming[index])
    }

    func open(_ e: EKEvent) {
        if let url = Self.meetingURL(e) {
            NSWorkspace.shared.open(url)
        } else if let id = e.calendarItemExternalIdentifier?.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed),
                  let url = URL(string: "ical://ekevent/\(id)?method=show&options=more")
        {
            NSWorkspace.shared.open(url)
        } else {
            NSWorkspace.shared.open(URL(fileURLWithPath: "/System/Applications/Calendar.app"))
        }
    }

    static func timeRange(_ e: EKEvent) -> String {
        let f = DateFormatter()
        f.timeStyle = .short
        f.dateStyle = .none
        let day = Calendar.current.isDateInToday(e.startDate) ? "" : "Tomorrow "
        if e.startDate <= Date() { return "Now · until \(f.string(from: e.endDate))" }
        let mins = Int(e.startDate.timeIntervalSinceNow / 60)
        let soon = mins < 60 ? " (in \(max(mins, 1)) min)" : ""
        return "\(day)\(f.string(from: e.startDate))–\(f.string(from: e.endDate))\(soon)"
    }

    /// Zoom, Google Meet, Teams, Webex, Around, Whereby… links in the URL, location or notes.
    static func meetingURL(_ e: EKEvent) -> URL? {
        let hosts = ["zoom.us", "meet.google.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com", "around.co", "chime.aws", "meet.jit.si", "facetime.apple.com"]
        var candidates: [String] = []
        if let u = e.url?.absoluteString { candidates.append(u) }
        if let l = e.location { candidates.append(l) }
        if let n = e.notes { candidates.append(n) }
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return nil }
        for text in candidates {
            let ns = text as NSString
            for m in detector.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
                guard let url = m.url, let host = url.host?.lowercased() else { continue }
                if hosts.contains(where: { host == $0 || host.hasSuffix("." + $0) }) { return url }
            }
        }
        return nil
    }
}
