import AppKit
import CryptoKit
import Foundation
import Vision

/// Clipboard + screenshot history (Raycast/Alfred-style): originals, thumbs, OCR, pin/delete.
@MainActor
final class ClipboardHistory {
    static let shared = ClipboardHistory()

    enum Kind: String, Codable {
        case text, image, file, shot
    }

    struct Entry: Codable {
        let id: String
        let kind: Kind
        var text: String?
        var mediaFile: String?
        var thumbFile: String?
        var filePath: String?
        var ocrText: String?
        var contentHash: String
        var pinned: Bool
        let at: Date

        enum CodingKeys: String, CodingKey {
            case id, kind, text, mediaFile, thumbFile, filePath, ocrText, contentHash, pinned, at
            case imageFile // legacy
        }

        init(
            id: String, kind: Kind, text: String?, mediaFile: String?, thumbFile: String?,
            filePath: String?, ocrText: String?, contentHash: String, pinned: Bool, at: Date
        ) {
            self.id = id
            self.kind = kind
            self.text = text
            self.mediaFile = mediaFile
            self.thumbFile = thumbFile
            self.filePath = filePath
            self.ocrText = ocrText
            self.contentHash = contentHash
            self.pinned = pinned
            self.at = at
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            kind = try c.decode(Kind.self, forKey: .kind)
            text = try c.decodeIfPresent(String.self, forKey: .text)
            mediaFile = try c.decodeIfPresent(String.self, forKey: .mediaFile)
                ?? c.decodeIfPresent(String.self, forKey: .imageFile)
            thumbFile = try c.decodeIfPresent(String.self, forKey: .thumbFile)
            filePath = try c.decodeIfPresent(String.self, forKey: .filePath)
            ocrText = try c.decodeIfPresent(String.self, forKey: .ocrText)
            contentHash = try c.decodeIfPresent(String.self, forKey: .contentHash) ?? id
            pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
            at = try c.decodeIfPresent(Date.self, forKey: .at) ?? Date()
        }

        func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(id, forKey: .id)
            try c.encode(kind, forKey: .kind)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(mediaFile, forKey: .mediaFile)
            try c.encodeIfPresent(thumbFile, forKey: .thumbFile)
            try c.encodeIfPresent(filePath, forKey: .filePath)
            try c.encodeIfPresent(ocrText, forKey: .ocrText)
            try c.encode(contentHash, forKey: .contentHash)
            try c.encode(pinned, forKey: .pinned)
            try c.encode(at, forKey: .at)
        }
    }

    private var entries: [Entry] = []
    private var changeCount = -1
    private var timer: Timer?
    private var shotTimer: Timer?
    private var storeURL: URL?
    private var mediaDir: URL?
    private var thumbDir: URL?
    private var thumbCache: [String: NSImage] = [:]
    private var saveTask: Task<Void, Never>?
    private var knownShotPaths: Set<String> = []
    private let maxEntries = 200
    private let maxChars = 8_000
    private let retentionDays = 7

    private static let ignoreBundles: Set<String> = [
        "com.1password.1password",
        "com.1password.1password-launcher",
        "com.agilebits.onepassword7",
        "com.bitwarden.desktop",
        "com.lastpass.LastPass",
        "com.apple.keychainaccess",
        "com.apple.Passwords",
    ]

    func install() {
        guard timer == nil else { return }
        Task.detached {
            let path = (Engine.call(["op": "configDir"])["path"] as? String)
                ?? (NSHomeDirectory() + "/.config/opencast")
            await MainActor.run {
                let root = URL(fileURLWithPath: path)
                let shared = ClipboardHistory.shared
                shared.storeURL = root.appendingPathComponent("clipboard.json")
                shared.mediaDir = root.appendingPathComponent("clipboard-media")
                shared.thumbDir = root.appendingPathComponent("clipboard-thumbs")
                for dir in [shared.mediaDir, shared.thumbDir].compactMap({ $0 }) {
                    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                }
                shared.load()
                shared.pruneRetention()
                shared.poll()
                shared.scanScreenshots(initial: true)
                shared.timer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: true) { _ in
                    MainActor.assumeIsolated { ClipboardHistory.shared.poll() }
                }
                shared.shotTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { _ in
                    MainActor.assumeIsolated { ClipboardHistory.shared.scanScreenshots(initial: false) }
                }
            }
        }
    }

    func shutdown() {
        timer?.invalidate()
        timer = nil
        shotTimer?.invalidate()
        shotTimer = nil
        saveTask?.cancel()
        saveNow()
    }

    func text(for id: String) -> String? {
        entries.first { $0.id == id }?.text
    }

    func ocrText(for id: String) -> String? {
        entries.first { $0.id == id }?.ocrText
    }

    /// Row thumbnail (small). Falls back to full image once.
    func image(for id: String) -> NSImage? {
        if let cached = thumbCache[id] { return cached }
        guard let e = entries.first(where: { $0.id == id }) else { return nil }
        if let name = e.thumbFile, let dir = thumbDir,
           let img = NSImage(contentsOf: dir.appendingPathComponent(name))
        {
            thumbCache[id] = img
            return img
        }
        if let path = e.filePath, e.kind == .shot || e.kind == .file,
           let img = Self.makeThumb(from: URL(fileURLWithPath: path))
        {
            thumbCache[id] = img
            return img
        }
        if let name = e.mediaFile, let dir = mediaDir,
           let img = Self.makeThumb(from: dir.appendingPathComponent(name))
        {
            thumbCache[id] = img
            return img
        }
        return nil
    }

    func restoreToPasteboard(_ id: String, plainText: Bool = false) -> Bool {
        guard let e = entries.first(where: { $0.id == id }) else { return false }
        let board = NSPasteboard.general
        board.clearContents()
        switch e.kind {
        case .text:
            guard let text = e.text else { return false }
            return board.setString(text, forType: .string)
        case .image:
            if plainText, let ocr = e.ocrText, !ocr.isEmpty {
                return board.setString(ocr, forType: .string)
            }
            if let name = e.mediaFile, let dir = mediaDir {
                let url = dir.appendingPathComponent(name)
                if let data = try? Data(contentsOf: url) {
                    let type: NSPasteboard.PasteboardType = name.hasSuffix(".png") ? .png : .tiff
                    return board.setData(data, forType: type)
                }
            }
            return false
        case .file, .shot:
            guard let path = e.filePath else { return false }
            let url = URL(fileURLWithPath: path)
            if plainText {
                return board.setString(path, forType: .string)
            }
            if let data = try? Data(contentsOf: url),
               ["png", "jpg", "jpeg", "tif", "tiff", "gif", "webp"].contains(url.pathExtension.lowercased())
            {
                let type: NSPasteboard.PasteboardType =
                    url.pathExtension.lowercased() == "png" ? .png : .tiff
                if type == .png || url.pathExtension.lowercased().hasPrefix("tif") {
                    return board.setData(data, forType: type)
                }
                if let img = NSImage(data: data), let tiff = img.tiffRepresentation {
                    return board.setData(tiff, forType: .tiff)
                }
            }
            board.writeObjects([url as NSURL])
            return true
        }
    }

    func copyOnly(_ id: String) -> Bool {
        restoreToPasteboard(id, plainText: false)
    }

    @discardableResult
    func togglePin(_ id: String) -> Bool? {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { return nil }
        entries[i].pinned.toggle()
        let pinned = entries[i].pinned
        // Pinned float to top among peers.
        entries.sort { a, b in
            if a.pinned != b.pinned { return a.pinned && !b.pinned }
            return a.at > b.at
        }
        scheduleSave()
        return pinned
    }

    func delete(_ id: String) {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { return }
        removeFiles(entries[i])
        entries.remove(at: i)
        thumbCache.removeValue(forKey: id)
        scheduleSave()
    }

    func deleteAll(keepPinned: Bool = true) {
        let kept = keepPinned ? entries.filter(\.pinned) : []
        for e in entries where !(keepPinned && e.pinned) {
            removeFiles(e)
            thumbCache.removeValue(forKey: e.id)
        }
        entries = kept
        scheduleSave()
    }

    func pasteLatestScreenshot() -> String? {
        let shot = entries.first { $0.kind == .shot && $0.filePath != nil }
        guard let shot, restoreToPasteboard(shot.id) else { return nil }
        return shot.id
    }

    static func isBrowseQuery(_ query: String) -> Bool {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return q == "clip" || q.hasPrefix("clip ")
            || q == "clipboard" || q.hasPrefix("clipboard ")
            || q == "cb" || q.hasPrefix("cb ")
            || q == "paste"
    }

    static func isShotsQuery(_ query: String) -> Bool {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return q == "shots" || q.hasPrefix("shots ")
            || q == "screenshots" || q.hasPrefix("screenshots ")
            || q == "screenshot" || q.hasPrefix("screenshot ")
    }

    func merge(into commands: [Command], query: String) -> [Command] {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let browsingClip = Self.isBrowseQuery(q)
        let browsingShots = Self.isShotsQuery(q)
        let needle: String = {
            if q.hasPrefix("clipboard") {
                return String(q.dropFirst("clipboard".count)).trimmingCharacters(in: .whitespaces)
            }
            if q.hasPrefix("screenshots") {
                return String(q.dropFirst("screenshots".count)).trimmingCharacters(in: .whitespaces)
            }
            if q.hasPrefix("screenshot") {
                return String(q.dropFirst("screenshot".count)).trimmingCharacters(in: .whitespaces)
            }
            if q.hasPrefix("shots") {
                return String(q.dropFirst(5)).trimmingCharacters(in: .whitespaces)
            }
            if q.hasPrefix("clip") {
                return String(q.dropFirst(4)).trimmingCharacters(in: .whitespaces)
            }
            if q.hasPrefix("cb") {
                return String(q.dropFirst(2)).trimmingCharacters(in: .whitespaces)
            }
            if q == "paste" { return "" }
            return (browsingClip || browsingShots) ? "" : q
        }()
        guard browsingClip || browsingShots || !needle.isEmpty else { return commands }

        var rows: [Command] = []
        for e in entries {
            if browsingShots, e.kind != .shot { continue }
            if browsingClip, e.kind == .shot { continue }
            if !browsingClip, !browsingShots, e.kind == .shot {
                // Root search: include shots only when needle hits OCR/name.
            }

            let title: String
            let subtitle: String
            let hay: String
            switch e.kind {
            case .text:
                let oneLine = (e.text ?? "")
                    .replacingOccurrences(of: "\n", with: " ")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                guard !oneLine.isEmpty else { continue }
                hay = oneLine.lowercased() + " " + (e.ocrText ?? "").lowercased()
                if !needle.isEmpty, !hay.contains(needle) { continue }
                title = oneLine.count > 72 ? String(oneLine.prefix(69)) + "…" : oneLine
                subtitle = "Clipboard · \(ago(e.at))" + (e.pinned ? " · pinned" : "")
            case .image:
                hay = ((e.ocrText ?? "") + " image png img").lowercased()
                if !needle.isEmpty, !hay.contains(needle) { continue }
                let ocrHint = (e.ocrText?.trimmingCharacters(in: .whitespacesAndNewlines)).flatMap {
                    $0.isEmpty ? nil : ($0.count > 40 ? String($0.prefix(37)) + "…" : $0)
                }
                title = ocrHint ?? "Image"
                subtitle = "Clipboard · image · \(ago(e.at))" + (e.pinned ? " · pinned" : "")
            case .file:
                let name = (e.filePath as NSString?)?.lastPathComponent ?? "File"
                hay = (name + " " + (e.filePath ?? "") + " " + (e.ocrText ?? "")).lowercased()
                if !needle.isEmpty, !hay.contains(needle) { continue }
                title = name
                subtitle = "Clipboard · file · \(ago(e.at))" + (e.pinned ? " · pinned" : "")
            case .shot:
                let name = (e.filePath as NSString?)?.lastPathComponent ?? "Screenshot"
                hay = (name + " " + (e.ocrText ?? "")).lowercased()
                if !needle.isEmpty, !hay.contains(needle) { continue }
                title = name
                subtitle = "Screenshot · \(ago(e.at))" + (e.pinned ? " · pinned" : "")
            }
            rows.append(Command(
                id: e.id,
                title: title,
                subtitle: subtitle,
                icon: e.kind == .text ? "📋" : "🖼",
                kind: .clipboard,
                arguments: [],
                hotkey: nil
            ))
            if rows.count >= 40 { break }
        }
        if rows.isEmpty { return commands }
        var seen = Set(rows.map(\.id))
        let rest = commands.filter { seen.insert($0.id).inserted }
        return rows + rest
    }

    // MARK: - Capture

    private func poll() {
        let board = NSPasteboard.general
        let count = board.changeCount
        guard count != changeCount else { return }
        changeCount = count

        if let bid = NSWorkspace.shared.frontmostApplication?.bundleIdentifier,
           Self.ignoreBundles.contains(bid)
        {
            return
        }

        // Copied files
        if let urls = board.readObjects(forClasses: [NSURL.self], options: [
            .urlReadingFileURLsOnly: true,
        ]) as? [URL], !urls.isEmpty {
            let path = urls[0].path
            let hash = Self.hashString(path)
            if entries.contains(where: { $0.contentHash == hash }) { return }
            let id = "clip:\(UUID().uuidString)"
            push(Entry(
                id: id, kind: .file, text: nil, mediaFile: nil, thumbFile: nil,
                filePath: path, ocrText: nil, contentHash: hash, pinned: false, at: Date()
            ))
            return
        }

        if let raw = board.string(forType: .string) {
            let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                guard text.count <= maxChars else { return }
                let hash = Self.hashString(text)
                if entries.contains(where: { $0.contentHash == hash }) { return }
                push(Entry(
                    id: "clip:\(UUID().uuidString)", kind: .text, text: text,
                    mediaFile: nil, thumbFile: nil, filePath: nil, ocrText: nil,
                    contentHash: hash, pinned: false, at: Date()
                ))
                return
            }
        }

        let png = board.data(forType: .png)
        let tiff = board.data(forType: .tiff)
        guard let data = png ?? tiff, let mediaDir, let thumbDir else { return }
        let hash = Self.hashData(data)
        if entries.contains(where: { $0.contentHash == hash }) { return }
        let id = "clip:\(UUID().uuidString)"
        let ext = png != nil ? "png" : "tiff"
        let mediaName = "\(id.dropFirst(5)).\(ext)"
        let thumbName = "\(id.dropFirst(5)).jpg"
        let mediaURL = mediaDir.appendingPathComponent(mediaName)
        let thumbURL = thumbDir.appendingPathComponent(thumbName)
        Task.detached {
            try? data.write(to: mediaURL)
            let thumb = Self.makeThumb(from: data)
            if let thumb, let tiff = thumb.tiffRepresentation,
               let rep = NSBitmapImageRep(data: tiff),
               let jpg = rep.representation(using: .jpeg, properties: [.compressionFactor: 0.7])
            {
                try? jpg.write(to: thumbURL)
            }
            let ocr = Self.ocr(data: data)
            await MainActor.run {
                ClipboardHistory.shared.push(Entry(
                    id: id, kind: .image, text: nil, mediaFile: mediaName, thumbFile: thumbName,
                    filePath: nil, ocrText: ocr, contentHash: hash, pinned: false, at: Date()
                ))
            }
        }
    }

    private func scanScreenshots(initial: Bool) {
        let dirs = Self.screenshotDirs()
        let fm = FileManager.default
        var found: [URL] = []
        for dir in dirs {
            guard let items = try? fm.contentsOfDirectory(
                at: dir,
                includingPropertiesForKeys: [.contentModificationDateKey, .isRegularFileKey],
                options: [.skipsHiddenFiles]
            ) else { continue }
            for url in items {
                let ext = url.pathExtension.lowercased()
                guard ["png", "jpg", "jpeg", "heic"].contains(ext) else { continue }
                let name = url.lastPathComponent.lowercased()
                // Prefer screenshot-like names unless on Desktop we only take Screen Shot / Screenshot
                let looksShot = name.contains("screenshot") || name.contains("screen shot")
                    || name.contains("cleanshot")
                if dir.lastPathComponent == "Desktop", !looksShot { continue }
                found.append(url)
            }
        }
        found.sort {
            let a = (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
            let b = (try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
            return a > b
        }
        if initial {
            knownShotPaths = Set(found.prefix(80).map(\.path))
            // Index newest 30 as shots if not already present.
            for url in found.prefix(30) {
                ingestShot(url, skipIfKnown: false)
            }
            return
        }
        for url in found.prefix(40) {
            if knownShotPaths.contains(url.path) { continue }
            knownShotPaths.insert(url.path)
            ingestShot(url, skipIfKnown: true)
        }
    }

    private func ingestShot(_ url: URL, skipIfKnown: Bool) {
        let path = url.path
        let hash = Self.hashString("shot:" + path)
        if skipIfKnown, entries.contains(where: { $0.contentHash == hash || $0.filePath == path }) {
            return
        }
        if entries.contains(where: { $0.filePath == path }) { return }
        let id = "shot:\(UUID().uuidString)"
        let thumbName = "\(id.dropFirst(5)).jpg"
        guard let thumbDir else { return }
        let thumbURL = thumbDir.appendingPathComponent(thumbName)
        let at = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? Date()
        Task.detached {
            let thumb = Self.makeThumb(from: url)
            if let thumb, let tiff = thumb.tiffRepresentation,
               let rep = NSBitmapImageRep(data: tiff),
               let jpg = rep.representation(using: .jpeg, properties: [.compressionFactor: 0.7])
            {
                try? jpg.write(to: thumbURL)
            }
            let data = try? Data(contentsOf: url)
            let ocr = data.flatMap { Self.ocr(data: $0) }
            await MainActor.run {
                ClipboardHistory.shared.push(Entry(
                    id: id, kind: .shot, text: nil, mediaFile: nil, thumbFile: thumbName,
                    filePath: path, ocrText: ocr, contentHash: hash, pinned: false, at: at
                ))
            }
        }
    }

    private static func screenshotDirs() -> [URL] {
        var dirs: [URL] = [URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Desktop")]
        let loc = UserDefaults(suiteName: "com.apple.screencapture")?.string(forKey: "location")
        if let loc, !loc.isEmpty {
            dirs.insert(URL(fileURLWithPath: (loc as NSString).expandingTildeInPath), at: 0)
        }
        return dirs
    }

    // MARK: - Persist

    private func push(_ entry: Entry) {
        entries.removeAll { $0.contentHash == entry.contentHash && !$0.pinned }
        entries.insert(entry, at: 0)
        entries.sort { a, b in
            if a.pinned != b.pinned { return a.pinned && !b.pinned }
            return a.at > b.at
        }
        while entries.count > maxEntries {
            if let idx = entries.lastIndex(where: { !$0.pinned }) {
                removeFiles(entries[idx])
                thumbCache.removeValue(forKey: entries[idx].id)
                entries.remove(at: idx)
            } else {
                break
            }
        }
        scheduleSave()
    }

    private func pruneRetention() {
        let cutoff = Date().addingTimeInterval(-Double(retentionDays) * 86400)
        entries.removeAll { e in
            guard !e.pinned, e.at < cutoff else { return false }
            removeFiles(e)
            thumbCache.removeValue(forKey: e.id)
            return true
        }
        scheduleSave()
    }

    private func removeFiles(_ e: Entry) {
        if let name = e.mediaFile, let dir = mediaDir {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent(name))
        }
        if let name = e.thumbFile, let dir = thumbDir {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent(name))
        }
    }

    private func load() {
        guard let url = storeURL,
              let data = try? Data(contentsOf: url),
              let decoded = try? JSONDecoder().decode([Entry].self, from: data)
        else { return }
        entries = Array(decoded.prefix(maxEntries))
        knownShotPaths = Set(entries.compactMap { $0.kind == .shot ? $0.filePath : nil })
    }

    private func scheduleSave() {
        saveTask?.cancel()
        saveTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled else { return }
            saveNow()
        }
    }

    private func saveNow() {
        guard let url = storeURL else { return }
        let snapshot = entries
        Task.detached {
            guard let data = try? JSONEncoder().encode(snapshot) else { return }
            try? data.write(to: url, options: .atomic)
        }
    }

    private func ago(_ at: Date) -> String {
        let mins = Int(Date().timeIntervalSince(at) / 60)
        if mins < 1 { return "just now" }
        if mins < 60 { return "\(mins)m ago" }
        let hours = mins / 60
        if hours < 48 { return "\(hours)h ago" }
        return "\(hours / 24)d ago"
    }

    // MARK: - Image helpers (off-main OK)

    nonisolated private static func hashString(_ s: String) -> String {
        SHA256.hash(data: Data(s.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    nonisolated private static func hashData(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    nonisolated private static func makeThumb(from url: URL) -> NSImage? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return makeThumb(from: data)
    }

    nonisolated private static func makeThumb(from data: Data) -> NSImage? {
        guard let src = NSImage(data: data) else { return nil }
        let maxEdge: CGFloat = 128
        let size = src.size
        guard size.width > 0, size.height > 0 else { return src }
        let scale = min(1, maxEdge / max(size.width, size.height))
        let target = NSSize(width: size.width * scale, height: size.height * scale)
        let out = NSImage(size: target)
        out.lockFocus()
        src.draw(
            in: NSRect(origin: .zero, size: target),
            from: NSRect(origin: .zero, size: size),
            operation: .copy,
            fraction: 1
        )
        out.unlockFocus()
        return out
    }

    nonisolated private static func ocr(data: Data) -> String? {
        guard let img = NSImage(data: data),
              let tiff = img.tiffRepresentation,
              let rep = NSBitmapImageRep(data: tiff),
              let cg = rep.cgImage
        else { return nil }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .fast
        request.usesLanguageCorrection = false
        let handler = VNImageRequestHandler(cgImage: cg, options: [:])
        do {
            try handler.perform([request])
            let text = (request.results ?? [])
                .compactMap { $0.topCandidates(1).first?.string }
                .joined(separator: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return text.isEmpty ? nil : text
        } catch {
            return nil
        }
    }
}
