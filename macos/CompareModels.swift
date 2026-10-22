import AppKit
import SwiftUI

/// "Compare Models": one prompt, several provider/model pairs streaming side
/// by side, with time to first token and total time. ⌘↩ sends.
@MainActor
final class CompareController: NSObject, NSWindowDelegate {
    static let shared = CompareController()
    private var window: NSWindow?
    let state = CompareState()

    func show(prompt: String?) {
        state.loadProviders()
        if let prompt, !prompt.trimmingCharacters(in: .whitespaces).isEmpty {
            state.prompt = prompt
        }
        if window == nil {
            let w = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 980, height: 620),
                styleMask: [.titled, .closable, .resizable, .miniaturizable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            w.title = "Compare Models"
            w.titlebarAppearsTransparent = true
            w.isReleasedWhenClosed = false
            w.center()
            w.setFrameAutosaveName("OpenCastCompare")
            w.contentView = NSHostingView(rootView: CompareView(state: state))
            w.delegate = self
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
        if state.canSend, prompt?.isEmpty == false { state.send() }
    }

    func windowWillClose(_ notification: Notification) {
        state.stopAll()
    }

    /// Route `ai` events for our requests. Returns true when handled.
    func handle(_ obj: [String: Any]?) -> Bool {
        guard let id = obj?["requestId"] as? String, let i = state.columns.firstIndex(where: { $0.requestId == id }),
              let event = obj?["event"] as? [String: Any], let kind = event["type"] as? String
        else { return false }
        state.apply(index: i, kind: kind, text: event["text"] as? String ?? "")
        return true
    }
}

struct CompareTarget: Hashable, Identifiable {
    var provider: String
    var model: String
    var id: String { provider + "/" + model }
    var label: String { model.isEmpty ? provider : "\(provider) · \(model)" }
}

struct CompareColumn: Identifiable {
    var target: CompareTarget
    var requestId: String
    var text = ""
    var error: String?
    var started = Date()
    var firstToken: TimeInterval?
    var total: TimeInterval?
    var id: String { requestId }
    var running: Bool { total == nil && error == nil }
}

@MainActor
@Observable
final class CompareState {
    var prompt = ""
    var system = ""
    var available: [CompareTarget] = []
    var chosen: [CompareTarget] = []
    var columns: [CompareColumn] = []

    var canSend: Bool {
        chosen.count >= 1 && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    func loadProviders() {
        let raw = Engine.call(["op": "providers"])["providers"] as? [[String: Any]] ?? []
        let providers = raw.compactMap(Provider.init)
        var out: [CompareTarget] = []
        for p in providers {
            let models = p.models.isEmpty ? [p.defaultModel] : p.models
            for m in models where !out.contains(CompareTarget(provider: p.id, model: m)) {
                out.append(CompareTarget(provider: p.id, model: m))
            }
        }
        available = out
        chosen = chosen.filter { out.contains($0) }
        if chosen.isEmpty {
            // Default: each provider's default model, up to three.
            chosen = providers.prefix(3).map { CompareTarget(provider: $0.id, model: $0.defaultModel) }.filter { out.contains($0) }
        }
    }

    func toggle(_ t: CompareTarget) {
        if let i = chosen.firstIndex(of: t) {
            chosen.remove(at: i)
        } else if chosen.count < 4 {
            chosen.append(t)
        }
    }

    func send() {
        guard canSend else { return }
        stopAll()
        let text = prompt
        let sys = system.trimmingCharacters(in: .whitespacesAndNewlines)
        columns = chosen.map { CompareColumn(target: $0, requestId: UUID().uuidString) }
        for c in columns {
            let id = c.requestId
            let provider = c.target.provider
            let model = c.target.model
            Task.detached {
                var body: [String: Any] = [
                    "op": "stream",
                    "requestId": id,
                    "messages": [["role": "user", "content": text]],
                    "provider": provider,
                    "tools": false,
                    "profile": false,
                ]
                if !model.isEmpty { body["model"] = model }
                if !sys.isEmpty { body["system"] = sys }
                let r = Engine.call(body)
                if let e = r["error"] as? String {
                    await MainActor.run {
                        let s = CompareController.shared.state
                        if let i = s.columns.firstIndex(where: { $0.requestId == id }) { s.columns[i].error = e }
                    }
                }
            }
        }
    }

    func apply(index i: Int, kind: String, text: String) {
        guard columns.indices.contains(i) else { return }
        let elapsed = Date().timeIntervalSince(columns[i].started)
        switch kind {
        case "delta":
            if columns[i].firstToken == nil { columns[i].firstToken = elapsed }
            columns[i].text += text
        case "done":
            columns[i].total = elapsed
        case "error":
            columns[i].error = text.isEmpty ? "Error" : text
        default:
            break
        }
    }

    func stopAll() {
        for c in columns where c.running {
            let id = c.requestId
            Task.detached { _ = Engine.call(["op": "cancel", "requestId": id]) }
        }
    }
}

struct CompareView: View {
    @Bindable var state: CompareState
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 8) {
                TextEditor(text: $state.prompt)
                    .font(.system(size: 13))
                    .frame(minHeight: 54, maxHeight: 110)
                    .scrollContentBackground(.hidden)
                    .padding(6)
                    .background(RoundedRectangle(cornerRadius: 8).fill(Color.primary.opacity(0.05)))
                VStack(spacing: 6) {
                    Button("Send") { state.send() }
                        .keyboardShortcut(.return, modifiers: .command)
                        .disabled(!state.canSend)
                    Button("Stop") { state.stopAll() }
                        .disabled(!state.columns.contains(where: \.running))
                }
            }
            TextField("System prompt (optional)", text: $state.system)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 12))
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(state.available) { t in
                        let on = state.chosen.contains(t)
                        Button { state.toggle(t) } label: {
                            Text(t.label)
                                .font(.system(size: 11, weight: on ? .semibold : .regular))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(Capsule().fill(on ? Color.accentColor.opacity(0.25) : Color.primary.opacity(0.06)))
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            if state.available.isEmpty {
                Text("No AI providers configured — add [[providers]] to config.toml.")
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .top, spacing: 10) {
                ForEach(state.columns) { c in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(c.target.label).font(.system(size: 12, weight: .semibold)).lineLimit(1)
                            Spacer()
                            Text(stats(c)).font(.system(size: 11).monospacedDigit()).foregroundStyle(.secondary)
                        }
                        ScrollView {
                            VStack(alignment: .leading) {
                                if let e = c.error {
                                    Text(e).foregroundStyle(.red)
                                } else if c.text.isEmpty {
                                    ProgressView().controlSize(.small)
                                } else {
                                    MarkdownText(text: c.text, scheme: scheme)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                        }
                        HStack {
                            Spacer()
                            Button("Copy") {
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(c.text, forType: .string)
                            }
                            .controlSize(.small)
                            .disabled(c.text.isEmpty)
                        }
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.04)))
                }
            }
            .frame(maxHeight: .infinity)
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 14)
        .padding(.top, 30)
        .frame(minWidth: 640, minHeight: 420)
    }

    private func stats(_ c: CompareColumn) -> String {
        var parts: [String] = []
        if let f = c.firstToken { parts.append(String(format: "first %.1fs", f)) }
        if let t = c.total {
            parts.append(String(format: "%.1fs", t))
            let words = c.text.split(whereSeparator: \.isWhitespace).count
            if t > 0 { parts.append(String(format: "%.0f words/s", Double(words) / t)) }
        } else if c.error == nil {
            parts.append("…")
        }
        return parts.joined(separator: " · ")
    }
}
