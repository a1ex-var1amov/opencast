import AppKit
import Quartz
import SwiftUI

/// What the preview pane shows for the selected row.
struct PreviewData: Equatable {
    var id: String
    var title: String
    var text: String?
    var image: NSImage?
    var path: String?
    var facts: [(String, String)] = []

    static func == (a: PreviewData, b: PreviewData) -> Bool {
        a.id == b.id && a.title == b.title && a.text == b.text && a.path == b.path && (a.image == nil) == (b.image == nil)
    }
}

enum PreviewLoader {
    /// Kinds that get a pane (apps, builtins and window commands keep the full-width list).
    static func wants(_ c: Command) -> Bool {
        switch c.kind {
        case .file, .clipboard, .snippet, .ai, .script, .mcp, .quicklink: return true
        default: return c.id.hasPrefix("note:") || c.id.hasPrefix("chat:")
        }
    }

    /// Off the main thread: file contents, Rust previews.
    static func load(_ c: Command) async -> PreviewData? {
        if c.kind == .file {
            let path = String(c.id.dropFirst("file:".count))
            return file(path: path, id: c.id, title: c.title)
        }
        let raw = Engine.call(["op": "preview", "id": c.id])
        guard let p = raw["preview"] as? [String: Any] else { return nil }
        let facts = (p["facts"] as? [[Any]] ?? []).compactMap { pair -> (String, String)? in
            guard pair.count == 2, let k = pair[0] as? String, let v = pair[1] as? String else { return nil }
            return (k, v)
        }
        return PreviewData(
            id: c.id,
            title: p["title"] as? String ?? c.title,
            text: p["text"] as? String,
            image: nil,
            path: p["path"] as? String,
            facts: facts
        )
    }

    static func file(path: String, id: String, title: String) -> PreviewData {
        let url = URL(fileURLWithPath: path)
        let values = try? url.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey, .isDirectoryKey])
        var facts: [(String, String)] = []
        if let size = values?.fileSize {
            facts.append(("Size", ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)))
        }
        if let date = values?.contentModificationDate {
            facts.append(("Modified", date.formatted(date: .abbreviated, time: .shortened)))
        }
        facts.append(("Where", (path as NSString).deletingLastPathComponent.replacingOccurrences(of: NSHomeDirectory(), with: "~")))
        let ext = url.pathExtension.lowercased()
        if ["png", "jpg", "jpeg", "gif", "heic", "tiff", "tif", "webp", "bmp", "pdf"].contains(ext), let img = NSImage(contentsOf: url) {
            return PreviewData(id: id, title: title, text: nil, image: img, path: path, facts: facts)
        }
        if values?.isDirectory == true {
            let items = (try? FileManager.default.contentsOfDirectory(atPath: path))?.filter { !$0.hasPrefix(".") }.sorted().prefix(40) ?? []
            return PreviewData(id: id, title: title, text: items.joined(separator: "\n"), image: nil, path: path, facts: facts)
        }
        if let handle = try? FileHandle(forReadingFrom: url) {
            let data = (try? handle.read(upToCount: 6000)) ?? Data()
            try? handle.close()
            if let s = String(data: data, encoding: .utf8), !s.contains("\0") {
                return PreviewData(id: id, title: title, text: s, image: nil, path: path, facts: facts)
            }
        }
        return PreviewData(id: id, title: title, text: nil, image: NSWorkspace.shared.icon(forFile: path), path: path, facts: facts)
    }
}

struct PreviewPane: View {
    var data: PreviewData
    var scheme: ColorScheme

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(data.title)
                .font(.system(size: 13, weight: .semibold))
                .lineLimit(2)
            if let image = data.image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .frame(maxWidth: .infinity, maxHeight: 220)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            if let text = data.text, !text.isEmpty {
                ScrollView {
                    MarkdownText(text: text, scheme: scheme)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            Spacer(minLength: 0)
            if !data.facts.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(Array(data.facts.enumerated()), id: \.offset) { _, f in
                        HStack(alignment: .top) {
                            Text(f.0).foregroundStyle(Theme.tertiary(scheme))
                            Spacer(minLength: 8)
                            Text(f.1).foregroundStyle(Theme.secondary(scheme)).lineLimit(2).multilineTextAlignment(.trailing)
                        }
                    }
                }
                .font(.system(size: 11))
            }
        }
        .padding(12)
        .frame(width: 290)
        .frame(maxHeight: .infinity, alignment: .top)
    }

}

/// Small markdown renderer for the detail pane: headings, bullets, quotes,
/// fenced code and inline styling (bold, italic, code, links). SwiftUI's
/// `Text(AttributedString)` drops block structure, so blocks are laid out here.
struct MarkdownText: View {
    var text: String
    var scheme: ColorScheme

    private enum Block: Hashable {
        case heading(Int, String)
        case bullet(String)
        case quote(String)
        case code(String)
        case paragraph(String)
        case rule
    }

    private var blocks: [Block] {
        var out: [Block] = []
        var code: [String]?
        var para: [String] = []
        func flush() {
            if !para.isEmpty { out.append(.paragraph(para.joined(separator: "\n"))) }
            para = []
        }
        for line in text.components(separatedBy: "\n") {
            let t = line.trimmingCharacters(in: .whitespaces)
            if t.hasPrefix("```") {
                if let c = code {
                    out.append(.code(c.joined(separator: "\n")))
                    code = nil
                } else {
                    flush()
                    code = []
                }
                continue
            }
            if code != nil {
                code?.append(line)
                continue
            }
            if t.isEmpty {
                flush()
            } else if t == "---" || t == "***" {
                flush()
                out.append(.rule)
            } else if let hashes = t.firstIndex(where: { $0 != "#" }), t.hasPrefix("#"), t[hashes] == " " {
                flush()
                out.append(.heading(t.distance(from: t.startIndex, to: hashes), String(t[hashes...]).trimmingCharacters(in: .whitespaces)))
            } else if t.hasPrefix("- ") || t.hasPrefix("* ") || t.hasPrefix("• ") {
                flush()
                out.append(.bullet(String(t.dropFirst(2))))
            } else if t.hasPrefix("> ") {
                flush()
                out.append(.quote(String(t.dropFirst(2))))
            } else {
                para.append(line)
            }
        }
        if let c = code { out.append(.code(c.joined(separator: "\n"))) }
        flush()
        return out
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .heading(let level, let s):
                    Text(Self.inline(s))
                        .font(.system(size: level <= 1 ? 16 : level == 2 ? 14 : 13, weight: .semibold))
                case .bullet(let s):
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("•").foregroundStyle(Theme.tertiary(scheme))
                        Text(Self.inline(s))
                    }
                    .font(.system(size: 12))
                case .quote(let s):
                    Text(Self.inline(s))
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.secondary(scheme))
                        .padding(.leading, 8)
                        .overlay(alignment: .leading) { Rectangle().fill(Theme.stroke(scheme)).frame(width: 2) }
                case .code(let s):
                    Text(s)
                        .font(.system(size: 11, design: .monospaced))
                        .padding(8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                case .paragraph(let s):
                    Text(Self.inline(s)).font(.system(size: 12))
                case .rule:
                    Rectangle().fill(Theme.stroke(scheme)).frame(height: 0.5)
                }
            }
        }
        .textSelection(.enabled)
    }

    static func inline(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(s)
    }
}

/// Space-bar style Quick Look for files, apps and clipboard images (⌘Y).
@MainActor
final class QuickLook: NSObject, QLPreviewPanelDataSource {
    static let shared = QuickLook()
    private var urls: [URL] = []

    func show(_ urls: [URL]) {
        guard !urls.isEmpty, let panel = QLPreviewPanel.shared() else { return }
        self.urls = urls
        Model.shared.keepOnBlurOnce = true
        panel.dataSource = self
        panel.reloadData()
        if panel.isVisible {
            panel.orderOut(nil)
        } else {
            panel.makeKeyAndOrderFront(nil)
            panel.level = NSWindow.Level(rawValue: 26) // above the launcher
        }
    }

    nonisolated func numberOfPreviewItems(in panel: QLPreviewPanel!) -> Int {
        MainActor.assumeIsolated { urls.count }
    }

    nonisolated func previewPanel(_ panel: QLPreviewPanel!, previewItemAt index: Int) -> QLPreviewItem! {
        MainActor.assumeIsolated { urls[index] as NSURL }
    }
}
