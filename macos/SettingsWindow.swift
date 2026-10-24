import AppKit
import AVFoundation
import EventKit
import Security
import ServiceManagement
import Speech
import SwiftUI

/// A real settings window. Everything still lives in `config.toml` (this
/// window writes it with comments kept), so dotfiles keep working.
@MainActor
final class SettingsWindowController {
    static let shared = SettingsWindowController()
    private var window: NSWindow?

    func show(tab: SettingsTab = .general) {
        SettingsState.shared.tab = tab
        SettingsState.shared.load()
        if window == nil {
            let w = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 680, height: 540),
                styleMask: [.titled, .closable, .miniaturizable],
                backing: .buffered,
                defer: false
            )
            w.title = "OpenCast Settings"
            w.isReleasedWhenClosed = false
            w.contentView = NSHostingView(rootView: SettingsView(state: SettingsState.shared))
            w.center()
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

enum SettingsTab: String, CaseIterable, Identifiable {
    case general = "General", ai = "AI", extensions = "Extensions", permissions = "Permissions", about = "About"
    var id: String { rawValue }
    var symbol: String {
        switch self {
        case .general: "gearshape"
        case .ai: "sparkles"
        case .extensions: "puzzlepiece.extension"
        case .permissions: "lock.shield"
        case .about: "info.circle"
        }
    }
}

struct ProviderRow: Identifiable, Equatable {
    var id: String
    var kind: String
    var model: String
    var baseUrl: String?
    var hasKey: Bool
    var local: Bool
}

struct SnippetRow: Identifiable, Equatable {
    var id: String { keyword }
    var keyword: String
    var text: String
}

@MainActor
@Observable
final class SettingsState {
    static let shared = SettingsState()

    var tab: SettingsTab = .general
    var general: [String: Any] = [:]
    var providers: [ProviderRow] = []
    var snippets: [SnippetRow] = []
    var mcp: [McpServerRow] = []
    var configDir = ""
    var version = ""
    var synced = false
    var status = ""
    var hotkeyDraft = ""
    var hotkeyStatus = ""
    var updateRepoDraft = ""
    var updateRepoStatus = ""
    var permissionTick = 0
    /// Last `opencast_hotkeys_fix` lines (filled by `Model.checkHotkeys`).
    var hotkeyProblems: [String] = []
    /// In-flight "Test connection" stream id (routed from Model.accept).
    var providerTestId: String?
    var providerTestStarted: Date?
    var providerTestStatus = ""

    func load() {
        Task.detached {
            let r = Engine.call(["op": "configGet"])
            await MainActor.run {
                let s = SettingsState.shared
                s.general = r["general"] as? [String: Any] ?? [:]
                s.providers = (r["providers"] as? [[String: Any]] ?? []).map {
                    ProviderRow(
                        id: $0["id"] as? String ?? "",
                        kind: $0["kind"] as? String ?? "",
                        model: $0["model"] as? String ?? "",
                        baseUrl: $0["baseUrl"] as? String,
                        hasKey: $0["hasKey"] as? Bool ?? false,
                        local: $0["local"] as? Bool ?? false
                    )
                }
                s.snippets = (r["snippets"] as? [[String: Any]] ?? []).compactMap { row in
                    guard let k = row["keyword"] as? String, let t = row["text"] as? String else { return nil }
                    return SnippetRow(keyword: k, text: t)
                }
                s.mcp = (r["mcp"] as? [[String: Any]] ?? []).compactMap { row in
                    guard let id = row["id"] as? String else { return nil }
                    return McpServerRow(
                        id: id,
                        oauth: row["oauth"] as? Bool ?? false,
                        signedIn: row["signedIn"] as? Bool ?? false
                    )
                }
                s.configDir = r["configDir"] as? String ?? ""
                s.version = r["version"] as? String ?? ""
                s.synced = r["synced"] as? Bool ?? false
                s.hotkeyDraft = s.general["hotkey"] as? String ?? ""
                s.updateRepoDraft = s.general["update_repo"] as? String ?? ""
            }
        }
    }

    func bool(_ key: String) -> Bool { general[key] as? Bool ?? false }
    func string(_ key: String) -> String { general[key] as? String ?? "" }

    func set(_ key: String, _ value: Any) {
        general[key] = value
        Task.detached {
            let r = Engine.call(["op": "configSet", "key": key, "value": value])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? (r["text"] as? String ?? "Saved")
                Model.shared.refreshSettings()
                SnippetExpander.shared.reload()
            }
        }
    }

    /// Open config.toml; jump to `section` when Cursor/VS Code is on PATH (N-6).
    func openConfigSection(_ section: String? = nil) {
        let file = URL(fileURLWithPath: configDir).appendingPathComponent("config.toml")
        guard let section else {
            NSWorkspace.shared.open(file)
            return
        }
        var lineN: Int?
        if let text = try? String(contentsOf: file, encoding: .utf8) {
            for (i, raw) in text.split(separator: "\n", omittingEmptySubsequences: false).enumerated() {
                let t = raw.trimmingCharacters(in: .whitespaces)
                let bare = t.hasPrefix("#")
                    ? String(t.dropFirst()).trimmingCharacters(in: .whitespaces) : t
                if bare == section || bare.hasPrefix(section + " ") {
                    lineN = i + 1
                    break
                }
            }
        }
        if let lineN {
            status = "\(section) is at line \(lineN)"
            let bins = ["/usr/local/bin/cursor", "/opt/homebrew/bin/cursor",
                        "/usr/local/bin/code", "/opt/homebrew/bin/code"]
            if let bin = bins.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) {
                let p = Process()
                p.executableURL = URL(fileURLWithPath: bin)
                p.arguments = ["--goto", "\(file.path):\(lineN)"]
                try? p.run()
                return
            }
        } else {
            status = "\(section) — not found in config.toml"
        }
        NSWorkspace.shared.open(file)
    }

    func addSnippet(keyword: String, text: String) {
        Task.detached {
            let r = Engine.call(["op": "snippetAdd", "keyword": keyword, "text": text])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? (r["text"] as? String ?? "Saved")
                SettingsState.shared.load()
                SnippetExpander.shared.reload()
            }
        }
    }

    func removeSnippet(_ keyword: String) {
        Task.detached {
            let r = Engine.call(["op": "snippetRemove", "keyword": keyword])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? (r["text"] as? String ?? "Removed")
                SettingsState.shared.load()
                SnippetExpander.shared.reload()
            }
        }
    }

    func removeProvider(_ id: String) {
        Task.detached {
            let r = Engine.call(["op": "providerRemove", "id": id])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? "Removed \(id)"
                SettingsState.shared.load()
            }
        }
    }

    func addProvider(id: String, kind: String, baseUrl: String, model: String, key: String, makeDefault: Bool) {
        var provider: [String: Any] = ["id": id, "kind": kind, "model": model]
        if !baseUrl.trimmingCharacters(in: .whitespaces).isEmpty { provider["base_url"] = baseUrl }
        if !key.isEmpty {
            let service = "opencast-\(id)"
            guard Keychain.store(service: service, secret: key) else {
                status = "Could not save the key to the Keychain"
                return
            }
            provider["api_key_cmd"] = "security find-generic-password -s \(service) -w"
        }
        let preferDefault = makeDefault || providers.isEmpty
        Task.detached {
            let r = Engine.call(["op": "providerAdd", "provider": provider])
            if r["error"] == nil, preferDefault {
                _ = Engine.call(["op": "configSet", "key": "default_provider", "value": id])
            }
            await MainActor.run {
                let s = SettingsState.shared
                s.status = (r["error"] as? String) ?? "Added \(id)"
                if preferDefault { s.general["default_provider"] = id }
                s.load()
                Model.shared.refreshSettings()
            }
        }
    }

    /// Tiny stream ping; timing matches CompareModels (started → done).
    func testProvider(_ p: ProviderRow) {
        let id = UUID().uuidString
        providerTestId = id
        providerTestStarted = Date()
        providerTestStatus = "Testing…"
        Task.detached {
            var body: [String: Any] = [
                "op": "stream",
                "requestId": id,
                "messages": [["role": "user", "content": "Reply with exactly: ok"]],
                "provider": p.id,
                "tools": false,
                "profile": false,
            ]
            if !p.model.isEmpty { body["model"] = p.model }
            let r = Engine.call(body)
            if let e = r["error"] as? String {
                await MainActor.run {
                    let s = SettingsState.shared
                    s.providerTestId = nil
                    s.providerTestStatus = e
                    s.status = e
                }
            }
        }
    }

    func handleProviderTest(_ obj: [String: Any]?) -> Bool {
        guard let id = providerTestId, id == obj?["requestId"] as? String,
              let event = obj?["event"] as? [String: Any], let kind = event["type"] as? String
        else { return false }
        let elapsed = providerTestStarted.map { Date().timeIntervalSince($0) } ?? 0
        switch kind {
        case "done":
            providerTestId = nil
            let msg = String(format: "OK · %.1fs", elapsed)
            providerTestStatus = msg
            status = msg
        case "error":
            providerTestId = nil
            let msg = (event["text"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Failed"
            providerTestStatus = msg
            status = msg
        default:
            break
        }
        return true
    }
}

/// Keys are stored as generic passwords that both OpenCast and
/// `/usr/bin/security` (the `api_key_cmd` in config) may read without a prompt.
enum Keychain {
    static func store(service: String, secret: String) -> Bool {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        SecItemDelete(base as CFDictionary)
        var item = base
        item[kSecAttrAccount as String] = NSUserName()
        item[kSecValueData as String] = Data(secret.utf8)
        item[kSecAttrLabel as String] = "OpenCast · \(service)"
        if let access = access(label: service) {
            item[kSecAttrAccess as String] = access
        }
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    /// Lets `/usr/bin/security` read the item without a prompt, which is how
    /// `api_key_cmd = "security find-generic-password …"` works from the engine.
    /// Apple marked these ACL APIs deprecated in 10.10 but never replaced them
    /// for this case; `kSecAttrAccess` on `SecItemAdd` is still the only way to
    /// name a second reader. The alternative would be keys in plain files.
    private static func access(label: String) -> SecAccess? {
        var apps: [SecTrustedApplication] = []
        var me: SecTrustedApplication?
        if SecTrustedApplicationCreateFromPath(nil, &me) == errSecSuccess, let me { apps.append(me) }
        var cli: SecTrustedApplication?
        if SecTrustedApplicationCreateFromPath("/usr/bin/security", &cli) == errSecSuccess, let cli { apps.append(cli) }
        var access: SecAccess?
        guard SecAccessCreate("OpenCast \(label)" as CFString, apps as CFArray, &access) == errSecSuccess else { return nil }
        return access
    }
}

struct SettingsView: View {
    @Bindable var state: SettingsState

    var body: some View {
        NavigationSplitView {
            List(SettingsTab.allCases, selection: Binding(get: { state.tab }, set: { if let t = $0 { state.tab = t } })) { tab in
                Label(tab.rawValue, systemImage: tab.symbol).tag(tab)
            }
            .navigationSplitViewColumnWidth(170)
        } detail: {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    switch state.tab {
                    case .general: GeneralPane(state: state)
                    case .ai: AIPane(state: state)
                    case .extensions: ExtensionsPane(state: state)
                    case .permissions: PermissionsPane(state: state)
                    case .about: AboutPane(state: state)
                    }
                    if !state.status.isEmpty {
                        Text(state.status)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(minWidth: 680, minHeight: 540)
    }
}

struct PaneSection<Content: View>: View {
    var title: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline)
            content
        }
    }
}

private struct GeneralPane: View {
    @Bindable var state: SettingsState
    @State private var loginItem = SMAppService.mainApp.status == .enabled
    @State private var newKeyword = ""
    @State private var newText = ""

    var body: some View {
        PaneSection(title: "Launcher") {
            HStack {
                Text("Hotkey")
                Text(state.hotkeyDraft.isEmpty ? "Not set" : state.hotkeyDraft)
                    .font(.system(.body, design: .monospaced))
                    .frame(minWidth: 120, alignment: .leading)
                Button(Model.shared.recordingLauncherHotkey ? "Listening…" : "Record…") {
                    Model.shared.beginRecordLauncherHotkey()
                }
                .disabled(Model.shared.recordingLauncherHotkey)
                if !state.hotkeyStatus.isEmpty {
                    Text(state.hotkeyStatus).font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("Examples: Alt+Space, Cmd+Space, Ctrl+Alt+K, F18. With the hyper key on, Hyper+Space works too.")
                .font(.caption).foregroundStyle(.secondary)
            if !state.hotkeyProblems.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Hotkey problems").font(.subheadline.weight(.semibold)).foregroundStyle(.orange)
                    ForEach(state.hotkeyProblems, id: \.self) { line in
                        Text(line).font(.caption).foregroundStyle(.orange)
                    }
                }
            }
            Picker("Position", selection: Binding(get: { state.string("presentation") }, set: { state.set("presentation", $0) })) {
                Text("Island (top of screen)").tag("island")
                Text("Spotlight (centered)").tag("spotlight")
            }
            .pickerStyle(.radioGroup)
            Toggle("Launch at login", isOn: Binding(get: { loginItem }, set: { on in
                do {
                    if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
                    loginItem = on
                } catch {
                    state.status = "Login item: \(error.localizedDescription)"
                }
            }))
        }
        PaneSection(title: "Keyboard") {
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Hyper key", isOn: Binding(get: { state.bool("hyperkey") }, set: { state.set("hyperkey", $0) }))
                Text("Caps Lock becomes ⌃⌥⌘⇧ when held; tap opens the launcher. Restart to apply.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        PaneSection(title: "Snippets") {
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Expand while typing", isOn: Binding(get: { state.bool("snippet_expansion") }, set: { state.set("snippet_expansion", $0) }))
                Text("Keywords that start with a symbol (like ;sig) expand in any app.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if state.snippets.isEmpty {
                Text("None yet.").foregroundStyle(.secondary)
            }
            ForEach(state.snippets) { s in
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(s.keyword).font(.system(size: 13, weight: .semibold))
                        Text(s.text).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Spacer()
                    Button(role: .destructive) { state.removeSnippet(s.keyword) } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Remove snippet")
                }
                .padding(.vertical, 2)
            }
            HStack {
                TextField(";sig", text: $newKeyword).textFieldStyle(.roundedBorder).frame(width: 100)
                TextField("expansion text", text: $newText).textFieldStyle(.roundedBorder)
                Button("Add") {
                    let k = newKeyword.trimmingCharacters(in: .whitespaces)
                    let t = newText
                    guard !k.isEmpty, !t.isEmpty else { return }
                    state.addSnippet(keyword: k, text: t)
                    newKeyword = ""
                    newText = ""
                }
                .disabled(newKeyword.trimmingCharacters(in: .whitespaces).isEmpty || newText.isEmpty)
            }
        }
        PaneSection(title: "Config") {
            Text("Everything else is in config.toml. These open the file at the right section when Cursor or VS Code is available.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Open config.toml") { state.openConfigSection() }
                Button("Global hotkeys…") { state.openConfigSection("[[hotkeys]]") }
                Button("Snippets…") { state.openConfigSection("[[snippets]]") }
                Button("MCP servers…") { state.openConfigSection("[[mcp_servers]]") }
            }
        }
        PaneSection(title: "Search") {
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Shortcuts.app", isOn: Binding(get: { state.bool("shortcuts") }, set: { state.set("shortcuts", $0) }))
                Text("Include Shortcuts.app shortcuts in search.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Next meeting", isOn: Binding(get: { state.bool("calendar") }, set: { state.set("calendar", $0) }))
                Text("Show the next meeting at the top of the launcher.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Login shell for scripts", isOn: Binding(get: { state.bool("script_login_shell") }, set: { state.set("script_login_shell", $0) }))
                Text("Run scripts through your login shell (Homebrew, nvm… on PATH).")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        PaneSection(title: "iCloud Sync") {
            if state.synced {
                Label("config.toml, scripts, notes, layouts and pins sync through your synced folder", systemImage: "checkmark.icloud")
            } else {
                Text("Keep config.toml, scripts, notes, layouts and pins in iCloud Drive so every Mac shares them. Clipboard history, chats and usage stay on this Mac.")
                    .font(.caption).foregroundStyle(.secondary)
                Button("Sync with iCloud Drive") {
                    Task.detached {
                        let r = Engine.call(["op": "execute", "id": "builtin:sync-icloud", "args": [String](), "quiet": true])
                        await MainActor.run {
                            SettingsState.shared.status = (r["text"] as? String) ?? (r["message"] as? String) ?? "Done"
                            SettingsState.shared.load()
                        }
                    }
                }
            }
        }
    }
}

private struct Preset: Identifiable {
    var id: String
    var kind: String
    var baseUrl: String
    var model: String
    var needsKey: Bool

    static let all: [Preset] = [
        Preset(id: "openai", kind: "openai", baseUrl: "", model: "gpt-5-mini", needsKey: true),
        Preset(id: "claude", kind: "anthropic", baseUrl: "", model: "claude-sonnet-5-5", needsKey: true),
        Preset(id: "gemini", kind: "gemini", baseUrl: "", model: "gemini-2.5-flash", needsKey: true),
        Preset(id: "openrouter", kind: "openai", baseUrl: "https://openrouter.ai/api/v1", model: "meta-llama/llama-3.3-70b-instruct", needsKey: true),
        Preset(id: "groq", kind: "openai", baseUrl: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", needsKey: true),
        Preset(id: "ollama", kind: "openai", baseUrl: "http://localhost:11434/v1", model: "llama3.2", needsKey: false),
        Preset(id: "lmstudio", kind: "openai", baseUrl: "http://localhost:1234/v1", model: "local-model", needsKey: false),
        Preset(id: "custom", kind: "openai", baseUrl: "", model: "", needsKey: true),
    ]
}

private struct AIPane: View {
    @Bindable var state: SettingsState
    @State private var preset = Preset.all[1]
    @State private var newId = "claude"
    @State private var kind = "anthropic"
    @State private var baseUrl = ""
    @State private var model = "claude-sonnet-5-5"
    @State private var key = ""
    @State private var makeDefault = true

    private var addBlockedReason: String? {
        if newId.trimmingCharacters(in: .whitespaces).isEmpty { return "Id is required." }
        if model.trimmingCharacters(in: .whitespaces).isEmpty { return "Model is required." }
        return nil
    }

    var body: some View {
        PaneSection(title: "Providers") {
            if state.providers.isEmpty {
                Text("No providers yet. Add one below.").foregroundStyle(.secondary)
            }
            ForEach(state.providers) { p in
                HStack {
                    Button {
                        state.set("default_provider", p.id)
                    } label: {
                        Image(systemName: p.id == state.string("default_provider") ? "star.fill" : "star")
                            .foregroundStyle(p.id == state.string("default_provider") ? Color.accentColor : Color.secondary)
                    }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Make default")
                    .help("Make default")
                    VStack(alignment: .leading) {
                        Text(p.id).font(.system(size: 13, weight: .semibold))
                        Text("\(p.kind) · \(p.model)\(p.baseUrl.map { " · \($0)" } ?? "")")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !p.hasKey && !p.local {
                        Text("no key").font(.caption).foregroundStyle(.orange)
                    }
                    Button("Test connection") { state.testProvider(p) }
                        .buttonStyle(.borderless)
                        .disabled(state.providerTestId != nil)
                        .accessibilityLabel("Test connection")
                    Button(role: .destructive) { state.removeProvider(p.id) } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Remove provider")
                }
                .padding(.vertical, 2)
            }
            if !state.providerTestStatus.isEmpty {
                Text(state.providerTestStatus).font(.caption).foregroundStyle(.secondary)
            }
        }
        PaneSection(title: "Add a provider") {
            Picker("Preset", selection: Binding(get: { preset.id }, set: { id in
                if let p = Preset.all.first(where: { $0.id == id }) {
                    preset = p
                    newId = p.id == "custom" ? "" : p.id
                    kind = p.kind
                    baseUrl = p.baseUrl
                    model = p.model
                }
            })) {
                ForEach(Preset.all) { Text($0.id).tag($0.id) }
            }
            .frame(width: 260)
            Grid(alignment: .leading, verticalSpacing: 8) {
                GridRow {
                    Text("Id")
                    TextField("claude", text: $newId).textFieldStyle(.roundedBorder)
                }
                GridRow {
                    Text("Wire format")
                    Picker("", selection: $kind) {
                        Text("OpenAI-compatible").tag("openai")
                        Text("Anthropic").tag("anthropic")
                        Text("Gemini").tag("gemini")
                    }
                    .labelsHidden()
                }
                GridRow {
                    Text("Base URL")
                    TextField("default for the wire format", text: $baseUrl).textFieldStyle(.roundedBorder)
                }
                GridRow {
                    Text("Model")
                    TextField("model id your account serves", text: $model).textFieldStyle(.roundedBorder)
                }
                GridRow {
                    Text("API key")
                    SecureField(preset.needsKey ? "stored in your Keychain, never in config.toml" : "not needed for local servers", text: $key)
                        .textFieldStyle(.roundedBorder)
                }
            }
            Toggle("Make default", isOn: $makeDefault)
            Button("Add Provider") {
                state.addProvider(
                    id: newId.trimmingCharacters(in: .whitespaces),
                    kind: kind,
                    baseUrl: baseUrl,
                    model: model,
                    key: key,
                    makeDefault: makeDefault
                )
                key = ""
                makeDefault = false
            }
            .disabled(addBlockedReason != nil)
            if let reason = addBlockedReason {
                Text(reason).font(.caption).foregroundStyle(.secondary)
            }
        }
        .onAppear { makeDefault = state.providers.isEmpty }
    }
}

private struct ExtensionsPane: View {
    @Bindable var state: SettingsState

    var body: some View {
        McpServersPaneSection(state: state)
        PaneSection(title: "Script commands") {
            Text("Executable files in the scripts folder, with Raycast-style headers. `@opencast.mode list` makes a Script Filter.")
                .foregroundStyle(.secondary)
            Button("Open Scripts Folder") {
                NSWorkspace.shared.open(URL(fileURLWithPath: state.configDir).appendingPathComponent("scripts"))
            }
        }
    }
}

private struct PermissionsPane: View {
    @Bindable var state: SettingsState

    var body: some View {
        let _ = state.permissionTick
        PaneSection(title: "Permissions") {
            Text("OpenCast only asks for what a feature needs. Everything here works on-device.")
                .foregroundStyle(.secondary)
            PermissionRow(
                title: "Accessibility",
                detail: "Window management, AI on selected text, paste, snippets, menu search",
                granted: AXIsProcessTrusted(),
                pane: "Privacy_Accessibility",
                request: {
                    let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                    _ = AXIsProcessTrustedWithOptions(opts)
                }
            )
            PermissionRow(
                title: "Screen Recording",
                detail: "Click Text (on-screen OCR) and attaching screen text to chats. Rebuilt or dev binaries often need the checkbox toggled off and on again.",
                granted: CGPreflightScreenCaptureAccess(),
                pane: "Privacy_ScreenCapture",
                request: { _ = CGRequestScreenCaptureAccess() }
            )
            PermissionRow(
                title: "Calendars",
                detail: "My Schedule and the next-meeting row",
                granted: EKEventStore.authorizationStatus(for: .event) == .fullAccess,
                pane: "Privacy_Calendars",
                request: { Schedule.shared.requestAccess { _ in SettingsState.shared.permissionTick += 1 } }
            )
            PermissionRow(
                title: "Microphone",
                detail: "Dictation",
                granted: AVCaptureDevice.authorizationStatus(for: .audio) == .authorized,
                pane: "Privacy_Microphone",
                request: {
                    AVCaptureDevice.requestAccess(for: .audio) { _ in
                        Task { @MainActor in SettingsState.shared.permissionTick += 1 }
                    }
                }
            )
            PermissionRow(
                title: "Speech Recognition",
                detail: "Dictation (on-device when your language supports it)",
                granted: SFSpeechRecognizer.authorizationStatus() == .authorized,
                pane: "Privacy_SpeechRecognition",
                request: {
                    SFSpeechRecognizer.requestAuthorization { _ in
                        Task { @MainActor in SettingsState.shared.permissionTick += 1 }
                    }
                }
            )
            PermissionRow(
                title: "Automation",
                detail: "Front browser tab for Ask About Page (grant OpenCast per browser)",
                granted: false,
                pane: "Privacy_Automation",
                request: {}
            )
            Button("Re-check") { state.permissionTick += 1 }
        }
    }
}

private struct PermissionRow: View {
    var title: String
    var detail: String
    var granted: Bool
    var pane: String
    var request: () -> Void

    var body: some View {
        HStack(alignment: .top) {
            Image(systemName: granted ? "checkmark.circle.fill" : "circle.dashed")
                .foregroundStyle(granted ? .green : .secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if !granted {
                Button("Allow…") {
                    request()
                    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
                        NSWorkspace.shared.open(url)
                    }
                }
            }
        }
        .padding(.vertical, 3)
    }
}

private struct AboutPane: View {
    @Bindable var state: SettingsState

    var body: some View {
        PaneSection(title: "OpenCast \(state.version)") {
            Text("Open-source launcher. MIT licensed. No account, no telemetry; the only network traffic is to the AI and MCP endpoints you configure.")
                .foregroundStyle(.secondary)
            HStack {
                Button("Open Config Folder") { NSWorkspace.shared.open(URL(fileURLWithPath: state.configDir)) }
                Button("Check for Updates") {
                    Task.detached {
                        let r = Engine.call(["op": "execute", "id": "builtin:check-updates", "args": [String](), "quiet": true])
                        await MainActor.run {
                            SettingsState.shared.status = (r["text"] as? String) ?? (r["message"] as? String) ?? ""
                        }
                    }
                }
            }
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Releases from GitHub repo")
                    TextField("owner/repo", text: $state.updateRepoDraft)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 220)
                        .onSubmit {
                            state.set("update_repo", state.updateRepoDraft)
                            state.updateRepoStatus = "Saved"
                        }
                }
                Text(state.updateRepoStatus.isEmpty ? "Press Return to save." : state.updateRepoStatus)
                    .font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Toggle("Weekly update check", isOn: Binding(get: { state.bool("check_updates") }, set: { state.set("check_updates", $0) }))
                Text("Check for updates once a week.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
