import AppKit
import SwiftUI

struct LauncherView: View {
    @Bindable var model: Model
    @FocusState private var focused: Bool
    @Environment(\.colorScheme) private var systemScheme
    @State private var pointer = CGPoint(x: -1_000_000, y: -1_000_000)
    @State private var reduceTransparency = Theme.reduceTransparency

    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }
    private var isSpotlight: Bool { model.presentation == "spotlight" }

    var body: some View {
        Group {
            switch model.mode {
            case .search:
                search
            case .actions(let command):
                ActionsPane(model: model, command: command)
            case .args(let command):
                ArgsForm(model: model, command: command)
            case .output(let title, let text):
                OutputPane(title: title, text: text)
            case .chat:
                ChatView(model: model)
            }
        }
        .foregroundStyle(Theme.primary(scheme))
        .background { panelFill }
        .environment(\.themePreference, model.themePreference)
        // Accessory apps: nil preferredColorScheme does not track day/night — pin to resolved scheme.
        .preferredColorScheme(scheme)
        .overlay(alignment: .bottom) {
            if let text = model.toastText {
                Text(text)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(model.toastIsError ? Color.white : Theme.island(scheme))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(model.toastIsError ? Theme.danger : Theme.primary(scheme), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .padding(.bottom, 50)
            }
        }
        .clipShape(panelShape)
        .ignoresSafeArea()
        .overlay { panelShape.stroke(Theme.stroke(scheme), lineWidth: 0.5) }
        .compositingGroup()
        .onAppear { selectField() }
        .onChange(of: model.focusTick) { _, _ in selectField() }
        .onChange(of: model.ready) { _, ready in
            if ready { model.search() }
        }
        .onChange(of: model.query) { _, _ in
            model.selected = 0
            model.listStart = 0
            model.noteContentChanged()
            if model.ready { model.search() }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
            reduceTransparency = Theme.reduceTransparency
        }
    }

    private var panelShape: AnyShape {
        if isSpotlight {
            AnyShape(RoundedRectangle(cornerRadius: Theme.spotlightRadius, style: .continuous))
        } else {
            AnyShape(UnevenRoundedRectangle(
                bottomLeadingRadius: Theme.islandBottomRadius,
                bottomTrailingRadius: Theme.islandBottomRadius
            ))
        }
    }

    @ViewBuilder private var panelFill: some View {
        if reduceTransparency {
            Theme.island(scheme)
        } else {
            Rectangle().fill(.ultraThinMaterial)
        }
    }

    private var search: some View {
        VStack(spacing: 0) {
            field
            if !model.clicking {
                list.frame(height: model.listHeight)
            }
            footer
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            TextField(
                "",
                text: Binding(
                    get: { model.clicking ? model.seekQuery : model.query },
                    set: { value in
                        if model.clicking {
                            model.seekQuery = value
                            model.seekIndex = 0
                        } else {
                            model.query = value
                        }
                    }
                ),
                prompt: Text(model.clicking ? "Type what you see…" : "Search apps and commands, or ask AI…")
                    .foregroundStyle(Theme.tertiary(scheme))
            )
            .textFieldStyle(.plain)
            .font(.system(size: 19))
            .foregroundStyle(Theme.primary(scheme))
            .tint(Theme.accent)
            .focusEffectDisabled()
            .focused($focused)
            .autocorrectionDisabled()
            if model.busy {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 16, height: 16)
            }
        }
        .padding(.horizontal, 18)
        .frame(height: 58)
    }

    private var list: some View {
        Group {
            if model.rows.isEmpty {
                Text("No results")
                    .foregroundStyle(Theme.secondary(scheme))
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .padding(.vertical, 6)
            } else {
                let start = model.listStart
                let visible = Array(model.rows.enumerated()).dropFirst(start).prefix(8)
                VStack(spacing: 0) {
                    ForEach(Array(visible), id: \.element.id) { index, row in
                        rowView(row, index: index)
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
            }
        }
    }

    private func rowView(_ row: Model.Row, index: Int) -> some View {
        HStack(spacing: 12) {
            switch row {
            case .ask:
                Image(systemName: "sparkles")
                    .font(.system(size: 16, weight: .semibold))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(Theme.accent)
                    .frame(width: 28, height: 28)
                Text("Ask AI “\(model.trimmedQuery)”")
                    .lineLimit(1)
                Spacer(minLength: 8)
                Text("Tab")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
            case .command(let command):
                icon(command)
                Text(command.title)
                    .font(.system(size: 14, weight: .medium))
                    .lineLimit(1)
                if let subtitle = command.subtitle {
                    Text(subtitle)
                        .foregroundStyle(Theme.secondary(scheme))
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    Spacer(minLength: 0)
                }
                if let hotkey = command.hotkey {
                    Text(hotkey)
                        .font(.system(size: 12, design: .monospaced))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                        .foregroundStyle(Theme.secondary(scheme))
                }
                Text(command.kind.label)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 44)
        .background {
            if index == model.selected {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Theme.accent.opacity(scheme == .light ? 0.14 : 0.22))
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .stroke(Theme.accent.opacity(0.35), lineWidth: 1)
                    )
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .onHover { inside in
            guard inside else { return }
            let loc = NSEvent.mouseLocation
            let dx = loc.x - pointer.x
            let dy = loc.y - pointer.y
            // A stationary cursor must not grab the highlight when the window slides.
            guard dx * dx + dy * dy > 1 else { return }
            pointer = loc
            model.selected = index
        }
        .onTapGesture { model.activate(index) }
    }

    @ViewBuilder private func icon(_ command: Command) -> some View {
        if command.kind == .clipboard,
           let image = ClipboardHistory.shared.image(for: command.id)
        {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 28, height: 28)
                .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
        } else if let path = fileIconPath(command), let image = model.appIcons[path] {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 28, height: 28)
        } else if let name = Theme.resolvedSymbol(for: command) {
            Image(systemName: name)
                .font(.system(size: 17, weight: .medium))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(Theme.primary(scheme))
                .frame(width: 28, height: 28)
        } else {
            Text(command.id == "window:center-third" ? "\u{2153}" : (command.icon ?? ""))
                .font(.system(size: 17))
                .frame(width: 28, height: 28)
        }
    }

    private func fileIconPath(_ command: Command) -> String? {
        switch command.kind {
        case .app: String(command.id.dropFirst(4))
        case .file: String(command.id.dropFirst(5))
        default: nil
        }
    }

    private var footer: some View {
        HStack {
            if model.searchingFiles {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.mini)
                    Text("Searching files…")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.tertiary(scheme))
                }
            } else {
                Text("OpenCast")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
            }
            Spacer()
            HStack(spacing: 14) {
                if model.clicking {
                    if !model.seekCount.isEmpty {
                        Text(model.seekCount)
                    }
                    hint("Back", "Esc")
                    hint("Click", "Return")
                } else {
                    if let verb = model.verb {
                        hint(verb, "Return")
                        if !model.askSelected { hint("Ask AI", "Tab") }
                    }
                    hint("Actions", "⌘K")
                    hint("Click", "`")
                }
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
    }

    private func hint(_ label: String, _ key: String) -> some View {
        HStack(spacing: 4) {
            Text(label)
            Text(key)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .font(.system(size: 12))
        .foregroundStyle(Theme.secondary(scheme))
    }

    private func selectField() {
        guard case .search = model.mode else { return }
        focused = true
        DispatchQueue.main.async {
            let appearance = Theme.nsAppearance(preference: model.themePreference)
            if let editor = NSApp.keyWindow?.fieldEditor(false, for: nil) as? NSTextView {
                if let appearance {
                    editor.appearance = appearance
                    appearance.performAsCurrentDrawingAppearance {
                        editor.textColor = .labelColor
                        editor.insertionPointColor = .labelColor
                    }
                }
                editor.selectAll(nil)
            }
        }
    }
}

private struct ActionsPane: View {
    @Bindable var model: Model
    var command: Command
    @Environment(\.colorScheme) private var systemScheme
    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }

    private var actions: [Model.ActionRow] { model.actionRows(for: command) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                BackButton { model.backToSearch(clearQuery: false) }
                Text(command.title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                Spacer()
                Text("Actions")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
            }
            .padding(.horizontal, 14)
            .frame(height: 52)
            ForEach(Array(actions.enumerated()), id: \.element.id) { index, action in
                let on = index == model.selected
                HStack {
                    Text(action.title)
                        .font(.system(size: 14))
                    Spacer()
                }
                .padding(.horizontal, 14)
                .frame(height: 36)
                .background(on ? Theme.accent.opacity(0.18) : Color.clear)
                .contentShape(Rectangle())
                .onTapGesture { model.runAction(action.id, for: command) }
            }
            Spacer(minLength: 0)
            HStack {
                Text("Esc back")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
                Spacer()
                Text("Return")
                    .font(.system(size: 12))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                    .foregroundStyle(Theme.secondary(scheme))
            }
            .padding(.horizontal, 14)
            .frame(height: 38)
        }
        .frame(height: 52 + CGFloat(min(actions.count, 8)) * 36 + 38)
    }
}

struct BackButton: View {
    var action: () -> Void
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        Button(action: action) {
            Image(systemName: "chevron.left")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.secondary(scheme))
                .frame(width: 26, height: 26)
                .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Back")
    }
}

struct HintBar: View {
    var left: String
    var hints: [(String, String)]
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        HStack {
            Text(left)
                .font(.system(size: 12))
                .foregroundStyle(Theme.tertiary(scheme))
            Spacer()
            HStack(spacing: 14) {
                ForEach(Array(hints.enumerated()), id: \.offset) { _, hint in
                    HStack(spacing: 4) {
                        Text(hint.0)
                        Text(hint.1)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 5))
                    }
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.secondary(scheme))
                }
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
    }
}

private struct ArgsForm: View {
    @Bindable var model: Model
    var command: Command
    @FocusState private var focus: Int?
    @Environment(\.colorScheme) private var systemScheme
    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                BackButton { model.backToSearch(clearQuery: false) }
                argIcon
                Text(command.title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    .layoutPriority(1)
                ForEach(command.arguments.indices, id: \.self) { index in
                    field(index)
                }
                if model.busy {
                    ProgressView()
                        .controlSize(.small)
                        .frame(width: 16, height: 16)
                }
            }
            .padding(.horizontal, 18)
            .frame(minHeight: 58)
            .overlay(alignment: .bottom) { hairline }
            hint
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            HintBar(left: "OpenCast", hints: [("Run", "↵"), ("Next field", "⇥"), ("Back", "esc")])
        }
        .onAppear { focus = model.argField }
        .onChange(of: model.focusTick) { _, _ in focus = model.argField }
        .onChange(of: focus) { _, value in
            if let value { model.argField = value }
        }
    }

    private var hint: some View {
        let which = command.arguments.count == 1 ? "the field" : "the fields"
        return (Text("Fill in \(which) above and press Enter to run ") + Text(command.title).bold() + Text("."))
            .font(.system(size: 14))
            .foregroundStyle(Theme.secondary(scheme))
            .multilineTextAlignment(.center)
            .padding(24)
    }

    @ViewBuilder private var argIcon: some View {
        if let name = Theme.resolvedSymbol(for: command) {
            Image(systemName: name)
                .font(.system(size: 17, weight: .medium))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(Theme.primary(scheme))
                .frame(width: 22, height: 22)
        } else if let icon = command.icon, !icon.isEmpty {
            Text(icon)
                .font(.system(size: 17))
        }
    }

    @ViewBuilder private func field(_ index: Int) -> some View {
        let spec = command.arguments[index]
        let placeholder = spec.placeholder + (spec.optional ? " (optional)" : "")
        let prompt = Text(placeholder).foregroundStyle(Theme.tertiary(scheme))
        Group {
            if spec.secure {
                SecureField("", text: $model.argDrafts[index], prompt: prompt)
            } else {
                TextField("", text: $model.argDrafts[index], prompt: prompt)
            }
        }
        .textFieldStyle(.plain)
        .font(.system(size: 14))
        .foregroundStyle(Theme.primary(scheme))
        .tint(Theme.accent)
        .focused($focus, equals: index)
        .focusEffectDisabled()
        .autocorrectionDisabled()
        .padding(.horizontal, 10)
        .frame(minWidth: 90, maxWidth: .infinity)
        .frame(height: 32)
        .background(scheme == .dark ? Color.white.opacity(0.06) : Color.black.opacity(0.04), in: RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(focus == index ? Theme.accent : Theme.stroke(scheme).opacity(0.7), lineWidth: 1)
        )
    }

    private var hairline: some View {
        Rectangle().fill(Theme.stroke(scheme).opacity(0.45)).frame(height: 0.5)
    }
}

private struct OutputPane: View {
    var title: String
    var text: String
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                BackButton { Model.shared.backToSearch(clearQuery: false) }
                Text(title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 18)
            .frame(height: 58)
            .overlay(alignment: .bottom) {
                Rectangle().fill(Theme.stroke(scheme).opacity(0.45)).frame(height: 0.5)
            }
            ScrollView {
                Text(text.isEmpty ? "(no output)" : text)
                    .font(.system(size: 13, design: .monospaced))
                    .foregroundStyle(text.isEmpty ? Theme.tertiary(scheme) : Theme.primary(scheme))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 16)
            }
            .frame(maxHeight: .infinity)
            HintBar(left: "OpenCast", hints: [("Copy", "↵"), ("Back", "esc")])
        }
    }
}
