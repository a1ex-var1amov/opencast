import Foundation

/// What an `opencast://` URL (or a file opened with OpenCast) asks for.
/// Parsing is pure so it can be tested without a panel: `./test-swift.command`.
enum DeepLink: Equatable {
    case search(String)
    case run(String)
    /// Finder › Open With › OpenCast on an Alfred export.
    case importAlfred(String)
    case ignored

    private static let alfredExtensions = ["alfredworkflow", "alfredsnippets"]

    static func parse(_ url: URL) -> DeepLink {
        if url.isFileURL, alfredExtensions.contains(url.pathExtension.lowercased()) {
            return .importAlfred(url.path)
        }
        guard url.scheme?.lowercased() == "opencast" else { return .ignored }
        let host = (url.host ?? "").lowercased()
        switch host {
        case "search":
            let q = URLComponents(url: url, resolvingAgainstBaseURL: false)?
                .queryItems?.first(where: { $0.name == "q" })?.value ?? ""
            return .search(q)
        case "run":
            // opencast://run/builtin:reload — the id may contain colons.
            let id = String(url.path.drop(while: { $0 == "/" }))
            return id.isEmpty ? .ignored : .run(decode(id))
        default:
            // Also accept opencast:///run/<id> (some mail clients eat the host).
            guard url.path.hasPrefix("/run/") else { return .ignored }
            let id = String(url.path.dropFirst("/run/".count))
            return id.isEmpty ? .ignored : .run(decode(id))
        }
    }

    private static func decode(_ s: String) -> String { s.removingPercentEncoding ?? s }
}
