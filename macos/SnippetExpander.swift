import AppKit

/// Text expansion in any app: type a snippet keyword (`;sig`) and it is
/// replaced by the snippet text. Only keywords that start with a symbol expand
/// by default (see `auto_expand` in config), so normal words are never touched.
///
/// Needs Accessibility (global key monitoring). Password fields are never
/// seen: macOS hides Secure Input from monitors.
@MainActor
final class SnippetExpander {
    static let shared = SnippetExpander()

    private var monitor: Any?
    private var mouseMonitor: Any?
    private var buffer = ""
    /// keyword → text
    private var snippets: [String: String] = [:]
    private var longest = 0
    private var expanding = false

    func reload() {
        Task.detached {
            let raw = Engine.call(["op": "snippets"])["snippets"] as? [[String: Any]] ?? []
            var map: [String: String] = [:]
            for s in raw {
                if let k = s["keyword"] as? String, let t = s["text"] as? String { map[k] = t }
            }
            let loaded = map
            await MainActor.run {
                let me = SnippetExpander.shared
                me.snippets = loaded
                me.longest = loaded.keys.map(\.count).max() ?? 0
                if !me.loaded { me.install() }
            }
        }
    }

    private var loaded: Bool { monitor != nil }

    private func install() {
        guard monitor == nil, !snippets.isEmpty else { return }
        monitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { event in
            MainActor.assumeIsolated { SnippetExpander.shared.handle(event) }
        }
        mouseMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { _ in
            MainActor.assumeIsolated { SnippetExpander.shared.buffer = "" }
        }
    }

    private func handle(_ event: NSEvent) {
        guard !expanding, !snippets.isEmpty else { return }
        let mods = event.modifierFlags.intersection([.command, .control, .option])
        // Shortcuts, arrows, Return, Tab, Delete… start a new word.
        guard mods.isEmpty || mods == .option, let chars = event.characters, !chars.isEmpty,
              !chars.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) || $0.value >= 0xF700 })
        else {
            buffer = ""
            return
        }
        buffer += chars
        if buffer.count > max(longest, 1) * 2 { buffer = String(buffer.suffix(longest)) }
        for (keyword, text) in snippets where buffer.hasSuffix(keyword) {
            buffer = ""
            expand(keyword: keyword, text: text)
            return
        }
    }

    private func expand(keyword: String, text: String) {
        expanding = true
        Task.detached {
            let r = Engine.call(["op": "expandSnippet", "text": text])
            let expanded = r["text"] as? String ?? text
            let back = r["back"] as? Int ?? 0
            await MainActor.run {
                SnippetExpander.shared.replace(count: keyword.count, with: expanded, cursorBack: back)
            }
        }
    }

    /// Delete the typed keyword, paste the text, restore the clipboard.
    private func replace(count: Int, with text: String, cursorBack: Int) {
        let board = NSPasteboard.general
        let saved = board.pasteboardItems?.compactMap { item -> NSPasteboardItem? in
            let copy = NSPasteboardItem()
            for t in item.types {
                if let d = item.data(forType: t) { copy.setData(d, forType: t) }
            }
            return copy
        } ?? []
        Self.press(key: 51, times: count) // Delete
        board.clearContents()
        board.setString(text, forType: .string)
        // Mark it transient so clipboard history skips it.
        board.setData(Data(), forType: NSPasteboard.PasteboardType("org.nspasteboard.TransientType"))
        Self.press(key: 9, times: 1, flags: .maskCommand) // ⌘V
        Self.press(key: 123, times: cursorBack) // ←
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 400_000_000)
            board.clearContents()
            if !saved.isEmpty { board.writeObjects(saved) }
            SnippetExpander.shared.expanding = false
        }
    }

    private static func press(key: CGKeyCode, times: Int, flags: CGEventFlags = []) {
        guard times > 0 else { return }
        let src = CGEventSource(stateID: .combinedSessionState)
        for _ in 0..<min(times, 2000) {
            let down = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: true)
            let up = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: false)
            down?.flags = flags
            up?.flags = flags
            down?.post(tap: .cghidEventTap)
            up?.post(tap: .cghidEventTap)
        }
    }
}
