import AppKit
import SwiftUI

/// "Keyboard Shortcuts": every menu shortcut of the app you were using, grouped
/// by menu, in a floating sheet. Type to filter, click a row to run it, Esc
/// closes.
@MainActor
final class CheatSheet: NSObject, NSWindowDelegate {
    static let shared = CheatSheet()
    private var panel: NSPanel?
    let state = CheatSheetState()

    func show(for app: NSRunningApplication) {
        let pid = app.processIdentifier
        let name = app.localizedName ?? "App"
        Task.detached {
            let items = MenuSearch.collect(pid: pid).filter { !$0.shortcut.isEmpty }
            await MainActor.run {
                guard !items.isEmpty else {
                    Model.shared.show()
                    Model.shared.flash("\(name) has no menu shortcuts OpenCast can read", error: true, thenHide: false)
                    return
                }
                CheatSheet.shared.present(app: app, name: name, items: items)
            }
        }
    }

    private func present(app: NSRunningApplication, name: String, items: [MenuSearch.Item]) {
        state.app = app
        state.appName = name
        state.items = items
        state.filter = ""
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.main
        let vf = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1200, height: 800)
        let size = NSSize(width: min(vf.width * 0.86, 1180), height: min(vf.height * 0.8, 760))
        let rect = NSRect(x: vf.midX - size.width / 2, y: vf.midY - size.height / 2, width: size.width, height: size.height)
        let p = panel ?? {
            let p = KeyPanel(contentRect: rect, styleMask: [.titled, .fullSizeContentView, .closable], backing: .buffered, defer: false)
            p.titleVisibility = .hidden
            p.titlebarAppearsTransparent = true
            p.isMovableByWindowBackground = true
            p.level = .floating
            p.isReleasedWhenClosed = false
            p.hidesOnDeactivate = true
            p.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
            p.contentView = NSHostingView(rootView: CheatSheetView(state: state))
            p.delegate = self
            return p
        }()
        p.setFrame(rect, display: true)
        panel = p
        NSApp.activate(ignoringOtherApps: true)
        p.makeKeyAndOrderFront(nil)
    }

    func close() {
        panel?.orderOut(nil)
    }

    func windowDidResignKey(_ notification: Notification) {
        close()
    }

    /// Esc closes even when the filter field has focus.
    final class KeyPanel: NSPanel {
        override var canBecomeKey: Bool { true }
        override func cancelOperation(_ sender: Any?) {
            CheatSheet.shared.close()
        }
    }
}

@MainActor
@Observable
final class CheatSheetState {
    var app: NSRunningApplication?
    var appName = ""
    var items: [MenuSearch.Item] = []
    var filter = ""

    /// (menu, rows) in menu-bar order.
    var groups: [(String, [MenuSearch.Item])] {
        let q = filter.lowercased().trimmingCharacters(in: .whitespaces)
        var order: [String] = []
        var map: [String: [MenuSearch.Item]] = [:]
        for item in items {
            if !q.isEmpty, !(item.path + " " + item.title).lowercased().contains(q) { continue }
            let menu = item.path.components(separatedBy: " › ").first ?? item.path
            if map[menu] == nil { order.append(menu) }
            map[menu, default: []].append(item)
        }
        return order.map { ($0, map[$0] ?? []) }
    }

    func run(_ item: MenuSearch.Item) {
        CheatSheet.shared.close()
        MenuSearch.press(item, in: app)
    }
}

struct CheatSheetView: View {
    @Bindable var state: CheatSheetState

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("\(state.appName) Shortcuts").font(.system(size: 17, weight: .semibold))
                Spacer()
                TextField("Filter", text: $state.filter)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 220)
            }
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 260), spacing: 22, alignment: .top)], alignment: .leading, spacing: 18) {
                    ForEach(state.groups, id: \.0) { menu, rows in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(menu).font(.system(size: 12, weight: .bold)).foregroundStyle(.secondary)
                            ForEach(Array(rows.enumerated()), id: \.offset) { _, item in
                                Button { state.run(item) } label: {
                                    HStack(spacing: 8) {
                                        Text(item.title).font(.system(size: 12)).lineLimit(1)
                                        Spacer(minLength: 8)
                                        Text(item.shortcut)
                                            .font(.system(size: 12, weight: .medium, design: .rounded))
                                            .foregroundStyle(.secondary)
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .help(item.path)
                            }
                        }
                    }
                }
                .padding(.bottom, 8)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .background(.regularMaterial)
    }
}
