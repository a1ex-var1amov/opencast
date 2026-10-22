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

@MainActor
@Observable
final class SettingsState {
    static let shared = SettingsState()

    var tab: SettingsTab = .general
    var general: [String: Any] = [:]
    var providers: [ProviderRow] = []
    var mcp: [String] = []
    var configDir = ""
    var version = ""
    var synced = false
    var status = ""
    var hotkeyDraft = ""
    var updateRepoDraft = ""
    var permissionTick = 0

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
                s.mcp = (r["mcp"] as? [[String: Any]] ?? []).compactMap { $0["id"] as? String }
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

    func removeProvider(_ id: String) {
        Task.detached {
            let r = Engine.call(["op": "providerRemove", "id": id])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? "Removed \(id)"
                SettingsState.shared.load()
            }
        }
    }

    func addProvider(id: String, kind: String, baseUrl: String, model: String, key: String) {
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
        Task.detached {
            let r = Engine.call(["op": "providerAdd", "provider": provider])
            await MainActor.run {
                SettingsState.shared.status = (r["error"] as? String) ?? "Added \(id)"
                SettingsState.shared.load()
            }
        }
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

private struct PaneSection<Content: View>: View {
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

    var body: some View {
        PaneSection(title: "Launcher") {
            HStack {
                Text("Hotkey")
                TextField("Alt+Space", text: $state.hotkeyDraft)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 180)
                Button("Save") { state.set("hotkey", state.hotkeyDraft) }
            }
            Text("Examples: Alt+Space, Cmd+Space, Ctrl+Alt+K, F18. With the hyper key on, Hyper+Space works too.")
                .font(.caption).foregroundStyle(.secondary)
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
            Toggle("Caps Lock is a hyper key (⌃⌥⌘⇧ held, launcher when tapped) — restart to apply", isOn: Binding(get: { state.bool("hyperkey") }, set: { state.set("hyperkey", $0) }))
            Toggle("Expand snippets while typing in any app (keywords starting with a symbol, like ;sig)", isOn: Binding(get: { state.bool("snippet_expansion") }, set: { state.set("snippet_expansion", $0) }))
        }
        PaneSection(title: "Search") {
            Toggle("Include Shortcuts.app shortcuts", isOn: Binding(get: { state.bool("shortcuts") }, set: { state.set("shortcuts", $0) }))
            Toggle("Show the next meeting at the top of the launcher", isOn: Binding(get: { state.bool("calendar") }, set: { state.set("calendar", $0) }))
            Toggle("Run scripts through your login shell (Homebrew, nvm… on PATH)", isOn: Binding(get: { state.bool("script_login_shell") }, set: { state.set("script_login_shell", $0) }))
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
        Preset(id: "openrouter", kind: "openai", baseUrl: "https://openrouter.ai/api/v1", model: "", needsKey: true),
        Preset(id: "groq", kind: "openai", baseUrl: "https://api.groq.com/openai/v1", model: "", needsKey: true),
        Preset(id: "ollama", kind: "openai", baseUrl: "http://localhost:11434/v1", model: "llama3.2", needsKey: false),
        Preset(id: "lmstudio", kind: "openai", baseUrl: "http://localhost:1234/v1", model: "", needsKey: false),
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

    var body: some View {
        PaneSection(title: "Providers") {
            if state.providers.isEmpty {
                Text("No providers yet. Add one below.").foregroundStyle(.secondary)
            }
            ForEach(state.providers) { p in
                HStack {
                    Image(systemName: p.id == state.string("default_provider") ? "star.fill" : "star")
                        .foregroundStyle(p.id == state.string("default_provider") ? Color.accentColor : Color.secondary)
                        .onTapGesture { state.set("default_provider", p.id) }
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
                    Button(role: .destructive) { state.removeProvider(p.id) } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless)
                }
                .padding(.vertical, 2)
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
            Button("Add Provider") {
                state.addProvider(id: newId.trimmingCharacters(in: .whitespaces), kind: kind, baseUrl: baseUrl, model: model, key: key)
                key = ""
            }
            .disabled(newId.trimmingCharacters(in: .whitespaces).isEmpty || model.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }
}

private struct ExtensionsPane: View {
    @Bindable var state: SettingsState
    @State private var refreshing = false

    var body: some View {
        PaneSection(title: "MCP servers") {
            if state.mcp.isEmpty {
                Text("None configured. Add [[mcp_servers]] to config.toml — their tools become launcher commands and AI chat tools.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(state.mcp, id: \.self) { Label($0, systemImage: "powerplug") }
            }
            HStack {
                Button("Edit config.toml") {
                    NSWorkspace.shared.open(URL(fileURLWithPath: state.configDir).appendingPathComponent("config.toml"))
                }
                Button(refreshing ? "Refreshing…" : "Refresh MCP Tools") {
                    refreshing = true
                    Task.detached {
                        let r = Engine.call(["op": "mcpRefresh"])
                        await MainActor.run {
                            refreshing = false
                            let n = r["tools"] as? Int ?? 0
                            let errors = r["errors"] as? [String] ?? []
                            SettingsState.shared.status = errors.isEmpty ? "\(n) MCP tools" : errors.joined(separator: "\n")
                        }
                    }
                }
                .disabled(refreshing)
            }
        }
        PaneSection(title: "Script commands") {
            Text("Executable files in the scripts folder, with Raycast-style headers. `@opencast.mode list` makes a Script Filter.")
                .foregroundStyle(.secondary)
            Button("Open Scripts Folder") {
                NSWorkspace.shared.open(URL(fileURLWithPath: state.configDir).appendingPathComponent("scripts"))
            }
        }
        PaneSection(title: "Sync") {
            if state.synced {
                Label("config.toml, scripts, notes, layouts and pins sync through your synced folder", systemImage: "checkmark.icloud")
            } else {
                Text("Keep config.toml, scripts, notes, layouts and pins in iCloud Drive so every Mac shares them. Clipboard history, chats and usage stay on this Mac.")
                    .foregroundStyle(.secondary)
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
                detail: "Click Text (on-screen OCR) and attaching screen text to chats",
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
            HStack {
                Text("Releases from GitHub repo")
                TextField("owner/repo", text: $state.updateRepoDraft)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 220)
                Button("Save") { state.set("update_repo", state.updateRepoDraft) }
            }
            Toggle("Check for updates once a week", isOn: Binding(get: { state.bool("check_updates") }, set: { state.set("check_updates", $0) }))
        }
    }
}
