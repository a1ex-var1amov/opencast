import AppKit
import SwiftUI

extension Notification.Name {
    static let debugRefresh = Notification.Name("OpenCastDebugRefresh")
}

final class DebugPanelController: NSObject, NSWindowDelegate {
    static let shared = DebugPanelController()
    private var window: NSWindow?
    private var timer: Timer?

    func show() {
        dispatchPrecondition(condition: .onQueue(.main))
        if window == nil {
            let w = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 720, height: 560),
                styleMask: [.titled, .closable, .resizable, .miniaturizable],
                backing: .buffered,
                defer: false
            )
            w.title = "OpenCast Debug"
            w.isReleasedWhenClosed = false
            w.delegate = self
            w.contentView = NSHostingView(rootView: DebugRoot())
            w.center()
            window = w
        }
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        start()
    }

    func windowWillClose(_ notification: Notification) {
        timer?.invalidate()
        timer = nil
    }

    private func start() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            NotificationCenter.default.post(name: .debugRefresh, object: nil)
        }
    }
}

private struct DebugRoot: View {
    @State private var filter = ""
    @State private var status = ""
    @State private var level = ""
    @State private var rss = ""
    @State private var cpu = ""
    @State private var logDir = ""
    @State private var lines: [String] = []
    @State private var ops: [String] = []
    @State private var slow: [String] = []
    @State private var hotkeys: [String] = []
    @State private var problems: [String] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                TextField("Filter", text: $filter)
                    .textFieldStyle(.roundedBorder)
                Button("Export") { export() }
                Button("Refresh") { reload() }
            }
            Text("\(level) · RSS \(rss) MB · CPU \(cpu)% · \(StayAwakeController.shared.debugLine())")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(logDir).font(.caption2).foregroundStyle(.secondary).textSelection(.enabled)
            if !status.isEmpty {
                Text(status).font(.caption).foregroundStyle(.red)
            }
            List {
                Section("Slow (10 min)") {
                    if slow.isEmpty { Text("None").foregroundStyle(.secondary) }
                    ForEach(Array(slow.enumerated()), id: \.offset) { _, row in
                        Text(row).font(.system(.caption, design: .monospaced))
                    }
                }
                Section("Operations") {
                    if ops.isEmpty { Text("None yet").foregroundStyle(.secondary) }
                    ForEach(Array(ops.enumerated()), id: \.offset) { _, row in
                        Text(row).font(.system(.caption, design: .monospaced))
                    }
                }
                Section("Hotkeys") {
                    if hotkeys.isEmpty { Text("None registered").foregroundStyle(.secondary) }
                    ForEach(Array(hotkeys.enumerated()), id: \.offset) { _, row in
                        Text(row).font(.caption)
                    }
                    ForEach(Array(problems.enumerated()), id: \.offset) { _, row in
                        Text(row).font(.caption).foregroundStyle(.red)
                    }
                }
                Section("Events") {
                    let shown = filtered
                    if shown.isEmpty { Text("No lines").foregroundStyle(.secondary) }
                    ForEach(Array(shown.enumerated()), id: \.offset) { _, line in
                        Text(line).font(.system(.caption2, design: .monospaced)).textSelection(.enabled)
                    }
                }
            }
        }
        .padding(12)
        .frame(minWidth: 640, minHeight: 420)
        .onAppear { reload() }
        .onReceive(NotificationCenter.default.publisher(for: .debugRefresh)) { _ in reload() }
    }

    private var filtered: [String] {
        let q = filter.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return lines }
        return lines.filter { $0.lowercased().contains(q) }
    }

    private func reload() {
        let raw = Engine.call(["op": "debugSnapshot", "limit": 200])
        level = raw["level"] as? String ?? ""
        rss = raw["rssMb"].map { "\($0)" } ?? "?"
        if let n = raw["cpu"] as? Double {
            cpu = String(format: "%.1f", n)
        } else if let n = raw["cpu"] as? Int {
            cpu = "\(n)"
        } else {
            cpu = "?"
        }
        logDir = raw["logDir"] as? String ?? ""
        lines = raw["lines"] as? [String] ?? []
        hotkeys = raw["hotkeys"] as? [String] ?? []
        problems = raw["hotkeyProblems"] as? [String] ?? []
        ops = (raw["ops"] as? [[String: Any]] ?? []).map { row in
            let name = row["name"] as? String ?? "?"
            let n = row["n"] as? Int ?? 0
            let p50 = row["p50"] as? Int ?? 0
            let p95 = row["p95"] as? Int ?? 0
            let p99 = row["p99"] as? Int ?? 0
            return "\(name)  n=\(n)  p50=\(p50)ms  p95=\(p95)ms  p99=\(p99)ms"
        }
        slow = (raw["slow"] as? [[String: Any]] ?? []).map { row in
            let name = row["name"] as? String ?? "?"
            let ms = row["ms"] as? Int ?? 0
            let budget = row["budget"] as? Int ?? 0
            return "\(name)  \(ms)ms  budget \(budget)ms"
        }
    }

    private func export() {
        let raw = Engine.call(["op": "debugExport"])
        if let path = raw["path"] as? String {
            status = ""
            NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
        } else {
            status = raw["error"] as? String ?? "Export failed"
        }
    }
}
