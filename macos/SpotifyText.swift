import Foundation

/// What Spotify reports back, parsed from the one-line script result. Kept free of
/// AppKit so the parsing and the wording get tested like any other pure logic.
struct SpotifyTrack {
    var state = ""
    var artist = ""
    var track = ""
    var album = ""
    var shuffle = false
    var repeatOn = false
    var volume = -1
    var position = -1.0
    var length = -1.0
    /// `artwork url of current track` when Spotify returns one (http or file).
    var artworkURL = ""

    /// Unit separator: no song title contains it, so the split can never lie.
    static let sep = "\u{1F}"

    var running: Bool { !state.isEmpty && state != "stopped" }
    var playing: Bool { state == "playing" }

    /// "Blinding Lights — The Weeknd · After Hours", empty when nothing is loaded.
    var line: String {
        guard !track.isEmpty else { return "" }
        var out = track
        if !artist.isEmpty { out += " — \(artist)" }
        if !album.isEmpty { out += " · \(album)" }
        return out
    }

    /// "Playing · 1:52 / 3:41 · shuffle on · volume 60"
    var facts: String {
        var bits: [String] = []
        if !state.isEmpty {
            bits.append(state == "playing" ? "Playing" : state == "paused" ? "Paused" : state.capitalized)
        }
        if let progress { bits.append(progress) }
        if shuffle { bits.append("shuffle on") }
        if repeatOn { bits.append("repeat on") }
        if volume >= 0 { bits.append("volume \(volume)") }
        return bits.joined(separator: " · ")
    }

    /// "1:52 / 3:41" once both ends of the track are known.
    var progress: String? {
        guard position >= 0, length > 0 else { return nil }
        return "\(clock(position)) / \(clock(length / 1000))"
    }

    private func clock(_ seconds: Double) -> String {
        // Players count up in whole seconds and drop the remainder, so do the same.
        let whole = Int(seconds)
        return "\(whole / 60):" + String(format: "%02d", whole % 60)
    }

    /// Fields arrive in a fixed order after the player state, booleans as "true".
    /// AppleScript writes numbers with the user's decimal mark, so accept both.
    static func parse(_ raw: String) -> SpotifyTrack {
        var out = SpotifyTrack()
        let parts = raw.components(separatedBy: sep).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        guard let first = parts.first, !first.isEmpty else { return out }
        out.state = first
        let number = { (text: String) -> Double? in Double(text.replacingOccurrences(of: ",", with: ".")) }
        if parts.count > 1 { out.artist = parts[1] }
        if parts.count > 2 { out.track = parts[2] }
        if parts.count > 3 { out.album = parts[3] }
        if parts.count > 4 { out.shuffle = parts[4] == "true" }
        if parts.count > 5 { out.repeatOn = parts[5] == "true" }
        if parts.count > 6 { out.volume = Int(number(parts[6]) ?? -1) }
        if parts.count > 7 { out.position = number(parts[7]) ?? -1 }
        if parts.count > 8 { out.length = number(parts[8]) ?? -1 }
        if parts.count > 9 { out.artworkURL = parts[9] }
        return out
    }

    /// Rows for the Spotify list, in the order people reach for them. The first row
    /// is the opposite of the current state, so the list always starts with the key.
    /// No shuffle or repeat rows: Spotify takes the AppleScript write and ignores it,
    /// and its own menu has no such item — so the launcher does not pretend.
    static func rows(for t: SpotifyTrack) -> [(title: String, verb: String, subtitle: String)] {
        var rows: [(String, String, String)] = []
        rows.append(t.playing
            ? ("Pause", "pause", t.facts)
            : ("Play", "play", t.facts.isEmpty ? "starts Spotify if it is not up" : t.facts))
        rows.append(("Next Track", "next", ""))
        rows.append(("Previous Track", "previous", ""))
        rows.append(("Seek Back 10s", "seekback", t.progress ?? ""))
        rows.append(("Seek Forward 10s", "seekfwd", t.progress ?? ""))
        rows.append(("Volume Up", "volup", t.volume >= 0 ? "now \(t.volume)" : "+10"))
        rows.append(("Volume Down", "voldn", t.volume >= 0 ? "now \(t.volume)" : "-10"))
        // No "Now Playing" row: the list header is already the strip (and copies).
        rows.append(("Open Spotify", "open", ""))
        return rows
    }
}
