import AppKit

// ponytail: Chat / AI stream helpers peeled from OpenCastApp Model — move-only.
extension Model {
    /// Reopen a saved chat with its provider and model.
    func resumeChat(id: String, provider: String?, model: String?, system: String?, turns: [ChatTurn]) {
        chatPreset = nil
        openChat(prompt: nil, system: system, provider: provider, model: model, autoSend: false)
        chatId = id
        chatTurns = turns
    }

    /// Tab / Ask AI: a fresh chat with the default provider settings.
    func openPlainChat(prompt: String?) {
        // Cheap config read (no network). Empty list → Settings AI, not a dead chat.
        let list = ((Engine.call(["op": "providers"])["providers"]) as? [[String: Any]])?.compactMap(Provider.init) ?? []
        providers = list
        providersReady = true
        if list.isEmpty {
            SettingsWindowController.shared.show(tab: .ai)
            return
        }
        chatTemperature = nil
        chatMaxTokens = nil
        chatPreset = nil
        openChat(prompt: prompt, system: nil, provider: nil, model: nil, autoSend: prompt != nil)
    }

    func openChat(prompt: String?, system: String?, provider: String?, model: String?, autoSend: Bool) {
        stopStream()
        providerGen += 1
        chatTurns = []
        chatId = "chat:\(UUID().uuidString)"
        chatAttachments = []
        pendingApproval = nil
        chatInput = ""
        chatSystem = system
        chatProviderId = provider
        chatModel = model
        providers = []
        providersReady = false
        mode = .chat(prompt: prompt, system: system, provider: provider, model: model, autoSend: autoSend)
        noteContentChanged()
        focusTick += 1
        if let prompt, !prompt.isEmpty {
            if autoSend { sendChat(prompt) } else { chatInput = prompt }
        }
        loadProviders()
    }

    func chatKey(_ event: NSEvent, _ mods: NSEvent.ModifierFlags) -> Bool {
        if pendingApproval != nil, mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            answerApproval(true)
            return true
        }
        let chars = event.charactersIgnoringModifiers?.lowercased()
        if mods == .command, chars == "o" {
            attachFile()
            return true
        }
        if mods == [.command, .shift], chars == "v" {
            attachClipboard()
            return true
        }
        if mods == [.command, .shift], chars == "s" {
            attachScreen()
            return true
        }
        if mods == [.command, .shift], chars == "[" {
            cycleProvider(-1)
            return true
        }
        if mods == [.command, .shift], chars == "]" {
            cycleProvider(1)
            return true
        }
        if mods == [.command, .shift], chars == "m" {
            cycleModel(1)
            return true
        }
        if keyMatches(event, "paste_answer") {
            pasteLastAnswer()
            return true
        }
        if keyMatches(event, "copy_answer") {
            copyAnswer()
            return true
        }
        if mods == [.command, .shift], event.charactersIgnoringModifiers?.lowercased() == "r" {
            rememberInProfile()
            return true
        }
        if keyMatches(event, "new_chat") {
            newChat()
            return true
        }
        if keyMatches(event, "dictate") {
            toggleDictation()
            return true
        }
        if mods.isEmpty, event.keyCode == 36 || event.keyCode == 76 {
            if providersReady && providers.isEmpty {
                SettingsWindowController.shared.show(tab: .ai)
                return true
            }
            if fieldIsComposing { return false }
            sendChat(chatInput)
            return true
        }
        return false
    }

    func cycleProvider(_ delta: Int) {
        guard !providers.isEmpty else { return }
        let ids = providers.map(\.id)
        let i = chatProviderId.flatMap { ids.firstIndex(of: $0) } ?? 0
        let next = ids[(i + delta % ids.count + ids.count) % ids.count]
        pickProvider(next)
    }

    func cycleModel(_ delta: Int) {
        guard let models = activeProvider?.models, !models.isEmpty else { return }
        let current = activeChatModel ?? models[0]
        let i = models.firstIndex(of: current) ?? 0
        pickChatModel(models[(i + delta % models.count + models.count) % models.count])
    }

    func sendChat(_ text: String) {
        let content = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !content.isEmpty, !chatStreaming else { return }
        let temperature = chatTemperature
        let maxTokens = chatMaxTokens
        let history = chatTurns.filter { turn in
            !(turn.role == "assistant" && (turn.error != nil || turn.content.isEmpty))
        }
        let attached = chatAttachments
        chatAttachments = []
        let context = attached.filter { !$0.text.isEmpty }
            .map { "<attachment name=\"\($0.name)\">\n\($0.text)\n</attachment>" }.joined(separator: "\n\n")
        let images = attached.compactMap { a -> [String: String]? in
            guard let b64 = a.imageB64, let mime = a.imageMime else { return nil }
            return ["mime": mime, "b64": b64]
        }
        let userTurn = ChatTurn(
            role: "user", content: content, context: context,
            attachmentNames: attached.map(\.name), images: images)
        // Images ride along on every later turn too: "what about the box in the
        // top-left?" only makes sense if the model can still see the shot.
        let messages: [[String: Any]] = (history + [userTurn]).map { turn in
            var m: [String: Any] = ["role": turn.role, "content": turn.wireContent]
            if !turn.images.isEmpty { m["images"] = turn.images }
            return m
        }
        let id = UUID().uuidString
        chatRequestId = id
        chatStreaming = true
        chatTurns = history + [userTurn, ChatTurn(role: "assistant", content: "")]
        chatInput = ""
        let provider = chatProviderId
        let model = chatModel
        let system = chatSystem
        let preset = chatPreset
        Task.detached {
            var body: [String: Any] = [
                "op": "stream",
                "requestId": id,
                "messages": messages,
            ]
            if let preset { body["preset"] = preset }
            if let provider { body["provider"] = provider }
            if let model { body["model"] = model }
            if let system { body["system"] = system }
            if let temperature { body["temperature"] = temperature }
            if let maxTokens { body["maxTokens"] = maxTokens }
            let result = Engine.call(body)
            if let error = result["error"] as? String {
                await MainActor.run { Model.shared.failStream(id, error) }
            }
        }
    }

    /// `output = replace | paste | copy`: stream without opening chat, then act.
    func runSilentAi(prompt: String, system: String?, provider: String?, model: String?, output: AiOutput) {
        stopStream()
        let id = UUID().uuidString
        chatRequestId = id
        chatStreaming = true
        silentOutput = output
        silentBuffer = ""
        busy = true
        flash("Thinking…", error: false, thenHide: false)
        let temperature = chatTemperature
        let maxTokens = chatMaxTokens
        Task.detached {
            var body: [String: Any] = [
                "op": "stream",
                "requestId": id,
                "messages": [["role": "user", "content": prompt]],
                // A rewrite/paste command: no tools, no profile.
                "tools": false,
                "profile": false,
            ]
            if let provider { body["provider"] = provider }
            if let model { body["model"] = model }
            if let system { body["system"] = system }
            if let temperature { body["temperature"] = temperature }
            if let maxTokens { body["maxTokens"] = maxTokens }
            let result = Engine.call(body)
            if let error = result["error"] as? String {
                await MainActor.run { Model.shared.failStream(id, error) }
            }
        }
    }

    private func finishSilentAi(error: String?) {
        let output = silentOutput
        let text = silentBuffer.trimmingCharacters(in: .whitespacesAndNewlines)
        silentOutput = nil
        silentBuffer = ""
        busy = false
        toastText = nil
        if let error {
            flash(error, error: true, thenHide: false)
            return
        }
        guard !text.isEmpty else {
            flash("The model returned nothing", error: true, thenHide: false)
            return
        }
        switch output {
        case .copy:
            copyToClipboard(text, toast: "Copied answer")
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 900_000_000)
                self.apply(.hide)
            }
        default:
            // replace / paste: ⌘V over the still-selected text in the previous app.
            query = ""
            mode = .search
            Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
        }
    }

    private func loadProviders() {
        let gen = providerGen
        Task.detached {
            let result = Engine.call(["op": "providers"])
            let parsed = (result["providers"] as? [[String: Any]])?.compactMap(Provider.init) ?? []
            await MainActor.run {
                guard Model.shared.providerGen == gen, case .chat = Model.shared.mode else { return }
                Model.shared.providers = parsed
                Model.shared.providersReady = true
                if Model.shared.chatProviderId == nil {
                    Model.shared.chatProviderId = parsed.first { $0.isDefault }?.id ?? parsed.first?.id
                }
            }
        }
    }

    func applyAI(_ obj: [String: Any]?) {
        guard let requestId = obj?["requestId"] as? String, requestId == chatRequestId,
              let event = obj?["event"] as? [String: Any],
              let kind = event["type"] as? String
        else { return }
        if silentOutput != nil {
            switch kind {
            case "delta":
                silentBuffer += event["text"] as? String ?? ""
            case "error":
                endStream()
                finishSilentAi(error: event["text"] as? String ?? "Error")
            case "done":
                endStream()
                finishSilentAi(error: nil)
            default:
                break
            }
            return
        }
        switch kind {
        case "delta":
            guard !chatTurns.isEmpty else { return }
            var turns = chatTurns
            turns[turns.count - 1].content += event["text"] as? String ?? ""
            chatTurns = turns
        case "tool":
            guard !chatTurns.isEmpty else { return }
            var turns = chatTurns
            turns[turns.count - 1].tools.append(event["text"] as? String ?? "tool")
            chatTurns = turns
            if pendingApproval?.callId == event["callId"] as? String { pendingApproval = nil }
        case "approve":
            let args: String = {
                guard let a = event["arguments"], let data = try? JSONSerialization.data(withJSONObject: a, options: [.prettyPrinted, .sortedKeys]) else { return "{}" }
                return String(data: data, encoding: .utf8) ?? "{}"
            }()
            pendingApproval = ToolApproval(
                requestId: requestId,
                callId: event["callId"] as? String ?? "",
                title: event["text"] as? String ?? "tool",
                arguments: args
            )
            NSSound(named: "Tink")?.play()
        case "error":
            if !chatTurns.isEmpty {
                var turns = chatTurns
                turns[turns.count - 1].error = event["text"] as? String ?? "Error"
                chatTurns = turns
            }
            pendingApproval = nil
            endStream()
        case "done":
            pendingApproval = nil
            endStream()
            saveChat()
        default:
            break
        }
    }

    private func failStream(_ id: String, _ message: String) {
        guard chatRequestId == id else { return }
        if silentOutput != nil {
            endStream()
            finishSilentAi(error: message)
            return
        }
        if !chatTurns.isEmpty {
            var turns = chatTurns
            turns[turns.count - 1].error = message
            chatTurns = turns
        }
        endStream()
    }

    private func endStream() {
        chatRequestId = nil
        chatStreaming = false
        focusTick += 1
    }

    /// Clears the streaming flag, then asks Rust to abort. No `done` follows.
    func stopStream() {
        let id = chatRequestId
        chatRequestId = nil
        chatStreaming = false
        if silentOutput != nil {
            silentOutput = nil
            silentBuffer = ""
            busy = false
        }
        guard let id else { return }
        Task.detached { _ = Engine.call(["op": "cancel", "requestId": id]) }
    }

    /// Answer the pending tool approval.
    func answerApproval(_ allow: Bool) {
        guard let a = pendingApproval else { return }
        pendingApproval = nil
        Task.detached {
            _ = Engine.call(["op": "approveTool", "requestId": a.requestId, "callId": a.callId, "allow": allow])
        }
    }

    /// Persist the current chat (after each finished answer).
    private func saveChat() {
        let turns = chatTurns.filter { !$0.content.isEmpty && $0.error == nil }
        guard turns.contains(where: { $0.role == "assistant" }) else { return }
        let messages: [[String: Any]] = turns.map { ["role": $0.role, "content": $0.wireContent] }
        var chat: [String: Any] = [
            "id": chatId,
            "title": "",
            "updated": Int(Date().timeIntervalSince1970),
            "messages": messages,
        ]
        if let p = chatProviderId { chat["provider"] = p }
        if let m = activeChatModel { chat["model"] = m }
        if let s = chatSystem { chat["system"] = s }
        let body: [String: Any] = ["op": "chatSave", "chat": chat]
        Task.detached { _ = Engine.call(body) }
    }

    func attach(_ attachment: ChatAttachment) {
        chatAttachments.append(attachment)
        focusTick += 1
    }

    /// Does the provider this chat would use take image attachments? When it
    /// does, screenshots go as pixels; otherwise the panel OCRs them on-device.
    var chatVision: Bool {
        if providers.isEmpty {
            // Cheap: reads config, no network. Attachments can be added before
            // the chat view has loaded its provider list.
            providers = ((Engine.call(["op": "providers"])["providers"]) as? [[String: Any]])?
                .compactMap(Provider.init) ?? []
        }
        let p = providers.first { $0.id == chatProviderId } ?? providers.first { $0.isDefault } ?? providers.first
        return p?.vision ?? false
    }

    func attachFile() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        keepOnBlurOnce = true
        guard panel.runModal() == .OK else { return }
        for url in panel.urls {
            let vision = chatVision
            let name = url.lastPathComponent
            Task.detached {
                // Pixels for a vision model (layout, colour, a chart's shape);
                // everything else gets on-device OCR text, as before.
                if vision, let img = Attachments.imageData(of: url) {
                    await MainActor.run {
                        Model.shared.attach(ChatAttachment(name: name, text: "", imageB64: img.b64, imageMime: img.mime))
                    }
                    return
                }
                let text = Attachments.text(of: url)
                await MainActor.run {
                    if let text {
                        Model.shared.attach(ChatAttachment(name: name, text: text))
                    } else {
                        Model.shared.flash("Could not read \(name)", error: true, thenHide: false)
                    }
                }
            }
        }
        launcher?.makeKeyAndOrderFront(nil)
    }

    func attachClipboard() {
        if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
            attach(ChatAttachment(name: "Clipboard", text: text))
            return
        }
        if let data = NSPasteboard.general.data(forType: .png) ?? NSPasteboard.general.data(forType: .tiff) {
            let vision = chatVision
            Task.detached {
                if vision, let img = Attachments.imageData(data) {
                    await MainActor.run {
                        Model.shared.attach(
                            ChatAttachment(name: "Clipboard image", text: "", imageB64: img.b64, imageMime: img.mime))
                    }
                    return
                }
                let text = Attachments.ocr(data: data) ?? ""
                await MainActor.run {
                    if text.isEmpty {
                        Model.shared.flash("No text in the clipboard image", error: true, thenHide: false)
                    } else {
                        Model.shared.attach(ChatAttachment(name: "Clipboard image", text: text))
                    }
                }
            }
            return
        }
        flash("The clipboard is empty", error: true, thenHide: false)
    }

    /// The screen under the panel: pixels for a vision model, OCR text otherwise
    /// (needs Screen Recording).
    func attachScreen() {
        let target = previousApp?.localizedName ?? "Screen"
        let vision = chatVision
        launcher?.alphaValue = 0
        Task.detached {
            try? await Task.sleep(nanoseconds: 120_000_000)
            if vision, let shot = Attachments.screenCaptureData(), let img = Attachments.imageData(shot) {
                await MainActor.run {
                    Model.shared.launcher?.alphaValue = 1
                    Model.shared.attach(
                        ChatAttachment(name: "\(target) (screen)", text: "", imageB64: img.b64, imageMime: img.mime))
                }
                return
            }
            let text = Attachments.screenText()
            await MainActor.run {
                Model.shared.launcher?.alphaValue = 1
                if let text, !text.isEmpty {
                    Model.shared.attach(ChatAttachment(name: "\(target) (screen)", text: text))
                } else {
                    Model.shared.flash("No text on screen (Screen Recording permission?)", error: true, thenHide: false)
                }
            }
        }
    }

    private func newChat() {
        stopStream()
        chatTurns = []
        focusTick += 1
    }

    private func copyAnswer() {
        guard let text = lastAnswer else { return }
        copyToClipboard(text, toast: "Copied answer")
    }

    private func pasteLastAnswer() {
        guard let text = lastAnswer else { return }
        Task.detached { _ = Engine.call(["op": "paste", "text": text]) }
    }

    private var fieldIsComposing: Bool {
        (NSApp.keyWindow?.firstResponder as? NSTextView)?.hasMarkedText() == true
    }
}
