import AppKit
import PDFKit
import Vision

/// Turns files, images and the screen into text for AI chat attachments.
/// Everything runs on-device; nothing is uploaded except the text you send.
enum Attachments {
    /// Most characters taken from one attachment.
    static let maxChars = 60_000

    /// Text of a file: plain text, PDF, or OCR for images.
    static func text(of url: URL) -> String? {
        let ext = url.pathExtension.lowercased()
        if ext == "pdf" {
            guard let doc = PDFDocument(url: url) else { return nil }
            return clip(doc.string ?? "")
        }
        if ["png", "jpg", "jpeg", "heic", "tiff", "tif", "gif", "webp", "bmp"].contains(ext) {
            guard let data = try? Data(contentsOf: url) else { return nil }
            return ocr(data: data).map(clip)
        }
        if ["rtf", "rtfd", "doc", "docx", "odt", "html", "htm", "webarchive"].contains(ext),
           let attr = try? NSAttributedString(url: url, options: [:], documentAttributes: nil)
        {
            return clip(attr.string)
        }
        guard let data = try? Data(contentsOf: url) else { return nil }
        if let s = String(data: data, encoding: .utf8) { return clip(s) }
        if let s = String(data: data, encoding: .isoLatin1), !s.contains("\0") { return clip(s) }
        return nil
    }

    static func ocr(data: Data) -> String? {
        guard let image = NSImage(data: data),
              let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else { return nil }
        return ocr(cgImage: cg)
    }

    static func ocr(cgImage: CGImage) -> String? {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
        guard (try? handler.perform([request])) != nil else { return nil }
        let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
        let text = lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    /// OCR of the main display (needs Screen Recording; uses `screencapture`).
    static func screenText() -> String? {
        let path = NSTemporaryDirectory() + "opencast-attach-\(ProcessInfo.processInfo.processIdentifier).png"
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        p.arguments = ["-x", "-m", path]
        do {
            try p.run()
            p.waitUntilExit()
        } catch {
            return nil
        }
        defer { try? FileManager.default.removeItem(atPath: path) }
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else { return nil }
        return ocr(data: data).map(clip)
    }

    private static func clip(_ s: String) -> String {
        s.count > maxChars ? String(s.prefix(maxChars)) + "\n[truncated]" : s
    }
}
