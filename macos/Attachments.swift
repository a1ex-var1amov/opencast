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
        screenCaptureData().flatMap { ocr(data: $0).map(clip) }
    }

    // MARK: - Pixels for vision models

    /// Longest edge we send: past this a screenshot costs tokens, not detail.
    static let maxImageEdge = 1568
    /// Base64 ceiling per image — must match `MAX_IMAGE_B64` in the engine.
    static let maxImageB64 = 12 * 1024 * 1024

    /// A screenshot of the main display, as PNG data.
    static func screenCaptureData() -> Data? {
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
        return try? Data(contentsOf: URL(fileURLWithPath: path))
    }

    static func imageData(of url: URL) -> (b64: String, mime: String)? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return imageData(data)
    }

    /// Re-encode for a vision model: downscale, then JPEG (or PNG when the image
    /// has transparency, so a cut-out menu keeps it). Sending the raw 5 MB Retina
    /// PNG would blow the size ceiling and the model's image budget alike.
    static func imageData(_ data: Data) -> (b64: String, mime: String)? {
        guard let image = NSImage(data: data),
              let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else { return nil }
        return imageData(cg)
    }

    static func imageData(_ cg: CGImage) -> (b64: String, mime: String)? {
        let longest = max(cg.width, cg.height)
        let scale = Double(longest) > Double(maxImageEdge) ? Double(maxImageEdge) / Double(longest) : 1
        let w = max(1, Int((Double(cg.width) * scale).rounded()))
        let h = max(1, Int((Double(cg.height) * scale).rounded()))
        let keepsAlpha = cg.alphaInfo != .none
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ) else { return nil }
        rep.size = NSSize(width: w, height: h)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        NSGraphicsContext.current?.imageInterpolation = .high
        NSImage(cgImage: cg, size: NSSize(width: w, height: h)).draw(in: NSRect(x: 0, y: 0, width: w, height: h))
        NSGraphicsContext.restoreGraphicsState()
        let out = keepsAlpha
            ? rep.representation(using: .png, properties: [:])
            : rep.representation(using: .jpeg, properties: [.compressionFactor: 0.82])
        guard let bytes = out else { return nil }
        let b64 = bytes.base64EncodedString()
        guard b64.count <= maxImageB64 else { return nil }
        return (b64, keepsAlpha ? "image/png" : "image/jpeg")
    }

    private static func clip(_ s: String) -> String {
        s.count > maxChars ? String(s.prefix(maxChars)) + "\n[truncated]" : s
    }
}
