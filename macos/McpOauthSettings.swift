import AppKit
import SwiftUI

/// MCP row for Settings (id + whether OAuth Sign in applies).
struct McpServerRow: Identifiable, Sendable {
    let id: String
    let oauth: Bool
    let signedIn: Bool
}

/// MCP list + Sign in / Refresh. Kept out of SettingsWindow to limit merge conflicts.
struct McpServersPaneSection: View {
    @Bindable var state: SettingsState
    @State private var refreshing = false
    @State private var signingIn: String?

    var body: some View {
        PaneSection(title: "MCP servers") {
            if state.mcp.isEmpty {
                Text("None configured. Add [[mcp_servers]] to config.toml — their tools become launcher commands and AI chat tools.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(state.mcp) { row in
                    HStack {
                        Label(row.id, systemImage: "powerplug")
                        Spacer()
                        if row.oauth {
                            Text(row.signedIn ? "Signed in" : "Not signed in")
                                .foregroundStyle(.secondary)
                                .font(.caption)
                            if row.signedIn {
                                Button("Sign out") {
                                    Task.detached {
                                        _ = Engine.call(["op": "mcpOauthSignOut", "id": row.id])
                                        await MainActor.run {
                                            SettingsState.shared.status = "Signed out of \(row.id)"
                                            SettingsState.shared.load()
                                        }
                                    }
                                }
                            }
                            Button(signingIn == row.id ? "Signing in…" : (row.signedIn ? "Re-auth…" : "Sign in…")) {
                                signingIn = row.id
                                Task.detached {
                                    let r = Engine.call(["op": "mcpOauthSignIn", "id": row.id])
                                    await MainActor.run {
                                        signingIn = nil
                                        if let err = r["error"] as? String {
                                            SettingsState.shared.status = err
                                        } else {
                                            SettingsState.shared.status = "Signed in to \(row.id)"
                                            SettingsState.shared.load()
                                        }
                                    }
                                }
                            }
                            .disabled(signingIn != nil)
                        }
                    }
                }
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
                            SettingsState.shared.load()
                        }
                    }
                }
                .disabled(refreshing)
            }
        }
    }
}
