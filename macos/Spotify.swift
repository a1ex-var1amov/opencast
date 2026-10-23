import AppKit

/// Spotify, asked directly instead of through the media keys. Media keys follow
/// whatever owns Now Playing — a browser tab counts — and read nothing back, so
/// "Next Track" can skip a YouTube video and "what is playing?" has no answer.
/// These commands always reach Spotify and can report the track.
enum Spotify {
    static let bundle = "com.spotify.client"

    static var installed: Bool { NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) != nil }
    static var running: Bool { NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == bundle } }

    /// A script either answers or says why it could not; silence is never success.
    private struct Answer {
        var text: String?
        var problem: String?
    }

    /// One script, run and reported. Apple Events need the user's OK the first time
    /// (the automation prompt); a refusal arrives here as a failure, not a crash.
    private static func ask(_ source: String) -> Answer {
        var info: NSDictionary?
        guard let script = NSAppleScript(source: source) else { return Answer(problem: "could not read the Spotify script") }
        let out = script.executeAndReturnError(&info)
        if let info {
            let detail = (info[NSAppleScript.errorMessage] as? String) ?? "\(info)"
            return Answer(problem: detail.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        let text = out.stringValue ?? ""
        return Answer(text: text.isEmpty ? nil : text)
    }

    private static func tell(_ body: String) -> String {
        "tell application id \"com.spotify.client\"\n" + body + "\nend tell"
    }

    /// Everything the list and "Now Playing" show, in one round trip. The `try`
    /// covers a player with no track loaded yet, which errors on every track field.
    static func state() -> SpotifyTrack {
        guard running else { return SpotifyTrack() }
        let src = tell("""
        	set sep to character id 31
        	try
        		return (player state as string) & sep & (artist of current track) & sep & (name of current track) & sep & (album of current track) & sep & (shuffling as string) & sep & (repeating as string) & sep & (sound volume as string) & sep & (player position as string) & sep & (duration of current track as string)
        	on error
        		return (player state as string) & sep & sep & sep & sep & (shuffling as string) & sep & (repeating as string) & sep & (sound volume as string) & sep & (player position as string)
        	end try
        """)
        return SpotifyTrack.parse(ask(src).text ?? "")
    }

    /// Runs one control verb. Spotify starts if it was not up — except `open`, which
    /// also brings it forward. Returns an error string when it could not be done.
    /// No shuffle or repeat: this Spotify takes `set shuffling`/`set repeating` and
    /// ignores them, and its menu has no such item to press instead.
    @discardableResult
    static func control(_ verb: String) -> String? {
        guard installed else { return "Spotify is not installed" }
        // Multi-line if blocks: AppleScript's one-line `if … then … else …` has to
        // stay on one line, and a wrapped one fails to compile.
        let body: String? = switch verb {
        case "play": "play"
        case "pause": "pause"
        case "toggle": "\tif player state is playing then\n\t\tpause\n\telse\n\t\tplay\n\tend if"
        case "next": "next track"
        case "previous": "previous track"
        case "seekfwd": "\tset p to player position\n\tset d to (duration of current track) / 1000\n\tif p + 10 > d then\n\t\tset player position to d - 1\n\telse\n\t\tset player position to p + 10\n\tend if"
        case "seekback": "\tset p to player position\n\tif p < 10 then\n\t\tset player position to 0\n\telse\n\t\tset player position to p - 10\n\tend if"
        case "volup": "\tset v to sound volume\n\tif v > 90 then\n\t\tset sound volume to 100\n\telse\n\t\tset sound volume to v + 10\n\tend if"
        case "voldn": "\tset v to sound volume\n\tif v < 10 then\n\t\tset sound volume to 0\n\telse\n\t\tset sound volume to v - 10\n\tend if"
        case "open": "activate"
        default: nil
        }
        guard let body else { return "Spotify: nothing to do for \(verb)" }
        return ask(tell(body)).problem
    }
}
