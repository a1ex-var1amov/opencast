// Panel logic tests: compiled by ./test-swift.command together with the pure
// files they cover (macos/DeepLink.swift, MeetingLinks.swift, SnippetMatch.swift).
// No XCTest on purpose — swiftc can build this without an Xcode project, so CI
// can run it in seconds.
import Foundation

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

func url(_ s: String) -> URL { URL(string: s)! }

// MARK: - Deep links

check(DeepLink.parse(url("opencast://search?q=coffee")) == .search("coffee"), "search?q=coffee")
check(DeepLink.parse(url("opencast://search")) == .search(""), "search with no query clears the field")
check(DeepLink.parse(url("opencast://search?q=")) == .search(""), "search?q= is empty, not missing")
check(DeepLink.parse(url("opencast://search?q=a%20b%20c")) == .search("a b c"), "query is percent-decoded")
check(DeepLink.parse(url("opencast://search/spot")) == .search("spot"), "search/spot path form")
check(DeepLink.parse(url("opencast://run/builtin:reload")) == .run("builtin:reload"), "run/ keeps colons in the id")
check(DeepLink.parse(url("opencast://run/ai%3Afix-grammar")) == .run("ai:fix-grammar"), "run/ decodes an encoded colon")
check(DeepLink.parse(url("opencast:///run/builtin:reload")) == .run("builtin:reload"), "run/ also works without a host")
check(DeepLink.parse(url("opencast://run/")) == .ignored, "run/ with no id does nothing")
check(DeepLink.parse(url("opencast://frob/x")) == .ignored, "unknown host does nothing")
check(DeepLink.parse(url("https://example.com/run/x")) == .ignored, "another scheme is not ours")
check(DeepLink.parse(url("file:///tmp/Wf.alfredworkflow")) == .importAlfred("/tmp/Wf.alfredworkflow"), "Alfred workflow import")
check(DeepLink.parse(url("file:///tmp/Notes.alfredsnippets")) == .importAlfred("/tmp/Notes.alfredsnippets"), "Alfred snippets import")
check(DeepLink.parse(url("file:///tmp/random.txt")) == .ignored, "opening a plain file does nothing")

// MARK: - Meeting links

check(MeetingLinks.firstURL(in: "Standup at https://meet.google.com/abc-def-ghi today")?.host == "meet.google.com",
      "Google Meet link in notes")
check(MeetingLinks.firstURL(in: "https://acme.zoom.us/j/12345")?.host == "acme.zoom.us", "zoom subdomain")
check(MeetingLinks.firstURL(in: "see https://zoom.us.evil.example/j/1") == nil,
      "a look-alike host is not a meeting link")
check(MeetingLinks.firstURL(in: "no links in this text") == nil, "no link")
check(MeetingLinks.firstURL(in: "https://example.com/x") == nil, "unknown host")
check(MeetingLinks.firstURL(in: [nil, nil, "call me on https://meet.jit.si/opencast"])?.host == "meet.jit.si",
      "fields are searched in order, nils skipped")
check(MeetingLinks.firstURL(in: ["location room 2", "notes: https://teams.microsoft.com/l/meetup-join/9"])?.host
      == "teams.microsoft.com", "link found in a later field")

// MARK: - Snippet matching

let keys = [";a", ";ia", ";sig"]
check(SnippetMatch.longest("hi ;ia", keywords: keys) == ";ia", "longest keyword wins (;ia over ;a)")
check(SnippetMatch.longest("hello ;sig", keywords: keys) == ";sig", "match at the end of the buffer")
check(SnippetMatch.longest(";a", keywords: keys) == ";a", "exact short match")
check(SnippetMatch.longest("signature", keywords: keys) == nil, "no keyword, no expansion")
check(SnippetMatch.longest("", keywords: keys) == nil, "empty buffer")
check(SnippetMatch.longest("x", keywords: [""]) == nil, "an empty keyword never matches")
check(SnippetMatch.trim("hello ;sig", longestKeyword: 4) == ";sig", "trim keeps a matchable tail")
check(SnippetMatch.trim("ab", longestKeyword: 0) == "ab", "no keywords: nothing to trim")
check(SnippetMatch.trim("abcdefghij", longestKeyword: 3) == "hij", "long typing streams stay short")
check(SnippetMatch.trim("abcde", longestKeyword: 4) == "abcde", "under the limit: untouched")

// MARK: - Space labels

check(SpaceLabels.visibleApps(["Safari", "Window Server", "Safari", "Dock"]) == ["Safari"],
      "menu-bar noise and duplicates drop out")
check(SpaceLabels.summary(["Safari", "Mail"], fullscreen: false) == "Safari, Mail", "apps on a desktop")
check(SpaceLabels.summary([], fullscreen: false) == "Empty", "an empty desktop says so")
check(SpaceLabels.summary([], fullscreen: true) == "", "a full-screen tile needs no summary")
check(SpaceLabels.summary(["Window Server", "OpenCast"], fullscreen: false) == "Empty", "noise only reads as empty")
check(SpaceLabels.summary(["A", "B", "C", "D", "E", "F"], fullscreen: false) == "A, B, C, D +2",
      "long lists are capped with a count")

// MARK: - List filtering (typing inside a panel-side list)

check(ListFilter.score(title: "New Browser Window", subtitle: nil, hotkey: nil, query: "nbrw") != nil,
      "word starts carry a subsequence: nbrw → New Browser Window")
check(ListFilter.score(title: "New Tab", subtitle: nil, hotkey: nil, query: "zzz") == nil,
      "letters that are not there match nothing")
check(ListFilter.score(title: "New Tab", subtitle: "Shell", hotkey: "⌘T", query: "⌘t") != nil,
      "the key equivalent is searchable too")
check(ListFilter.order(["Safari", "Safari Services", "Notes"], query: "saf").map(\.item)
      == ["Safari", "Safari Services"], "prefix first, non-matches dropped")
check(ListFilter.order(["New Tab", "Note"], query: "nt").map(\.item) == ["New Tab", "Note"],
      "word starts beat a tighter gap")
check(ListFilter.order(["Print", "Sub Print"], query: "print").map(\.item) == ["Print", "Sub Print"],
      "a title match outranks a subtitle match")
check(ListFilter.order(["ax", "bx"], query: "x").map(\.item) == ["ax", "bx"],
      "equal scores keep the order the list came in")
check(ListFilter.order(["b", "a"], query: "").map(\.item) == ["b", "a"], "no query keeps everything in order")
check(ListFilter.order(["Safari", "Notes"], query: "notes").map(\.index) == [1], "indices point at the original rows")

// MARK: - Which apps the shortcut drill-down offers

let apps = ShortcutApps.rows([
    ShortcutAppEntry(pid: 1, name: "Safari"),
    ShortcutAppEntry(pid: 2, name: "Ghostty"),
    ShortcutAppEntry(pid: 3, name: "telegram"),
], front: 2)
check(apps.map(\.name) == ["Ghostty", "Safari", "telegram"], "the app you were in leads, the rest are A→Z")
let twins = ShortcutApps.rows([
    ShortcutAppEntry(pid: 1, name: "Ghostty"),
    ShortcutAppEntry(pid: 7, name: "Ghostty"),
], front: 7)
check(twins.count == 1 && twins[0].pid == 7, "two instances, one row: the one you were in")
check(ShortcutApps.rows([
    ShortcutAppEntry(pid: 1, name: "Safari"),
    ShortcutAppEntry(pid: 2, name: "SAFARI"),
], front: nil).count == 1, "name case does not make two rows")
check(ShortcutApps.rows([], front: nil).isEmpty, "no running apps, no rows")

// MARK: - What Spotify reports, and what the Spotify list offers

let sep = SpotifyTrack.sep
let loaded = SpotifyTrack.parse("playing\(sep)The Weeknd\(sep)Blinding Lights\(sep)After Hours\(sep)true\(sep)false\(sep)60")
check(loaded.playing && loaded.artist == "The Weeknd" && loaded.track == "Blinding Lights", "a loaded player parses")
check(loaded.shuffle && !loaded.repeatOn && loaded.volume == 60, "booleans and volume come through")
check(loaded.line == "Blinding Lights — The Weeknd · After Hours", "the track line reads like a now-playing strip")
check(loaded.facts == "Playing · shuffle on · volume 60", "the facts line says what the player is doing")

let bare = SpotifyTrack.parse("paused")
check(bare.running && !bare.playing && bare.line.isEmpty, "no track loaded is not no player")
check(bare.facts == "Paused" && bare.volume == -1, "missing fields stay missing instead of zero")
check(SpotifyTrack.parse("").state.isEmpty && !SpotifyTrack.parse("").running, "an empty answer means not running")
check(SpotifyTrack.parse("stopped").line.isEmpty && !SpotifyTrack.parse("stopped").running, "stopped reads as stopped")

let playingRows = SpotifyTrack.rows(for: loaded)
check(playingRows[0].title == "Pause" && playingRows[0].verb == "pause", "a playing list starts with Pause")
check(playingRows[1].verb == "next" && playingRows[2].verb == "previous", "skip and back follow right behind")
check(playingRows.contains(where: { $0.title == "Volume Up" && $0.subtitle == "now 60" }), "volume rows say where the volume is")
check(!playingRows.contains(where: { $0.title.contains("Shuffle") || $0.title.contains("Repeat") }),
      "no shuffle or repeat row: Spotify ignores those writes, so the launcher does not promise them")
check(playingRows.contains(where: { $0.title == "Seek Forward 10s" && $0.subtitle.isEmpty }),
      "seek rows appear without a position to report")

let mid = SpotifyTrack.parse("playing\(sep)x\(sep)y\(sep)z\(sep)false\(sep)false\(sep)40\(sep)112.5\(sep)221000")
check(mid.progress == "1:52 / 3:41", "the progress line reads like a player strip")
check(mid.facts == "Playing · 1:52 / 3:41 · volume 40", "progress sits between the state and the volume")
check(SpotifyTrack.parse("playing\(sep)x\(sep)y\(sep)z\(sep)false\(sep)false\(sep)40\(sep)112,5").position == 112.5,
      "a comma decimal mark still parses")
check(SpotifyTrack.rows(for: mid).contains(where: { $0.title == "Seek Back 10s" && $0.subtitle == "1:52 / 3:41" }),
      "seek rows show where in the song you are")

check(SpotifyTrack.rows(for: bare)[0].title == "Play", "a paused list starts with Play")
check(SpotifyTrack.rows(for: SpotifyTrack()).contains(where: { $0.title == "Volume Up" && $0.subtitle == "+10" }),
      "with no volume read, the row just promises +10")

let withArt = SpotifyTrack.parse("playing\(sep)a\(sep)b\(sep)c\(sep)false\(sep)false\(sep)50\(sep)1\(sep)1000\(sep)https://i.scdn.co/image/ab")
check(withArt.artworkURL == "https://i.scdn.co/image/ab", "artwork url is the last field when present")
check(SpotifyTrack.parse("playing\(sep)a\(sep)b").artworkURL.isEmpty, "older payloads without art stay empty")

// Rows as the engine sends them. Unknown kinds decode as .unknown instead of
// vanishing; missing kind still drops the row.
let resource = Command(["id": "mcp:gh:res:issues", "title": "Open issues", "kind": "mcpresource", "icon": "🔌"])
check(resource?.kind == .mcpResource, "an MCP resource row decodes")
check(Command(["id": "mcp:gh:prompt:sum", "title": "Summarize", "kind": "mcpprompt"])?.kind == .mcpPrompt, "an MCP prompt row decodes")
check(Command(["id": "x", "title": "Y"]) == nil, "a row with no kind string is dropped")
let brandnew = Command(["id": "ext:foo", "title": "Foo", "kind": "brandnew"])
check(brandnew?.kind == .unknown, "a future engine kind decodes as unknown")
check(brandnew?.subtitle == "kind: brandnew", "unknown kind gets a kind: subtitle when empty")

let appRow = Command([
    "id": "app:/Applications/Spotify.app", "title": "Spotify", "kind": "app",
    "actions": [
        ["id": "spotify-play", "title": "Play / Pause"],
        ["id": "spotify-search", "title": "Search in Spotify…", "key": "cmd+f"],
    ],
])
check(appRow?.actions.count == 2, "integration verbs arrive on the app row")
check(appRow?.actions.first?.title == "Play / Pause", "the verb keeps its own label")
check(appRow?.actions.last?.key == "cmd+f", "and its key, when it has one")
check(Command(["id": "builtin:reload", "title": "Reload Commands", "kind": "builtin"])?.actions.isEmpty == true,
      "a row with no integration carries no verbs")

// MARK: - Window target subtitle (Move Left shows what would move)

check(WindowTarget.label(windowTitle: "README.md", appName: "Cursor") == "README.md · Cursor",
      "window title and app when they differ")
check(WindowTarget.label(windowTitle: "Safari", appName: "Safari") == "Safari",
      "no duplicate when title equals app")
check(WindowTarget.label(windowTitle: "safari", appName: "Safari") == "safari",
      "case-insensitive same name stays a single token")
check(WindowTarget.label(windowTitle: "", appName: "Mail") == "Mail",
      "AX title missing falls back to app name")
check(WindowTarget.label(windowTitle: "Inbox", appName: "") == "Inbox",
      "app name missing still shows the window title")
check(WindowTarget.label(windowTitle: "  ", appName: "  ") == nil,
      "blank title and app means no subtitle")
check(WindowTarget.label(windowTitle: "", appName: "") == nil, "both empty means no subtitle")

let move = Command(id: "window:move-left", title: "Move Left", subtitle: nil, icon: "⇤",
                   kind: .window, arguments: [], hotkey: nil)
let other = Command(id: "builtin:reload", title: "Reload Commands", subtitle: nil, icon: "↻",
                    kind: .builtin, arguments: [], hotkey: nil)
let tagged = WindowTarget.tag([move, other], label: "Notes · TextEdit")
check(tagged[0].subtitle == "Notes · TextEdit", "window rows get the live target subtitle")
check(tagged[1].subtitle == nil, "non-window rows stay untouched")
check(WindowTarget.tag([move], label: nil)[0].subtitle == nil, "nil label leaves subtitles alone")
check(WindowTarget.tag([move], label: "")[0].subtitle == nil, "empty label leaves subtitles alone")
let custom = Command(id: "window:custom:foo", title: "My Snap", subtitle: "Window", icon: "▣",
                     kind: .window, arguments: [], hotkey: nil)
check(WindowTarget.tag([custom], label: "Slack")[0].subtitle == "Slack",
      "live target replaces a generic Window subtitle")

// MARK: - Hotkey chord (Use ⌘Space button)

check(HotkeyChord.isCmdSpace("Cmd+Space"), "Cmd+Space is Spotlight's chord")
check(HotkeyChord.isCmdSpace("Command+Space"), "Command+Space is the same chord")
check(HotkeyChord.isCmdSpace("cmd+space"), "case does not matter")
check(!HotkeyChord.isCmdSpace("Alt+Space"), "Alt+Space is not Spotlight")
check(!HotkeyChord.isCmdSpace("Cmd+Shift+Space"), "extra modifiers are not bare ⌘Space")
check(!HotkeyChord.isCmdSpace("Cmd+S"), "not Space")
check(!HotkeyChord.isCmdSpace(""), "empty is not ⌘Space")

// MARK: - Menu bar tint / click policy (Tahoe dark bar + GG visibility)

check(!MenuBarSymbol.forcesAppLabelTint,
      "status items must not force app .labelColor (goes black on dark menu bars)")
check(!MenuBarSymbol.usesTaskAfterMenu,
      "menu actions must not use Task { @MainActor } after AppKit menu hops (Tahoe drops them)")
check(MenuBarSymbol.needsMonochrome("sparkles"), "sparkles ships multicolor and needs monochrome")
check(!MenuBarSymbol.needsMonochrome("circle"), "circle is already a mono template")
check(!MenuBarSymbol.needsMonochrome("circle.fill"), "circle.fill is already mono")
check(MenuBarSymbol.fallbacks(for: "sparkles").contains("star.fill"),
      "sparkles has a fallback when the symbol is missing on an OS")
let sparkles = MenuBarSymbol.templateImage(systemName: "sparkles", accessibilityDescription: "OpenCast")
check(sparkles.isTemplate, "menu-bar sparkles is a template for system tint")
check(sparkles.size.width > 0 && sparkles.size.height > 0, "menu-bar sparkles has a non-zero size")
let stay = MenuBarSymbol.templateImage(systemName: "circle", accessibilityDescription: "Stay Awake")
check(stay.isTemplate && stay.size.width > 0, "Stay Awake circle is a visible template")

// MARK: - Overlay / Click Text panel policy (Tahoe invisible graphics)

check(OverlayPanelPolicy.orderFrontRegardless,
      "Click Text / HUD overlays must orderFrontRegardless while accessory")
check(OverlayPanelPolicy.backingAlpha > 0 && OverlayPanelPolicy.backingAlpha < 0.01,
      "overlay backing keeps a hair of alpha so SwiftUI fills composite on Tahoe")

// MARK: - Toast key semantics (N-1, N-2) — Model lives in AppKit; verify by hand
// Scenario 1: permission error toast up → type "slack" → Return opens Slack (not Settings).
// Scenario 2: "Copied" toast up → Esc dismisses toast and also clears/hides as usual (one keypress).
// Scenario 3: "Thinking…" during a stream → Esc stops the stream (toast dismiss is a side effect).

// MARK: - Builtin icons map to Glyphs.byEmoji (P13-1)

do {
    let src = (try? String(contentsOfFile: "crates/opencast-core/src/engine.rs", encoding: .utf8)) ?? ""
    let pattern = #"Builtin::\w+,\s*"[a-z0-9-]+",\s*"[^"]*",\s*"([^"]*)""#
    let re = try! NSRegularExpression(pattern: pattern)
    let ns = src as NSString
    var offenders: [String] = []
    var matches = 0
    re.enumerateMatches(in: src, range: NSRange(location: 0, length: ns.length)) { match, _, _ in
        guard let match, match.numberOfRanges > 1 else { return }
        matches += 1
        let icon = ns.substring(with: match.range(at: 1))
        let key = String(icon.unicodeScalars.filter { $0.value != 0xfe0f && $0.value != 0xfe0e })
        let bareSymbol = key.count > 1 && key.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.contains($0) || $0 == "."
        }
        if !bareSymbol, Glyphs.byEmoji[key] == nil {
            offenders.append(icon.isEmpty ? "(empty)" : icon)
        }
    }
    check(matches >= 50, "glyph guard found the BUILTINS table in engine.rs (\(matches))")
    let uniq = Array(Set(offenders)).sorted()
    check(uniq.isEmpty, "every BUILTINS icon is in Glyphs.byEmoji or a bare SF Symbol name")
    if !uniq.isEmpty {
        print("  offenders: \(uniq.joined(separator: ", "))")
    }
}

print("\n\(checks - failures)/\(checks) panel logic checks passed")
exit(failures == 0 ? 0 : 1)
