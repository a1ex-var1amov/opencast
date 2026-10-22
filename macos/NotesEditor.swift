import AppKit
import SwiftUI

/// A small notes window: list on the left, title + body on the right.
/// Saves as you type (to `notes.json` through the engine). ⌘N new, ⌘⌫ delete.
@MainActor
final class NotesEditorController: NSObject, NSWindowDelegate {
    static let shared = NotesEditorController()
    private var window: NSWindow?
    let state = NotesEditorState()

    func show(noteId: String? = nil) {
        state.reload(select: noteId)
        if window == nil {
            let w = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 760, height: 500),
                styleMask: [.titled, .closable, .resizable, .miniaturizable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            w.title = "Notes"
            w.titlebarAppearsTransparent = true
            w.isReleasedWhenClosed = false
            w.center()
            w.setFrameAutosaveName("OpenCastNotes")
            w.contentView = NSHostingView(rootView: NotesEditorView(state: state))
            w.delegate = self
            window = w
        }
        applyPin()
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        state.flush()
    }

    /// Keep the notes window above other apps (remembered).
    func togglePin() {
        state.pinned.toggle()
        UserDefaults.standard.set(state.pinned, forKey: "OpenCastNotesPinned")
        applyPin()
    }

    private func applyPin() {
        window?.level = state.pinned ? .floating : .normal
        window?.collectionBehavior = state.pinned ? [.canJoinAllSpaces, .fullScreenAuxiliary] : []
    }
}

/// One note in its own small floating window ("sticky"). Several can be open.
@MainActor
final class StickyNotes: NSObject, NSWindowDelegate {
    static let shared = StickyNotes()
    private var windows: [String: NSWindow] = [:]

    func open(id: String, title: String, body: String) {
        if let w = windows[id] {
            w.makeKeyAndOrderFront(nil)
            return
        }
        let state = StickyState(id: id, title: title, body: body)
        let w = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 260),
            styleMask: [.titled, .closable, .resizable, .utilityWindow, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        w.title = title.isEmpty ? "Note" : title
        w.titlebarAppearsTransparent = true
        w.isFloatingPanel = true
        w.level = .floating
        w.hidesOnDeactivate = false
        w.isReleasedWhenClosed = false
        w.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        w.setFrameAutosaveName("OpenCastSticky-\(id)")
        if w.frame.origin == .zero {
            // Cascade new stickies from the top right.
            let vf = NSScreen.main?.visibleFrame ?? .zero
            let n = CGFloat(windows.count)
            w.setFrameTopLeftPoint(NSPoint(x: vf.maxX - 320 - n * 24, y: vf.maxY - 20 - n * 24))
        }
        w.contentView = NSHostingView(rootView: StickyView(state: state))
        w.delegate = self
        windows[id] = w
        NSApp.activate(ignoringOtherApps: true)
        w.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        guard let w = notification.object as? NSWindow, let id = windows.first(where: { $0.value === w })?.key else { return }
        if let st = (w.contentView as? NSHostingView<StickyView>)?.rootView.state {
            if (st.title + st.body).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                _ = Engine.call(["op": "noteDelete", "id": st.id]) // an untouched sticky leaves nothing behind
            } else {
                st.flush()
            }
        }
        windows[id] = nil
        NotesEditorController.shared.state.reload()
    }
}

@MainActor
@Observable
final class StickyState {
    let id: String
    var title: String
    var body: String
    var preview = false
    @ObservationIgnored private var saveTask: Task<Void, Never>?

    init(id: String, title: String, body: String) {
        self.id = id
        self.title = title
        self.body = body
    }

    func edited() {
        saveTask?.cancel()
        saveTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 500_000_000)
            guard !Task.isCancelled else { return }
            self.flush()
        }
    }

    func flush() {
        saveTask?.cancel()
        _ = Engine.call(["op": "noteUpdate", "id": id, "title": title, "body": body])
    }
}

struct StickyView: View {
    @Bindable var state: StickyState
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Spacer()
                Button { state.preview.toggle() } label: {
                    Image(systemName: state.preview ? "pencil" : "eye")
                }
                .buttonStyle(.plain)
                .keyboardShortcut("m", modifiers: [.command, .shift])
                .help(state.preview ? "Edit (⌘⇧M)" : "Preview Markdown (⌘⇧M)")
            }
            if state.preview {
                ScrollView { MarkdownText(text: state.body, scheme: scheme).frame(maxWidth: .infinity, alignment: .leading) }
            } else {
                TextEditor(text: $state.body)
                    .font(.system(size: 13))
                    .scrollContentBackground(.hidden)
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
        .padding(.top, 24)
        .background(Color.yellow.opacity(scheme == .dark ? 0.12 : 0.18))
        .onChange(of: state.body) { _, _ in state.edited() }
    }
}

struct NoteRow: Identifiable, Equatable {
    var id: String
    var title: String
    var body: String
    var updated: Double

    var label: String {
        if !title.trimmingCharacters(in: .whitespaces).isEmpty { return title }
        let first = body.split(separator: "\n").first.map(String.init) ?? ""
        return first.isEmpty ? "New Note" : first
    }
}

@MainActor
@Observable
final class NotesEditorState {
    var notes: [NoteRow] = []
    var selectedId: String?
    var search = ""
    var title = ""
    var body = ""
    /// Floating above other windows.
    var pinned = UserDefaults.standard.bool(forKey: "OpenCastNotesPinned")
    /// Rendered Markdown instead of the editor.
    var preview = false
    @ObservationIgnored private var saveTask: Task<Void, Never>?
    @ObservationIgnored private var dirty = false

    var filtered: [NoteRow] {
        let q = search.lowercased().trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return notes }
        return notes.filter { ($0.title + " " + $0.body).lowercased().contains(q) }
    }

    func reload(select id: String? = nil) {
        let raw = Engine.call(["op": "notesList"])["notes"] as? [[String: Any]] ?? []
        notes = raw.compactMap { n in
            guard let id = n["id"] as? String else { return nil }
            return NoteRow(
                id: id,
                title: n["title"] as? String ?? "",
                body: n["body"] as? String ?? "",
                updated: (n["updated"] as? Double) ?? Double(n["updated"] as? Int ?? 0)
            )
        }
        open(id ?? selectedId ?? notes.first?.id)
    }

    func open(_ id: String?) {
        flush()
        selectedId = id
        let n = notes.first { $0.id == id }
        title = n?.title ?? ""
        body = n?.body ?? ""
    }

    /// Called when the fields change; saves only real edits.
    func edited() {
        let saved = notes.first { $0.id == selectedId }
        if saved?.title == title, saved?.body == body { return }
        if saved == nil, (title + body).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return }
        scheduleSave()
    }

    func newNote() {
        flush()
        let r = Engine.call(["op": "noteUpdate", "id": "", "title": "", "body": " "])
        reload(select: r["id"] as? String)
        body = ""
    }

    func delete() {
        guard let id = selectedId else { return }
        saveTask?.cancel()
        dirty = false
        _ = Engine.call(["op": "noteDelete", "id": id])
        selectedId = nil
        reload()
    }

    private func scheduleSave() {
        dirty = true
        saveTask?.cancel()
        saveTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 500_000_000)
            guard !Task.isCancelled else { return }
            self.flush()
        }
    }

    func flush() {
        guard dirty else { return }
        dirty = false
        let (t, b) = (title, body)
        if let id = selectedId {
            _ = Engine.call(["op": "noteUpdate", "id": id, "title": t, "body": b])
            if let i = notes.firstIndex(where: { $0.id == id }) {
                notes[i].title = t
                notes[i].body = b
                // Most recently edited first, like the engine.
                let n = notes.remove(at: i)
                notes.insert(n, at: 0)
            }
        } else if !(t + b).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let r = Engine.call(["op": "noteUpdate", "id": "", "title": t, "body": b])
            selectedId = r["id"] as? String
            reload(select: selectedId)
        }
    }
}

struct NotesEditorView: View {
    @Bindable var state: NotesEditorState
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        NavigationSplitView {
            List(selection: Binding(get: { state.selectedId }, set: { state.open($0) })) {
                ForEach(state.filtered) { n in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(n.label).font(.system(size: 13, weight: .medium)).lineLimit(1)
                        Text(n.body.replacingOccurrences(of: "\n", with: " "))
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .tag(Optional(n.id))
                }
            }
            .searchable(text: $state.search, placement: .sidebar)
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
            .toolbar {
                ToolbarItem {
                    Button(action: state.newNote) { Image(systemName: "square.and.pencil") }
                        .keyboardShortcut("n", modifiers: .command)
                        .help("New Note (⌘N)")
                }
            }
        } detail: {
            if state.selectedId == nil, state.notes.isEmpty {
                VStack(spacing: 10) {
                    Text("No notes yet").foregroundStyle(.secondary)
                    Button("New Note", action: state.newNote)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    TextField("Title", text: $state.title)
                        .textFieldStyle(.plain)
                        .font(.system(size: 20, weight: .semibold))
                    if state.preview {
                        ScrollView {
                            MarkdownText(text: state.body, scheme: scheme)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .textSelection(.enabled)
                        }
                    } else {
                        TextEditor(text: $state.body)
                            .font(.system(size: 14))
                            .scrollContentBackground(.hidden)
                    }
                }
                .padding(16)
                .onChange(of: state.title) { _, _ in state.edited() }
                .onChange(of: state.body) { _, _ in state.edited() }
                .toolbar {
                    ToolbarItem {
                        Button { state.preview.toggle() } label: {
                            Image(systemName: state.preview ? "pencil" : "eye")
                        }
                        .keyboardShortcut("m", modifiers: [.command, .shift])
                        .help(state.preview ? "Edit (⌘⇧M)" : "Preview Markdown (⌘⇧M)")
                    }
                    ToolbarItem {
                        Button {
                            state.flush()
                            if let id = state.selectedId {
                                StickyNotes.shared.open(id: id, title: state.title, body: state.body)
                            }
                        } label: { Image(systemName: "rectangle.on.rectangle") }
                            .keyboardShortcut("o", modifiers: [.command, .shift])
                            .help("Open as a Floating Sticky (⌘⇧O)")
                            .disabled(state.selectedId == nil)
                    }
                    ToolbarItem {
                        Button { NotesEditorController.shared.togglePin() } label: {
                            Image(systemName: state.pinned ? "pin.fill" : "pin")
                        }
                        .keyboardShortcut("p", modifiers: [.command, .shift])
                        .help(state.pinned ? "Stop Floating (⌘⇧P)" : "Float Above Other Windows (⌘⇧P)")
                    }
                    ToolbarItem {
                        Button(role: .destructive, action: state.delete) { Image(systemName: "trash") }
                            .keyboardShortcut(.delete, modifiers: .command)
                            .help("Delete Note (⌘⌫)")
                            .disabled(state.selectedId == nil)
                    }
                }
            }
        }
        .frame(minWidth: 560, minHeight: 360)
    }
}
