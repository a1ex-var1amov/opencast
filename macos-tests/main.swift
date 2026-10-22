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

print("\n\(checks - failures)/\(checks) panel logic checks passed")
exit(failures == 0 ? 0 : 1)
