import AppKit
import SwiftUI

struct LauncherView: View {
    @Bindable var model: Model
    @FocusState private var focused: Bool
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        Group {
            if case .search = model.mode {
                search
            } else {
                Text(model.stubTitle)
                    .font(.system(size: 15, weight: .semibold))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .foregroundStyle(Theme.primary(scheme))
        .background(Theme.island(scheme))
        .overlay(alignment: .bottom) {
            if let text = model.toastText {
                Text(text)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(model.toastIsError ? Color.white : Theme.island(scheme))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(model.toastIsError ? Theme.danger : Theme.primary(scheme), in: RoundedRectangle(cornerRadius: 10))
                    .padding(.bottom, 50)
            }
        }
        .clipShape(islandShape)
        .ignoresSafeArea()
        .overlay { islandShape.stroke(Theme.stroke(scheme), lineWidth: 0.5) }
        .compositingGroup()
        .shadow(color: Theme.shadow(scheme), radius: Theme.shadowRoom, y: 22)
        .padding(.horizontal, Theme.shadowRoom)
        .padding(.bottom, Theme.shadowRoom)
        .onAppear { selectField() }
        .onChange(of: model.focusTick) { _, _ in selectField() }
        .onChange(of: model.ready) { _, ready in
            if ready { model.search() }
        }
        .onChange(of: model.query) { _, _ in
            model.selected = 0
            model.noteContentChanged()
            if model.ready { model.search() }
        }
    }

    private var islandShape: UnevenRoundedRectangle {
        UnevenRoundedRectangle(bottomLeadingRadius: 18, bottomTrailingRadius: 18)
    }

    private var search: some View {
        VStack(spacing: 0) {
            field
            list.frame(height: model.listHeight)
            footer
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            TextField(
                "",
                text: $model.query,
                prompt: Text("Search apps and commands, or ask AI…").foregroundStyle(Theme.tertiary(scheme))
            )
            .textFieldStyle(.plain)
            .font(.system(size: 19))
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
        ScrollView {
            if model.rows.isEmpty {
                Text("No results")
                    .foregroundStyle(Theme.secondary(scheme))
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .padding(.vertical, 6)
            } else {
                ScrollViewReader { proxy in
                    LazyVStack(spacing: 0) {
                        ForEach(Array(model.rows.enumerated()), id: \.element.id) { index, row in
                            rowView(row, index: index)
                                .id(index)
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .onChange(of: model.selected) { _, index in
                        proxy.scrollTo(index, anchor: .center)
                    }
                }
            }
        }
    }

    private func rowView(_ row: Model.Row, index: Int) -> some View {
        HStack(spacing: 12) {
            switch row {
            case .ask:
                Image(systemName: "sparkles")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Theme.accent)
                    .frame(width: 24, height: 24)
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
                Text(command.kind.label)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.tertiary(scheme))
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 44)
        .background {
            if index == model.selected {
                RoundedRectangle(cornerRadius: 9)
                    .fill(Theme.accent.opacity(scheme == .light ? 0.12 : 0.18))
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 9))
        .onHover { inside in
            if inside { model.selected = index }
        }
        .onTapGesture { model.activate(index) }
    }

    @ViewBuilder private func icon(_ command: Command) -> some View {
        if command.kind == .app, let image = model.appIcons[String(command.id.dropFirst(4))] {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 24, height: 24)
        } else if let name = Theme.symbol(for: command),
                  NSImage(systemSymbolName: name, accessibilityDescription: nil) != nil {
            Image(systemName: name)
                .font(.system(size: 16))
                .frame(width: 24, height: 24)
        } else {
            Text(command.id == "window:center-third" ? "\u{2153}" : (command.icon ?? ""))
                .font(.system(size: 17))
                .frame(width: 24, height: 24)
        }
    }

    private var footer: some View {
        HStack {
            Text("OpenCast")
                .font(.system(size: 12))
                .foregroundStyle(Theme.tertiary(scheme))
            Spacer()
            HStack(spacing: 14) {
                if let verb = model.verb {
                    hint(verb, "Return")
                    if !model.askSelected { hint("Ask AI", "Tab") }
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
                .background(Theme.kbd(scheme), in: RoundedRectangle(cornerRadius: 5))
        }
        .font(.system(size: 12))
        .foregroundStyle(Theme.secondary(scheme))
    }

    private func selectField() {
        guard case .search = model.mode else { return }
        focused = true
        DispatchQueue.main.async {
            NSApp.keyWindow?.fieldEditor(false, for: nil)?.selectAll(nil)
        }
    }
}
