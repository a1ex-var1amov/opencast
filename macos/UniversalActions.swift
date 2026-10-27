import AppKit

/// One action model for "do something with this": Act on Selection (global
/// hotkey), ⌘K on file rows, and the File Buffer all use these lists. Each
/// input type has built-in actions plus the scripts and AI commands that
/// declare `accepts` for it; the order learns from what you pick per type.
enum UAInput {
    case text(String)
    case url(URL)
    case files([URL])

    var kind: String {
        switch self {
        case .text: "text"
        case .url: "url"
        case .files: "file"
        }
    }

    /// Short description for list subtitles.
    var label: String {
        switch self {
        case .text(let t):
            let one = t.replacingOccurrences(of: "\n", with: " ")
            return one.count > 60 ? String(one.prefix(60)) + "…" : one
        case .url(let u): return u.absoluteString
        case .files(let f): return f.count == 1 ? f[0].lastPathComponent : "\(f.count) items"
        }
    }

    /// What a script or AI command receives.
    var argument: String {
        switch self {
        case .text(let t): t
        case .url(let u): u.absoluteString
        case .files(let f): f.map(\.path).joined(separator: "\n")
        }
    }

    /// Text that is just a web URL becomes a URL input.
    static func from(text: String) -> UAInput {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !t.contains(" "), !t.contains("\n"), let u = URL(string: t), let scheme = u.scheme?.lowercased(),
           scheme == "http" || scheme == "https", u.host != nil
        {
            return .url(u)
        }
        return .text(t)
    }
}

struct UAction {
    var id: String
    var title: String
    var icon: String
    /// Shortcut for ⌘K (`cmd+shift+f`).
    var key: String?
    var run: @MainActor () -> Void
}

@MainActor
enum UniversalActions {
    /// Every action for `input`, best first.
    static func actions(for input: UAInput, model: Model) -> [UAction] {
        var list: [UAction]
        switch input {
        case .text(let text): list = textActions(text, model: model)
        case .url(let url): list = urlActions(url, model: model)
        case .files(let files): list = fileActions(files, model: model)
        }
        // Scripts and AI commands that accept this input.
        let accepting = (Engine.call(["op": "accepting", "type": input.kind])["commands"] as? [[String: Any]] ?? [])
            .compactMap(Command.init)
        for cmd in accepting {
            list.append(UAction(id: "ua:cmd:\(cmd.id)", title: cmd.title, icon: cmd.icon ?? "sparkles") {
                if cmd.kind == .ai {
                    model.execute(id: cmd.id, args: [], selection: input.argument)
                } else if case .files(let f) = input {
                    model.execute(id: cmd.id, args: f.map(\.path))
                } else {
                    model.execute(id: cmd.id, args: [input.argument])
                }
            })
        }
        // Learned order per input type; ties keep the order above.
        let ranked = Engine.call(["op": "rankActions", "query": "ua:\(input.kind)", "ids": list.map(\.id)])["ids"] as? [String]
        guard let ranked, ranked.count == list.count else { return withRecording(list, input) }
        let byId = Dictionary(list.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return withRecording(ranked.compactMap { byId[$0] }, input)
    }

    /// Wrap each action so running it teaches the ranking.
    private static func withRecording(_ list: [UAction], _ input: UAInput) -> [UAction] {
        list.map { a in
            var a = a
            let run = a.run
            let id = a.id
            a.run = {
                Task.detached { _ = Engine.call(["op": "bumpUsage", "id": id, "query": "ua:\(input.kind)"]) }
                run()
            }
            return a
        }
    }

    // MARK: - Text

    private static func textActions(_ text: String, model: Model) -> [UAction] {
        let word = text.split(whereSeparator: \.isWhitespace).count == 1
        var a: [UAction] = [
            UAction(id: "ua:ask", title: "Ask AI About It", icon: "sparkles", key: "cmd+shift+a") {
                model.openChat(prompt: text + "\n\n", system: nil, provider: nil, model: nil, autoSend: false)
            },
        ]
        if word {
            a.append(UAction(id: "ua:define", title: "Define “\(text)”", icon: "book", key: "cmd+d") {
                model.handleUi("define", args: [text])
            })
            a.append(UAction(id: "ua:spell", title: "Spell “\(text)”", icon: "textformat.abc") {
                model.handleUi("spell", args: [text])
            })
        }
        a += [
            UAction(id: "ua:web", title: "Search the Web", icon: "magnifyingglass", key: "cmd+g") {
                let q = text.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? text
                if let u = URL(string: "https://www.google.com/search?q=\(q)") { NSWorkspace.shared.open(u) }
                model.apply(.hide)
            },
            UAction(id: "ua:large", title: "Large Type", icon: "textformat.size", key: "cmd+l") {
                model.handleUi("large-type", args: [text])
            },
            UAction(id: "ua:note", title: "Save as Note", icon: "note.text") {
                _ = Engine.call(["op": "noteUpdate", "id": "", "title": "", "body": text])
                model.flash("Saved as a note", error: false, thenHide: true)
            },
            UAction(id: "ua:paste-plain", title: "Paste as Plain Text", icon: "clipboard.fill", key: "cmd+shift+v") {
                Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
            },
            UAction(id: "ua:copy", title: "Copy", icon: "clipboard.fill", key: "cmd+c") {
                model.copyToClipboard(text, toast: "Copied")
            },
        ]
        return a
    }

    // MARK: - URL

    private static func urlActions(_ url: URL, model: Model) -> [UAction] {
        var a: [UAction] = [
            UAction(id: "ua:open", title: "Open", icon: "link") {
                NSWorkspace.shared.open(url)
                model.apply(.hide)
            },
        ]
        let browsers = NSWorkspace.shared.urlsForApplications(toOpen: url).prefix(6)
        for app in browsers {
            let name = FileManager.default.displayName(atPath: app.path).replacingOccurrences(of: ".app", with: "")
            a.append(UAction(id: "ua:open-in:\(name)", title: "Open in \(name)", icon: "fileicon:\(app.path)") {
                NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
                model.apply(.hide)
            })
        }
        a += [
            UAction(id: "ua:ask-url", title: "Ask AI About This Link", icon: "sparkles", key: "cmd+shift+a") {
                model.openChat(prompt: url.absoluteString + "\n\n", system: nil, provider: nil, model: nil, autoSend: false)
            },
            UAction(id: "ua:md-link", title: "Copy as Markdown Link", icon: "link", key: "cmd+shift+m") {
                let label = url.host?.replacingOccurrences(of: "www.", with: "") ?? url.absoluteString
                model.copyToClipboard("[\(label)](\(url.absoluteString))", toast: "Copied Markdown link")
            },
            UAction(id: "ua:large", title: "Large Type", icon: "textformat.size", key: "cmd+l") {
                model.handleUi("large-type", args: [url.absoluteString])
            },
            UAction(id: "ua:copy", title: "Copy", icon: "clipboard.fill", key: "cmd+c") {
                model.copyToClipboard(url.absoluteString, toast: "Copied")
            },
        ]
        return a
    }

    // MARK: - Files

    private static func fileActions(_ files: [URL], model: Model) -> [UAction] {
        guard let first = files.first else { return [] }
        var isDir: ObjCBool = false
        FileManager.default.fileExists(atPath: first.path, isDirectory: &isDir)
        let folder = isDir.boolValue ? first : first.deletingLastPathComponent()
        var a: [UAction] = [
            UAction(id: "ua:open", title: "Open", icon: "folder.fill") {
                files.forEach { NSWorkspace.shared.open($0) }
                model.apply(.hide)
            },
            UAction(id: "ua:open-with", title: "Open With…", icon: "shippingbox", key: "cmd+o") {
                showOpenWith(files, model: model)
            },
            UAction(id: "ua:quick-look", title: "Quick Look", icon: "eye", key: "cmd+y") {
                QuickLook.shared.show(files)
            },
            UAction(id: "ua:reveal", title: "Show in Finder", icon: "magnifyingglass", key: "cmd+shift+f") {
                NSWorkspace.shared.activateFileViewerSelecting(files)
                model.apply(.hide)
            },
            UAction(id: "ua:copy-path", title: files.count == 1 ? "Copy Path" : "Copy Paths", icon: "clipboard.fill", key: "cmd+shift+c") {
                model.copyToClipboard(files.map(\.path).joined(separator: "\n"), toast: "Copied path")
            },
            UAction(id: "ua:copy-file", title: files.count == 1 ? "Copy File" : "Copy Files", icon: "doc.fill", key: "cmd+shift+.") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.writeObjects(files.map { $0 as NSURL })
                model.flash(files.count == 1 ? "Copied file" : "Copied \(files.count) files", error: false, thenHide: false)
            },
            UAction(id: "ua:terminal", title: "Open in Terminal", icon: "keyboard", key: "cmd+t") {
                if let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") {
                    NSWorkspace.shared.open([folder], withApplicationAt: terminal, configuration: NSWorkspace.OpenConfiguration())
                }
                model.apply(.hide)
            },
            UAction(id: "ua:attach", title: "Ask AI About \(files.count == 1 ? "This File" : "These Files")", icon: "sparkles", key: "cmd+shift+a") {
                model.openChat(prompt: nil, system: nil, provider: nil, model: nil, autoSend: false)
                for url in files {
                    Task.detached {
                        let text = Attachments.text(of: url)
                        await MainActor.run {
                            if let text { model.attach(ChatAttachment(name: url.lastPathComponent, text: text)) }
                        }
                    }
                }
            },
            UAction(id: "ua:email", title: "Email \(files.count == 1 ? "File" : "Files")", icon: "envelope", key: "cmd+e") {
                NSSharingService(named: .composeEmail)?.perform(withItems: files)
                model.apply(.hide)
            },
            UAction(id: "ua:compress", title: "Compress to Zip", icon: "archivebox") {
                compress(files, model: model)
            },
            UAction(id: "ua:move-to", title: "Move To…", icon: "arrow.right") {
                transfer(files, move: true, model: model)
            },
            UAction(id: "ua:copy-to", title: "Copy To…", icon: "doc.on.doc") {
                transfer(files, move: false, model: model)
            },
            UAction(id: "ua:trash", title: "Move to Trash", icon: "trash.fill", key: "cmd+backspace") {
                NSWorkspace.shared.recycle(files) { _, error in
                    let ok = error == nil
                    Task { @MainActor in
                        model.flash(ok ? "Moved to Trash" : "Could not move to Trash", error: !ok, thenHide: false)
                        model.fileBuffer.removeAll { files.contains($0) }
                        if model.ready { model.search() }
                    }
                }
            },
        ]
        if files.count > 1 { a.removeAll { $0.id == "ua:open-with" } }
        return a
    }

    private static func showOpenWith(_ files: [URL], model: Model) {
        let apps = Array(NSWorkspace.shared.urlsForApplications(toOpen: files[0]).prefix(15))
        let items = apps.enumerated().map { i, app in
            ListItem(title: FileManager.default.displayName(atPath: app.path), subtitle: app.deletingLastPathComponent().path,
                     icon: "fileicon:\(app.path)", arg: String(i))
        }
        model.showLocalList(id: "open-with", title: "Open With", icon: "shippingbox", items: items, action: { i in
            guard apps.indices.contains(i) else { return }
            NSWorkspace.shared.open(files, withApplicationAt: apps[i], configuration: NSWorkspace.OpenConfiguration())
            model.apply(.hide)
        })
    }

    private static func compress(_ files: [URL], model: Model) {
        let dir = files[0].deletingLastPathComponent()
        let base = files.count == 1 ? files[0].deletingPathExtension().lastPathComponent : "Archive"
        var target = dir.appendingPathComponent("\(base).zip")
        var n = 2
        while FileManager.default.fileExists(atPath: target.path) {
            target = dir.appendingPathComponent("\(base) \(n).zip")
            n += 1
        }
        let out = target
        model.busy = true
        Task.detached {
            let p = Process()
            p.executableURL = URL(fileURLWithPath: "/usr/bin/zip")
            p.currentDirectoryURL = dir
            // Relative names keep the archive's paths short; -r for folders, -y keeps symlinks.
            p.arguments = ["-r", "-y", "-q", out.path] + files.map { $0.lastPathComponent }
            let ok = (try? p.run()) != nil && { p.waitUntilExit(); return p.terminationStatus == 0 }()
            await MainActor.run {
                model.busy = false
                if ok {
                    model.flash("Created \(out.lastPathComponent)", error: false, thenHide: false)
                    NSWorkspace.shared.activateFileViewerSelecting([out])
                } else {
                    model.flash("Could not create the zip", error: true, thenHide: false)
                }
            }
        }
    }

    private static func transfer(_ files: [URL], move: Bool, model: Model) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.prompt = move ? "Move Here" : "Copy Here"
        model.keepOnBlurOnce = true
        guard panel.runModal() == .OK, let dest = panel.url else { return }
        var failed = 0
        for f in files {
            let target = dest.appendingPathComponent(f.lastPathComponent)
            do {
                if move { try FileManager.default.moveItem(at: f, to: target) } else { try FileManager.default.copyItem(at: f, to: target) }
            } catch {
                failed += 1
            }
        }
        if move { model.fileBuffer.removeAll { files.contains($0) } }
        let verb = move ? "Moved" : "Copied"
        model.flash(failed == 0 ? "\(verb) \(files.count) item\(files.count == 1 ? "" : "s")" : "\(failed) item(s) failed (name taken?)",
                    error: failed > 0, thenHide: false)
    }
}
