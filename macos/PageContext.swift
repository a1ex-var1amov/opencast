import AppKit

/// The page open in the front browser: URL, title and readable text, for
/// "Ask About This Page" / "Summarize This Page".
///
/// The text comes from the browser itself when it allows JavaScript from
/// Apple Events (Safari: Develop › Allow JavaScript from Apple Events;
/// Chrome-family: View › Developer › Allow JavaScript from Apple Events),
/// otherwise the engine downloads the URL and strips the markup.
struct BrowserPage: Sendable {
    var browser: String
    var url: String
    var title: String
    var text: String
    /// True when the text came from the open tab (logged-in pages work).
    var fromTab: Bool
}

enum PageContext {
    enum Family { case safari, chromium }

    static let browsers: [String: Family] = [
        "com.apple.Safari": .safari,
        "com.apple.SafariTechnologyPreview": .safari,
        "com.google.Chrome": .chromium,
        "com.google.Chrome.canary": .chromium,
        "company.thebrowser.Browser": .chromium, // Arc
        "com.brave.Browser": .chromium,
        "com.microsoft.edgemac": .chromium,
        "com.vivaldi.Vivaldi": .chromium,
        "org.chromium.Chromium": .chromium,
        "com.operasoftware.Opera": .chromium,
    ]

    /// The browser to read: the app the panel covered if it is a browser,
    /// else the first running supported browser.
    @MainActor
    static func target(previous: NSRunningApplication?) -> NSRunningApplication? {
        if let p = previous, let b = p.bundleIdentifier, browsers[b] != nil { return p }
        return NSWorkspace.shared.runningApplications.first { app in
            guard let b = app.bundleIdentifier else { return false }
            return browsers[b] != nil
        }
    }

    /// Read the front tab with AppleScript (in-process; NSAppleScript wants the main thread).
    /// `text` is empty when the browser does not allow JavaScript from Apple Events.
    @MainActor
    static func readTab(bundleId: String, name: String) -> Result<BrowserPage, PageError> {
        guard let family = browsers[bundleId] else { return .failure(.unsupported(name)) }
        let app = "application id \"\(bundleId)\""
        let info: String
        let js: String
        switch family {
        case .safari:
            info = "tell \(app) to return (URL of front document) & linefeed & (name of front document)"
            js = "tell \(app) to do JavaScript \"document.body.innerText\" in front document"
        case .chromium:
            info = "tell \(app) to return (URL of active tab of front window) & linefeed & (title of active tab of front window)"
            js = "tell \(app) to execute active tab of front window javascript \"document.body.innerText\""
        }
        guard let head = run(info) else { return .failure(.noAccess(name)) }
        let parts = head.split(separator: "\n", maxSplits: 1).map(String.init)
        let url = parts.first ?? ""
        let title = parts.count > 1 ? parts[1] : ""
        guard !url.isEmpty, url != "missing value" else { return .failure(.noPage(name)) }
        let text = run(js) ?? ""
        let has = !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        return .success(BrowserPage(browser: name, url: url, title: title, text: has ? String(text.prefix(60_000)) : "", fromTab: has))
    }

    /// No JavaScript permission: download the URL (public pages only). Off the main thread.
    static func fetch(_ page: BrowserPage) -> Result<BrowserPage, PageError> {
        let r = Engine.call(["op": "fetchPage", "url": page.url])
        guard let text = r["text"] as? String, !text.isEmpty else {
            return .failure(.fetch(r["error"] as? String ?? "the page has no text"))
        }
        var p = page
        p.text = text
        if let t = r["title"] as? String, !t.isEmpty { p.title = t }
        return .success(p)
    }

    @MainActor
    private static func run(_ source: String) -> String? {
        var err: NSDictionary?
        guard let out = NSAppleScript(source: source)?.executeAndReturnError(&err), err == nil else { return nil }
        return out.stringValue
    }

    enum PageError: Error, CustomStringConvertible {
        case unsupported(String)
        case noAccess(String)
        case noPage(String)
        case fetch(String)

        var description: String {
            switch self {
            case .unsupported(let n): "\(n) can't share its page (Safari, Chrome, Arc, Brave, Edge and Vivaldi can)"
            case .noAccess(let n): "Allow OpenCast to control \(n): System Settings › Privacy & Security › Automation"
            case .noPage(let n): "\(n) has no page open"
            case .fetch(let e): "Couldn't read the page: \(e)"
            }
        }
    }
}

extension Model {
    /// `page` UI action: args[0] = ask | summarize, args[1] = question.
    func askAboutPage(mode: String, question: String) {
        guard let app = PageContext.target(previous: previousApp), let bid = app.bundleIdentifier else {
            flash("Open a page in Safari, Chrome, Arc, Brave, Edge or Vivaldi first", error: true, thenHide: false)
            return
        }
        let name = app.localizedName ?? "the browser"
        flash("Reading \(name)…", error: false, thenHide: false)
        let tab = PageContext.readTab(bundleId: bid, name: name)
        Task.detached {
            let result: Result<BrowserPage, PageContext.PageError>
            switch tab {
            case .success(let p) where p.text.isEmpty: result = PageContext.fetch(p)
            default: result = tab
            }
            await MainActor.run {
                let m = Model.shared
                switch result {
                case .failure(let e):
                    m.flash(e.description, error: true, thenHide: false)
                case .success(let page):
                    m.toastText = nil
                    let system = "The user is looking at a web page. Answer from its content; say so when the page doesn't cover something. Quote sparingly."
                    m.openChat(prompt: nil, system: system, provider: nil, model: nil, autoSend: false)
                    var body = "URL: \(page.url)\n"
                    if !page.fromTab { body += "(Downloaded copy: content behind a login may be missing.)\n" }
                    body += "\n" + page.text
                    m.chatAttachments.append(ChatAttachment(name: page.title.isEmpty ? page.url : page.title, text: body))
                    if mode == "summarize" {
                        m.sendChat("Summarize this page: the key points as short bullets, then one line on why it matters.")
                    } else if !question.trimmingCharacters(in: .whitespaces).isEmpty {
                        m.sendChat(question)
                    }
                }
            }
        }
    }
}
