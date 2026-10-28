import AppKit
import SwiftUI

struct LauncherView: View {
    @Bindable var model: Model
    @FocusState private var focused: Bool
    @Environment(\.colorScheme) private var systemScheme
    @State private var pointer = CGPoint(x: -1_000_000, y: -1_000_000)
    @State private var reduceTransparency = Theme.reduceTransparency

    // AX-2: five type roles + body scale drives island chrome via model.typeScale.
    @ScaledMetric(relativeTo: .title) private var fieldSize: CGFloat = 19
    @ScaledMetric(relativeTo: .headline) private var titleSize: CGFloat = 15
    @ScaledMetric(relativeTo: .body) private var bodySize: CGFloat = 14
    @ScaledMetric(relativeTo: .caption) private var metaSize: CGFloat = 12
    @ScaledMetric(relativeTo: .caption2) private var captionSize: CGFloat = 11
    @ScaledMetric(relativeTo: .body) private var iconSize: CGFloat = 17

    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }
    private var isSpotlight: Bool { model.presentation == "spotlight" }
    private var typeScale: CGFloat { bodySize / 14 }

    private func syncTypeScale() {
        let s = typeScale
        if abs(model.typeScale - s) > 0.01 {
            model.typeScale = s
            model.noteContentChanged()
        }
    }

    var body: some View {
        Group {
            switch model.mode {
            case .search, .list:
                search
            case .actions(let command):
                ActionsPane(model: model, command: command)
            case .args(let command):
                ArgsForm(model: model, command: command)
            case .output(let title, let text):
                OutputPane(title: title, text: text, markdown: model.outputMarkdown)
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
                    .font(.system(size: metaSize + 1, weight: .medium))
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
        .onAppear {
            syncTypeScale()
            selectField()
            // ready may already be true before first appear (see AppShell); still fill the list.
            if model.ready { model.search() }
        }
        .onChange(of: bodySize) { _, _ in syncTypeScale() }
        .onChange(of: model.focusTick) { _, _ in selectField() }
        .onChange(of: model.ready) { _, ready in
            if ready { model.search() }
        }
        // Query → search is handled in Model.query.didSet (Tahoe-safe).
        .onReceive(NotificationCenter.default.publisher(for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
            reduceTransparency = Theme.reduceTransparency
        }
    }

    private var panelShape: AnyShape {
        if isSpotlight {
            return AnyShape(RoundedRectangle(cornerRadius: Theme.file.radius.map { CGFloat($0) } ?? Theme.spotlightRadius, style: .continuous))
        }
        let r = Theme.file.radius.map { CGFloat($0) } ?? Theme.islandBottomRadius
        return AnyShape(UnevenRoundedRectangle(bottomLeadingRadius: r, bottomTrailingRadius: r))
    }

    @ViewBuilder private var panelFill: some View {
        if reduceTransparency || Theme.file.glass == false {
            Theme.island(scheme)
        } else {
            // ponytail: thinMaterial reads clearer than ultraThin; opaque island when Reduce transparency.
            Rectangle().fill(.thinMaterial)
        }
    }

    private var search: some View {
        VStack(spacing: 0) {
            field
            if !model.clicking {
                HStack(spacing: 0) {
                    if model.isListMode, model.listOptions.isGrid {
                        grid
                    } else {
                        list
                    }
                    if let preview = model.preview {
                        Rectangle().fill(Theme.stroke(scheme).opacity(0.45)).frame(width: 0.5)
                        PreviewPane(data: preview, scheme: scheme)
                    }
                }
                .frame(height: model.listHeight)
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
                prompt: Text(fieldPrompt)
                    .foregroundStyle(Theme.secondary(scheme))
            )
            .textFieldStyle(.plain)
            .font(Theme.font(size: Theme.file.titleFontSize.map { $0 * Double(typeScale) } ?? Double(fieldSize)))
            .foregroundStyle(Theme.primary(scheme))
            .tint(Theme.accent)
            .focusEffectDisabled()
            .focused($focused)
            .autocorrectionDisabled()
            if model.dictating {
                Image(systemName: "mic.fill")
                    .foregroundStyle(Theme.danger)
                    .symbolEffect(.pulse, isActive: !Theme.reduceMotion)
                    .help("Listening — Esc to stop")
            }
            if model.busy {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 16, height: 16)
            }
        }
        .padding(.horizontal, 18)
        .frame(height: model.fieldChrome)
    }

    private var fieldPrompt: String {
        if model.clicking { return "Type what you see…" }
        if case .list(let command, _, _) = model.mode { return "\(command.title) — filter…" }
        return "Search apps and commands, or ask AI…"
    }

    /// Matches footer `verb` when no providers are configured (N-8).
    private var askRowTitle: String {
        if model.providersReady, model.providers.isEmpty {
            return "Set up AI…"
        }
        return "Ask AI “\(model.trimmedQuery)”"
    }

    private var list: some View {
        Group {
            if model.rows.isEmpty {
                let q = model.query.trimmingCharacters(in: .whitespacesAndNewlines)
                Text(
                    q.isEmpty
                        ? "No results"
                        : model.isListMode
                            ? "No matches — Esc searches all commands"
                            : "No results for “\(q)” · Tab asks AI · / browses files"
                )
                    .foregroundStyle(Theme.secondary(scheme))
                    .frame(maxWidth: .infinity, minHeight: model.rowHeight)
                    .padding(.vertical, 6)
            } else {
                let start = model.listStart
                let visible = Array(model.rows.enumerated()).dropFirst(start).prefix(8)
                VStack(spacing: 0) {
                    ForEach(Array(visible), id: \.element.id) { index, row in
                        rowView(row, index: index, hideKind: hideKindLabels(in: visible))
                    }
                }
                .accessibilityElement(children: .contain)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
            }
        }
    }

    /// Script Filter `"view": "grid"`: big icons (emoji, images, colors) in columns.
    private var grid: some View {
        let columns = model.listOptions.columns
        let rows = model.rows
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: columns), spacing: 8) {
                    ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                        gridCell(row, index: index)
                            .id(index)
                    }
                }
                .padding(10)
            }
            .onChange(of: model.selected) { _, i in
                withAnimation(.easeOut(duration: 0.12)) { proxy.scrollTo(i) }
            }
        }
    }

    @ViewBuilder private func gridCell(_ row: Model.Row, index: Int) -> some View {
        if case .command(let command) = row {
            let on = index == model.selected
            VStack(spacing: 6) {
                if let key = command.icon, Model.isImageIcon(key) {
                    icon(command, size: 64)
                } else if let glyph = command.icon, !glyph.isEmpty {
                    Text(glyph).font(.system(size: captionSize * 3.5))
                        .frame(height: 64)
                } else {
                    icon(command, size: 40)
                        .frame(height: 64)
                }
                Text(command.title)
                    .font(.system(size: captionSize))
                    .lineLimit(1)
                    .foregroundStyle(on ? Theme.primary(scheme) : Theme.secondary(scheme))
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .background {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(on ? Theme.accent.opacity(scheme == .light ? 0.14 : 0.22) : Theme.kbd(scheme).opacity(0.35))
            }
            .overlay {
                if on {
                    RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Theme.accent.opacity(0.4), lineWidth: 1)
                }
            }
            .contentShape(Rectangle())
            .onTapGesture {
                model.selected = index
                model.activate(index)
            }
            .help(command.subtitle ?? command.title)
        }
    }

    private func hideKindLabels(in visible: ArraySlice<(offset: Int, element: Model.Row)>) -> Bool {
        let kinds = visible.compactMap { pair -> Command.Kind? in
            guard case .command(let c) = pair.element else { return nil }
            return c.kind
        }
        guard !kinds.isEmpty else { return false }
        if Set(kinds).count == 1 { return true }
        return kinds.filter { $0 == .app }.count * 2 > kinds.count
    }

    private func rowView(_ row: Model.Row, index: Int, hideKind: Bool = false) -> some View {
        let on = index == model.selected
        let axLabel: String
        let axValue: String
        switch row {
        case .ask:
            axLabel = askRowTitle
            axValue = "AI"
        case .command(let command):
            axLabel = [command.title, command.subtitle].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: ", ")
            axValue = command.kind.label
        }
        return HStack(spacing: 12) {
            switch row {
            case .ask:
                Image(systemName: "sparkles")
                    .font(.system(size: iconSize - 1, weight: .semibold))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(Theme.accent)
                    .frame(width: 28, height: 28)
                Text(askRowTitle)
                    .lineLimit(1)
                Spacer(minLength: 8)
                Text("Tab")
                    .font(.system(size: metaSize))
                    .foregroundStyle(Theme.tertiary(scheme))
            case .command(let command):
                icon(command)
                Text(command.title)
                    .font(Theme.font(
                        size: Theme.file.fontSize.map { $0 * Double(typeScale) } ?? Double(titleSize),
                        weight: .medium
                    ))
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
                        .font(.system(size: metaSize, design: .monospaced))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                        .foregroundStyle(Theme.secondary(scheme))
                }
                if !hideKind {
                    Text(command.kind.label)
                        .font(.system(size: metaSize))
                        .foregroundStyle(Theme.tertiary(scheme))
                }
            }
        }
        .padding(.horizontal, 10)
        .frame(height: model.rowHeight)
        .background {
            if on {
                let fill = Theme.increaseContrast
                    ? Theme.accent.opacity(scheme == .light ? 0.28 : 0.40)
                    : Theme.accent.opacity(scheme == .light ? 0.14 : 0.22)
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(fill)
                    .overlay(alignment: .leading) {
                        // 2pt leading accent; inset so it clears the continuous corner.
                        Rectangle()
                            .fill(Theme.accent)
                            .frame(width: 2)
                            .padding(.vertical, 6)
                    }
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .stroke(Theme.accent.opacity(0.35), lineWidth: 1)
                    )
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .modifier(LauncherRowA11y(label: axLabel, value: axValue, selected: on))
        .onHover { inside in
            guard inside else { return }
            let loc = NSEvent.mouseLocation
            let dx = loc.x - pointer.x
            let dy = loc.y - pointer.y
            // A stationary cursor must not grab the highlight when the window slides.
            guard dx * dx + dy * dy > 1 else { return }
            pointer = loc
            model.selected = index
            model.updatePreview()
        }
        .onTapGesture { model.activate(index) }
    }

    @ViewBuilder private func icon(_ command: Command, size: CGFloat = 28) -> some View {
        let _ = model.thumbTick
        if let swatch = command.icon, swatch.hasPrefix("color:"), let c = Theme.color(hex: String(swatch.dropFirst(6))) {
            Circle()
                .fill(c)
                .overlay(Circle().stroke(Theme.stroke(scheme), lineWidth: 0.5))
                .frame(width: size * 0.75, height: size * 0.75)
                .frame(width: size, height: size)
        } else if let key = command.icon, Model.isImageIcon(key) {
            if let image = model.imageIcons[key] {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: size, height: size)
            } else {
                Color.clear.frame(width: size, height: size)
            }
        } else if command.kind == .window, let region = WindowGlyphs.regions[command.id] {
            WindowGlyph(region: region, size: 24, tint: Theme.primary(scheme).opacity(0.8), portrait: WindowGlyphs.portrait)
                .frame(width: 28, height: 28)
        } else if command.kind == .clipboard,
           let image = ClipboardHistory.shared.image(for: command.id)
        {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 28, height: 28)
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        } else if let path = fileIconPath(command), let image = model.appIcons[path] {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 28, height: 28)
        } else if let name = Theme.resolvedSymbol(for: command) {
            Image(systemName: name)
                .font(.system(size: iconSize, weight: .medium))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(Theme.primary(scheme))
                .frame(width: 28, height: 28)
        } else {
            Text(command.id == "window:center-third" ? "\u{2153}" : (command.icon ?? ""))
                .font(.system(size: iconSize))
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
        HintBar(left: { footerLeft }, hints: footerHints)
            .frame(height: model.footerChrome)
    }

    @ViewBuilder private var footerLeft: some View {
        if model.searchingFiles {
            HStack(spacing: 6) {
                ProgressView()
                    .controlSize(.mini)
                Text("Searching files…")
            }
        } else if !model.fileBuffer.isEmpty {
            Label(
                "File Buffer · \(model.fileBuffer.count) · ⌘K acts on all · ⌥← clears",
                systemImage: "paperclip"
            )
            .labelStyle(.titleAndIcon)
            .lineLimit(1)
        } else if FocusController.shared.isActive {
            let f = FocusController.shared
            Label(
                "\(f.isPaused ? "Focus paused" : "Focus") · \(f.minutesLeft) min left\(f.goal.isEmpty ? "" : " · " + f.goal)",
                systemImage: f.isPaused ? "pause.fill" : "target"
            )
            .lineLimit(1)
            .labelStyle(.titleAndIcon)
        } else if model.clicking {
            if !model.seekCount.isEmpty {
                Text(model.seekCount)
            }
        } else {
            Text(footerLeftLabel)
        }
    }

    private var footerLeftLabel: String {
        let q = model.query.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty, model.warningCount > 0 {
            return model.warningCount == 1 ? "1 warning" : "\(model.warningCount) warnings"
        }
        guard !q.isEmpty else { return "OpenCast" }
        let total = model.rows.count
        if total == 0 { return "0 results" }
        let shown = min(8, max(0, total - model.listStart))
        return "\(shown) of \(total)"
    }

    private var footerHints: [(String, String)] {
        if model.clicking {
            return [("Back", "Esc"), ("Click", "Return")]
        }
        var h: [(String, String)] = []
        if let verb = model.verb {
            h.append((verb, "Return"))
            if model.isListMode {
                h.append(("Back", "Esc"))
            } else if !model.askSelected {
                h.append(("Ask AI", Theme.keyLabel(model.keymap["ask_ai"], "Tab")))
            }
        }
        h.append(("Actions", Theme.keyLabel(model.keymap["actions"], "⌘K")))
        if !model.isListMode {
            h.append(("Click Text", Theme.keyLabel(model.keymap["click_text"], "`")))
        }
        return h
    }

    private func selectField() {
        switch model.mode {
        case .search, .list: break
        default: return
        }
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
    @ScaledMetric(relativeTo: .headline) private var titleSize: CGFloat = 15
    @ScaledMetric(relativeTo: .body) private var bodySize: CGFloat = 14
    @ScaledMetric(relativeTo: .caption) private var metaSize: CGFloat = 12

    @Bindable var model: Model
    var command: Command
    @Environment(\.colorScheme) private var systemScheme
    private var scheme: ColorScheme {
        let _ = model.appearanceEpoch
        return Theme.scheme(preference: model.themePreference, system: systemScheme)
    }

    private var actions: [Model.ActionRow] { model.filteredActionRows(for: command) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                BackButton { model.backToSearch(clearQuery: false) }
                Text(command.title)
                    .font(.system(size: titleSize, weight: .semibold))
                    .lineLimit(1)
                Spacer()
                if !model.trimmedQuery.isEmpty {
                    Text("Filter “\(model.trimmedQuery)”")
                        .font(.system(size: metaSize))
                        .foregroundStyle(Theme.secondary(scheme))
                        .lineLimit(1)
                }
                Text("Actions")
                    .font(.system(size: metaSize))
                    .foregroundStyle(Theme.tertiary(scheme))
            }
            .padding(.horizontal, 14)
            .frame(height: 52 * (bodySize / 14))
            ScrollViewReader { proxy in
            ScrollView {
            VStack(spacing: 0) {
            ForEach(Array(actions.enumerated()), id: \.element.id) { index, action in
                let on = index == model.selected
                let ax = [action.title, action.keyLabel].compactMap { $0 }.joined(separator: ", ")
                HStack {
                    Text(action.title)
                        .font(.system(size: bodySize))
                    Spacer()
                    if let key = action.keyLabel {
                        Text(key)
                            .font(.system(size: metaSize))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                            .foregroundStyle(Theme.secondary(scheme))
                    }
                }
                .padding(.horizontal, 14)
                .frame(height: model.actionRowH)
                .background(on ? Theme.accent.opacity(0.18) : Color.clear)
                .contentShape(Rectangle())
                .modifier(LauncherRowA11y(label: ax, value: "Action", selected: on))
                .onTapGesture { model.runAction(action.id, for: command) }
                .id(index)
            }
            }
            }
            .onChange(of: model.selected) { _, i in proxy.scrollTo(i) }
            }
            Spacer(minLength: 0)
            HintBar(left: "OpenCast", hints: [("Back", "Esc"), ("Run", "Return")])
        }
        .frame(height: 52 * (bodySize / 14) + CGFloat(min(actions.count, 10)) * model.actionRowH + model.footerChrome)
    }
}

struct BackButton: View {
    @ScaledMetric(relativeTo: .caption) private var metaSize: CGFloat = 12

    var action: () -> Void
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        Button(action: action) {
            Image(systemName: "chevron.left")
                .font(.system(size: metaSize, weight: .semibold))
                .foregroundStyle(Theme.secondary(scheme))
                .frame(width: 26, height: 26)
                .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Back")
    }
}

struct HintBar<Left: View>: View {
    @ScaledMetric(relativeTo: .caption) private var metaSize: CGFloat = 12

    var left: Left
    var hints: [(String, String)]
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    init(left: String, hints: [(String, String)]) where Left == Text {
        self.left = Text(left)
        self.hints = hints
    }

    init(@ViewBuilder left: () -> Left, hints: [(String, String)]) {
        self.left = left()
        self.hints = hints
    }

    var body: some View {
        HStack {
            left
                .font(.system(size: metaSize))
                .foregroundStyle(Theme.secondary(scheme))
            Spacer()
            HStack(spacing: 14) {
                ForEach(Array(hints.enumerated()), id: \.offset) { _, hint in
                    HStack(spacing: 4) {
                        Text(hint.0)
                        Text(hint.1)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                    }
                    .font(.system(size: metaSize))
                    .foregroundStyle(Theme.secondary(scheme))
                }
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38 * (metaSize / 12))
    }
}

private struct ArgsForm: View {
    @ScaledMetric(relativeTo: .headline) private var titleSize: CGFloat = 15
    @ScaledMetric(relativeTo: .body) private var bodySize: CGFloat = 14
    @ScaledMetric(relativeTo: .body) private var iconSize: CGFloat = 17

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
                    .font(.system(size: titleSize, weight: .semibold))
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
            .frame(minHeight: 58 * (bodySize / 14))
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
            .font(.system(size: bodySize))
            .foregroundStyle(Theme.secondary(scheme))
            .multilineTextAlignment(.center)
            .padding(24)
    }

    @ViewBuilder private var argIcon: some View {
        if let name = Theme.resolvedSymbol(for: command) {
            Image(systemName: name)
                .font(.system(size: iconSize, weight: .medium))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(Theme.primary(scheme))
                .frame(width: 22, height: 22)
        } else if let icon = command.icon, !icon.isEmpty {
            Text(icon)
                .font(.system(size: iconSize))
        }
    }

    @ViewBuilder private func field(_ index: Int) -> some View {
        let spec = command.arguments[index]
        let placeholder = spec.placeholder + (spec.optional ? " (optional)" : "")
        let prompt = Text(placeholder).foregroundStyle(Theme.secondary(scheme))
        Group {
            if spec.secure {
                SecureField("", text: $model.argDrafts[index], prompt: prompt)
            } else {
                TextField("", text: $model.argDrafts[index], prompt: prompt)
            }
        }
        .textFieldStyle(.plain)
        .font(.system(size: bodySize))
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
    @ScaledMetric(relativeTo: .headline) private var titleSize: CGFloat = 15
    @ScaledMetric(relativeTo: .body) private var bodySize: CGFloat = 14
    @ScaledMetric(relativeTo: .caption) private var metaSize: CGFloat = 12

    var title: String
    var text: String
    var markdown = false
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.themePreference) private var themePreference
    private var scheme: ColorScheme { Theme.scheme(preference: themePreference, system: systemScheme) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                BackButton { Model.shared.backToSearch(clearQuery: false) }
                Text(title)
                    .font(.system(size: titleSize, weight: .semibold))
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 18)
            .frame(height: 58 * (bodySize / 14))
            .overlay(alignment: .bottom) {
                Rectangle().fill(Theme.stroke(scheme).opacity(0.45)).frame(height: 0.5)
            }
            ScrollView {
                if markdown, !text.isEmpty {
                    MarkdownText(text: text, scheme: scheme)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 16)
                } else {
                    Text(text.isEmpty ? "(no output)" : text)
                        .font(.system(size: metaSize + 1, design: .monospaced))
                        .foregroundStyle(text.isEmpty ? Theme.tertiary(scheme) : Theme.primary(scheme))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 16)
                }
            }
            .frame(maxHeight: .infinity)
            HintBar(left: "OpenCast", hints: [("Copy", "↵"), ("Back", "esc")])
        }
    }
}

/// Shared VoiceOver traits for search rows and ActionsPane rows (N-10).
private struct LauncherRowA11y: ViewModifier {
    let label: String
    let value: String
    let selected: Bool

    func body(content: Content) -> some View {
        content
            .accessibilityElement(children: .combine)
            .accessibilityLabel(label)
            .accessibilityValue(value)
            .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }
}
