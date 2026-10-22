import SwiftUI

struct ChatView: View {
    @Bindable var model: Model
    @FocusState private var composer: Bool
    @Environment(\.colorScheme) private var systemScheme
    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            messages
            if let approval = model.pendingApproval {
                approvalBanner(approval)
            }
            if !model.chatAttachments.isEmpty {
                attachmentChips
            }
            field
            HintBar(
                left: model.chatStreaming ? "Streaming…" : "OpenCast",
                hints: model.pendingApproval != nil
                    ? [("Allow", "↩"), ("Deny", "esc")]
                    : [
                        ("Attach", "⌘O"),
                        ("Clipboard", "⌘⇧V"),
                        ("Screen", "⌘⇧S"),
                        ("Paste answer", "⌘↩"),
                        ("New", "⌘N"),
                        (model.chatStreaming ? "Stop" : "Back", "esc"),
                    ]
            )
        }
        .onAppear { composer = true }
        .onChange(of: model.focusTick) { _, _ in composer = true }
    }

    private var header: some View {
        HStack(spacing: 10) {
            BackButton { model.backToSearch(clearQuery: true) }
            Text("AI Chat")
                .font(.system(size: 15, weight: .semibold))
            Spacer(minLength: 8)
            Menu(model.chatProviderId ?? "Provider") {
                ForEach(model.providers) { provider in
                    Button(provider.id) { model.pickProvider(provider.id) }
                }
            }
            .fixedSize()
            .disabled(model.providers.isEmpty)
            Menu(model.activeChatModel ?? "Model") {
                ForEach(model.activeProvider?.models ?? [], id: \.self) { name in
                    Button(name) { model.pickChatModel(name) }
                }
            }
            .fixedSize()
            .disabled(model.activeProvider == nil)
        }
        .font(.system(size: 12))
        .padding(.horizontal, 18)
        .frame(height: 58)
        .overlay(alignment: .bottom) {
            Rectangle().fill(Theme.stroke(scheme).opacity(0.45)).frame(height: 0.5)
        }
    }

    private var messages: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if model.providersReady && model.providers.isEmpty {
                        Text("No AI provider configured. Run Open Config Folder, add a [[providers]] entry to config.toml, then Reload Commands.")
                            .font(.system(size: 14))
                            .foregroundStyle(Theme.secondary(scheme))
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: .infinity)
                            .padding(24)
                    } else if model.chatTurns.isEmpty {
                        if model.providersReady {
                            Text(emptyHint)
                                .font(.system(size: 14))
                                .foregroundStyle(Theme.secondary(scheme))
                                .multilineTextAlignment(.center)
                                .frame(maxWidth: .infinity)
                                .padding(24)
                        } else {
                            ProgressView()
                                .controlSize(.small)
                                .frame(maxWidth: .infinity)
                                .padding(24)
                        }
                    }
                    ForEach(Array(model.chatTurns.enumerated()), id: \.offset) { index, turn in
                        turnView(turn, index: index)
                    }
                    Color.clear.frame(height: 1).id("end")
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 14)
            }
            .onChange(of: model.chatTurns) { _, _ in
                proxy.scrollTo("end", anchor: .bottom)
            }
        }
        .frame(maxHeight: .infinity)
    }

    private func approvalBanner(_ a: ToolApproval) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: "wrench.and.screwdriver.fill")
                    .foregroundStyle(Theme.accent)
                Text("Allow \(a.title)?")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                Button("Deny") { model.answerApproval(false) }
                    .buttonStyle(.plain)
                    .foregroundStyle(Theme.secondary(scheme))
                Button("Allow") { model.answerApproval(true) }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
            if a.arguments != "{}" {
                ScrollView {
                    Text(a.arguments)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(Theme.secondary(scheme))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 90)
            }
        }
        .padding(10)
        .background(Theme.accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Theme.accent.opacity(0.35), lineWidth: 1))
        .padding(.horizontal, 14)
        .padding(.top, 6)
    }

    private var attachmentChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(model.chatAttachments) { a in
                    HStack(spacing: 4) {
                        Image(systemName: "paperclip")
                        Text(a.name).lineLimit(1)
                        Text("\(a.text.count) chars")
                            .foregroundStyle(Theme.tertiary(scheme))
                        Button {
                            model.chatAttachments.removeAll { $0.id == a.id }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(Theme.tertiary(scheme))
                    }
                    .font(.system(size: 11.5))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Theme.kbd(scheme), in: Capsule())
                }
            }
            .padding(.horizontal, 14)
        }
        .padding(.top, 6)
    }

    private var emptyHint: String {
        let who = model.chatProviderId ?? "your default provider"
        if let modelName = model.activeChatModel { return "Ask anything. Answers stream from \(who) · \(modelName)." }
        return "Ask anything. Answers stream from \(who)."
    }

    @ViewBuilder private func turnView(_ turn: ChatTurn, index: Int) -> some View {
        if turn.role == "user" {
            HStack {
                Spacer(minLength: 48)
                VStack(alignment: .trailing, spacing: 4) {
                    if !turn.attachmentNames.isEmpty {
                        Text(turn.attachmentNames.map { "📎 " + $0 }.joined(separator: "  "))
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.tertiary(scheme))
                    }
                Text(turn.content)
                    .textSelection(.enabled)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(
                        Theme.accent.opacity(scheme == .light ? 0.12 : 0.18),
                        in: UnevenRoundedRectangle(
                            topLeadingRadius: 12,
                            bottomLeadingRadius: 12,
                            bottomTrailingRadius: 4,
                            topTrailingRadius: 12
                        )
                    )
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(turn.tools.enumerated()), id: \.offset) { _, tool in
                    Label(tool, systemImage: "wrench.and.screwdriver")
                        .font(.system(size: 11.5))
                        .foregroundStyle(Theme.tertiary(scheme))
                }
                if turn.content.isEmpty {
                    if turn.error == nil, model.chatStreaming, index == model.chatTurns.count - 1 {
                        StreamCaret()
                    }
                } else {
                    AssistantBody(text: turn.content) { model.copyToClipboard($0) }
                }
                if let error = turn.error {
                    Text(error)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.danger)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    private var field: some View {
        TextEditor(text: $model.chatInput)
            .font(.system(size: 15))
            .foregroundStyle(Theme.primary(scheme))
            .scrollContentBackground(.hidden)
            .scrollIndicators(.never)
            .focused($composer)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .overlay(alignment: .topLeading) {
                if model.chatInput.isEmpty {
                    Text(model.chatStreaming ? "Streaming… Esc to stop" : "Message — Enter to send, Shift+Enter for a new line")
                        .foregroundStyle(Theme.tertiary(scheme))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .allowsHitTesting(false)
                }
            }
        .frame(minHeight: 40, maxHeight: 120)
        .background(raised, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Theme.stroke(scheme).opacity(0.6), lineWidth: 1))
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }

    private var raised: Color {
        scheme == .dark
            ? Color(red: Double(0x1a) / 255, green: Double(0x1a) / 255, blue: Double(0x1d) / 255)
            : .white
    }
}

private struct StreamCaret: View {
    @State private var dim = false

    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(Theme.accent)
            .frame(width: 8, height: 16)
            .opacity(dim ? 0 : 1)
            .onAppear {
                withAnimation(.linear(duration: 0.5).repeatForever(autoreverses: true)) { dim = true }
            }
    }
}

private struct AssistantBody: View {
    var text: String
    var copy: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Fence.split(text)) { block in
                switch block.kind {
                case .prose(let prose):
                    ProseText(text: prose)
                case .code(let lang, let code):
                    FenceBlock(lang: lang, code: code, copy: copy)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct ProseText: View {
    var text: String

    var body: some View {
        let trimmed = Self.trimNewlines(text)
        if trimmed.isEmpty {
            EmptyView()
        } else if let parsed = try? AttributedString(
            markdown: trimmed,
            options: AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        ) {
            Text(parsed)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            Text(trimmed)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private static func trimNewlines(_ text: String) -> String {
        String(text.drop(while: { $0 == "\n" }).reversed().drop(while: { $0 == "\n" }).reversed())
    }
}

private struct FenceBlock: View {
    var lang: String
    var code: String
    var copy: (String) -> Void
    @State private var copied = false
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(lang.isEmpty ? "code" : lang)
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.tertiary(scheme))
                Spacer()
                Button(copied ? "Copied" : "Copy") {
                    copy(code)
                    copied = true
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(1200))
                        copied = false
                    }
                }
                .buttonStyle(.plain)
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondary(scheme))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            Text(code)
                .font(.system(size: 12.5, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
        }
        .background(codeBg, in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(Theme.stroke(scheme).opacity(0.45), lineWidth: 1))
    }

    private var codeBg: Color {
        scheme == .dark
            ? Color(red: Double(0x18) / 255, green: Double(0x18) / 255, blue: Double(0x1b) / 255)
            : Color(red: Double(0xf1) / 255, green: Double(0xf1) / 255, blue: Double(0xf4) / 255)
    }
}

private struct Fence {
    struct Block: Identifiable {
        enum Kind {
            case prose(String)
            case code(lang: String, text: String)
        }

        var id: Int
        var kind: Kind
    }

    /// Same split as `Markdown.tsx`: ```lang\nbody```, unclosed fences run to the end.
    static func split(_ text: String) -> [Block] {
        let pattern = #"```([^\n`]*)\n([\s\S]*?)(?:```|$)"#
        guard let regex = try? NSRegularExpression(pattern: pattern) else {
            return [Block(id: 0, kind: .prose(text))]
        }
        let ns = text as NSString
        let matches = regex.matches(in: text, range: NSRange(location: 0, length: ns.length))
        var blocks: [Block] = []
        var last = 0
        for match in matches {
            if match.range.location > last {
                blocks.append(Block(id: blocks.count, kind: .prose(ns.substring(with: NSRange(location: last, length: match.range.location - last)))))
            }
            let lang = ns.substring(with: match.range(at: 1)).trimmingCharacters(in: .whitespaces)
            var code = ns.substring(with: match.range(at: 2))
            if code.hasSuffix("\n") { code.removeLast() }
            blocks.append(Block(id: blocks.count, kind: .code(lang: lang, text: code)))
            last = match.range.location + match.range.length
        }
        if last < ns.length {
            blocks.append(Block(id: blocks.count, kind: .prose(ns.substring(from: last))))
        }
        return blocks
    }
}
