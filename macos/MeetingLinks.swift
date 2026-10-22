import Foundation

/// Finding a video-meeting link in free text (an event's URL, location or
/// notes). Pure so it can be tested without EventKit: `./test-swift.command`.
enum MeetingLinks {
    static let hosts = [
        "zoom.us", "meet.google.com", "teams.microsoft.com", "teams.live.com",
        "webex.com", "whereby.com", "around.co", "chime.aws", "meet.jit.si",
        "facetime.apple.com",
    ]

    /// The first link in `text` on a known meeting host.
    static func firstURL(in text: String) -> URL? {
        guard !text.isEmpty,
              let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)
        else { return nil }
        let ns = text as NSString
        for m in detector.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            guard let url = m.url, let host = url.host?.lowercased() else { continue }
            if hosts.contains(where: { host == $0 || host.hasSuffix("." + $0) }) {
                return url
            }
        }
        return nil
    }

    /// The first meeting link across several fields, in order.
    static func firstURL(in fields: [String?]) -> URL? {
        for case let f? in fields {
            if let u = firstURL(in: f) { return u }
        }
        return nil
    }
}
