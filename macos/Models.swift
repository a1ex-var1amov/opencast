import Foundation

struct Argument: Equatable, Sendable {
    var placeholder: String
    var optional: Bool
    var secure: Bool

    init(placeholder: String, optional: Bool, secure: Bool) {
        self.placeholder = placeholder
        self.optional = optional
        self.secure = secure
    }

    init?(_ raw: [String: Any]) {
        guard let placeholder = raw["placeholder"] as? String else { return nil }
        self.placeholder = placeholder
        optional = raw["optional"] as? Bool ?? false
        secure = raw["secure"] as? Bool ?? false
    }
}

/// One window as a saved layout remembers it (AX coordinates, top-left origin).
struct SavedWindow: Codable {
    var title: String
    var x: Double
    var y: Double
    var w: Double
    var h: Double
}

/// One app's windows in a saved layout. Lives here, next to the other wire
/// types, so `Mode` can decode without pulling in the window manager.
struct AppWindows: Codable {
    var app: String
    var bundleId: String?
    var windows: [SavedWindow]

    enum CodingKeys: String, CodingKey {
        case app, windows
        case bundleId = "bundle_id"
    }
}

struct CommandAction: Equatable, Sendable {
    var id: String
    var title: String
    var key: String?

    init(id: String, title: String, key: String?) {
        self.id = id
        self.title = title
        self.key = key
    }

    init?(_ raw: [String: Any]) {
        guard let id = raw["id"] as? String, let title = raw["title"] as? String else { return nil }
        self.id = id
        self.title = title
        key = raw["key"] as? String
    }
}

struct Command: Identifiable, Equatable, Sendable {
    var id: String
    var title: String
    var subtitle: String?
    var icon: String?
    var kind: Kind
    var arguments: [Argument]
    var hotkey: String?
    /// ⌘-menu actions an integration adds to this row (the app row of a running
    /// integration). The engine owns the list, so config switches apply.
    var actions: [CommandAction]

    enum Kind: String, Equatable, Sendable {
        case builtin, ai, script, quicklink, app, calculator, window, layout, file, snippet, clipboard, shortcut, mcp
        // Core serializes these lowercase; without them the rows fail to decode
        // and every MCP resource and prompt silently disappears from search.
        case mcpResource = "mcpresource"
        case mcpPrompt = "mcpprompt"

        var label: String {
            switch self {
            case .app: "Application"
            case .ai: "AI Command"
            case .script: "Script"
            case .quicklink: "Quicklink"
            case .builtin: "Command"
            case .calculator: "Calculator"
            case .window: "Window"
            case .layout: "Layout"
            case .file: "File"
            case .snippet: "Snippet"
            case .clipboard: "Clipboard"
            case .shortcut: "Shortcut"
            case .mcp: "MCP Tool"
            case .mcpResource: "MCP Resource"
            case .mcpPrompt: "MCP Prompt"
            }
        }
    }

    init?(_ raw: [String: Any]) {
        guard let id = raw["id"] as? String,
              let title = raw["title"] as? String,
              let kind = (raw["kind"] as? String).flatMap(Kind.init(rawValue:))
        else { return nil }
        self.id = id
        self.title = title
        self.kind = kind
        subtitle = raw["subtitle"] as? String
        icon = raw["icon"] as? String
        arguments = (raw["arguments"] as? [[String: Any]])?.compactMap(Argument.init) ?? []
        hotkey = raw["hotkey"] as? String
        actions = (raw["actions"] as? [[String: Any]])?.compactMap(CommandAction.init) ?? []
    }

    init(id: String, title: String, subtitle: String?, icon: String?, kind: Kind, arguments: [Argument], hotkey: String?, actions: [CommandAction] = []) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.icon = icon
        self.kind = kind
        self.arguments = arguments
        self.hotkey = hotkey
        self.actions = actions
    }
}

struct Provider: Identifiable, Equatable, Sendable {
    var id: String
    var models: [String]
    var defaultModel: String
    var isDefault: Bool
    /// The engine will take image attachments for this provider's model.
    var vision: Bool

    init?(_ raw: [String: Any]) {
        guard let id = raw["id"] as? String else { return nil }
        self.id = id
        models = raw["models"] as? [String] ?? []
        defaultModel = raw["defaultModel"] as? String ?? ""
        isDefault = raw["isDefault"] as? Bool ?? false
        vision = raw["vision"] as? Bool ?? false
    }
}

struct ChatTurn: Equatable, Sendable {
    var role: String
    /// What the user typed / what the model answered (shown).
    var content: String
    var error: String?
    /// Tools the model ran during this answer (`github · create_issue`).
    var tools: [String] = []
    /// Attachment text sent with a user turn (not shown inline).
    var context: String = ""
    var attachmentNames: [String] = []
    /// Image attachments as `{"mime":…, "b64":…}` for a vision model.
    /// Not saved with the chat: base64 would bloat the chats file.
    var images: [[String: String]] = []

    /// What the model sees for this turn.
    var wireContent: String { context.isEmpty ? content : content + "\n\n" + context }
}

/// A file, clipboard or screen attachment for the next chat message: either
/// extracted text (any model) or image pixels (a vision model — see
/// `Attachments.imageData`).
struct ChatAttachment: Equatable, Sendable, Identifiable {
    var id = UUID()
    var name: String
    var text: String
    var imageB64: String? = nil
    var imageMime: String? = nil

    /// What the attachment chip shows.
    var isImage: Bool { imageB64 != nil }
}

/// The model wants to call a tool that is not auto-approved.
struct ToolApproval: Equatable, Sendable {
    var requestId: String
    var callId: String
    var title: String
    var arguments: String
}

/// One row from a Script Filter (`@opencast.mode list`).
/// Extra action on a list row (⌘K or its own shortcut): runs the `run` step with `arg`.
struct ItemAction: Equatable, Sendable {
    var title: String
    var arg: String
    var key: String?
}

struct ListItem: Equatable, Sendable {
    var title: String
    var subtitle: String?
    var icon: String?
    var arg: String
    var autocomplete: String?
    /// Detail pane (markdown).
    var markdown: String?
    var actions: [ItemAction] = []
    /// File or URL for Quick Look.
    var quicklook: String?
    /// Text for Copy; defaults to `arg`.
    var copy: String?
    /// `false`: Return autocompletes instead of running (Alfred).
    var valid = true
    /// Passed to the `run` step as environment variables.
    var variables: [String: String] = [:]
    /// Key equivalent shown in the hotkey column (menu shortcuts list rows).
    var hotkey: String?

    init(title: String, subtitle: String?, icon: String?, arg: String, autocomplete: String? = nil,
         hotkey: String? = nil) {
        self.title = title
        self.subtitle = subtitle
        self.icon = icon
        self.arg = arg
        self.autocomplete = autocomplete
        self.hotkey = hotkey
    }

    init?(_ raw: [String: Any]) {
        guard let title = raw["title"] as? String, !title.isEmpty else { return nil }
        self.title = title
        subtitle = raw["subtitle"] as? String
        icon = raw["icon"] as? String
        arg = raw["arg"] as? String ?? title
        autocomplete = raw["autocomplete"] as? String
        markdown = raw["markdown"] as? String
        quicklook = raw["quicklook"] as? String
        copy = raw["copy"] as? String
        valid = raw["valid"] as? Bool ?? true
        variables = raw["variables"] as? [String: String] ?? [:]
        actions = (raw["actions"] as? [[String: Any]] ?? []).compactMap { a in
            guard let t = a["title"] as? String, let arg = a["arg"] as? String else { return nil }
            return ItemAction(title: t, arg: arg, key: a["key"] as? String)
        }
    }
}

/// How a Script Filter wants its rows shown.
struct ListOptions: Equatable, Sendable {
    /// `list` or `grid`.
    var view = "list"
    var columns = 5
    /// Re-run the list step every N seconds while it is open.
    var rerun: Double?

    init() {}

    init(_ raw: [String: Any]) {
        view = raw["view"] as? String ?? "list"
        columns = min(max(raw["columns"] as? Int ?? 5, 2), 10)
        rerun = raw["rerun"] as? Double
    }

    var isGrid: Bool { view == "grid" }
}

/// `[keys]` binding, pre-parsed by core: lowercase mods and key name.
struct KeyBinding: Equatable, Sendable {
    var name: String
    var mods: Set<String>
    var key: String

    init?(_ raw: [String: Any]) {
        guard let name = raw["name"] as? String, let key = raw["key"] as? String else { return nil }
        self.name = name
        self.key = key
        mods = Set(raw["mods"] as? [String] ?? [])
    }
}

/// Where an AI command's answer goes (`output` in config).
enum AiOutput: String, Sendable {
    case chat, replace, paste, copy
}

/// Optional `theme_file` values. Nil = keep the built-in default.
struct ThemeFile: Equatable, Sendable {
    var accent: String?
    var panelLight: String?
    var panelDark: String?
    var textLight: String?
    var textDark: String?
    var radius: Double?
    var rowHeight: Double?
    var width: Double?
    var fontSize: Double?
    var titleFontSize: Double?
    var glass: Bool?
    var font: String?

    init() {}

    init(_ raw: [String: Any]) {
        accent = raw["accent"] as? String
        panelLight = raw["panel_light"] as? String
        panelDark = raw["panel_dark"] as? String
        textLight = raw["text_light"] as? String
        textDark = raw["text_dark"] as? String
        radius = raw["radius"] as? Double
        rowHeight = raw["row_height"] as? Double
        width = raw["width"] as? Double
        fontSize = raw["font_size"] as? Double
        titleFontSize = raw["title_font_size"] as? Double
        glass = raw["glass"] as? Bool
        font = raw["font"] as? String
    }
}

enum ExecOutcome: Sendable {
    case hide
    case toast(String)
    case error(String)
    case output(title: String, text: String)
    case markdown(title: String, text: String)
    case chat(prompt: String?, system: String?, provider: String?, model: String?, autoSend: Bool,
              temperature: Double?, maxTokens: Int?, output: AiOutput, preset: String?)
    /// Snap the front window; `frac` = (x, y, w, h) of the work area or nil for special actions.
    case window(action: String, frac: [Double]?)
    /// `capture` → record windows and call layoutSave; `apply` → place `apps`.
    case layout(op: String, name: String, apps: [AppWindows])
    case list(command: Command, items: [ListItem], filter: Bool, options: ListOptions)
    case quit
    /// Panel-side feature: `settings`, `menu-items`, `dictate`, `schedule`.
    case ui(String, [String])
    case resumeChat(id: String, provider: String?, model: String?, system: String?, turns: [ChatTurn])
    case args(Command)
    case failure(String)
    case query(String)
    case theme(String)
    case presentation(String)
    case stayAwake(String)
    case runSelected
    case doEscape
    case hint(String)
    case askAi
    case clickMode

    init(_ raw: [String: Any]) {
        switch raw["type"] as? String {
        case "hide":
            self = .hide
        case "toast":
            self = .toast(raw["text"] as? String ?? "")
        case "error":
            self = .error(raw["message"] as? String ?? "Error")
        case "output":
            self = .output(title: raw["title"] as? String ?? "", text: raw["text"] as? String ?? "")
        case "markdown":
            self = .markdown(title: raw["title"] as? String ?? "", text: raw["text"] as? String ?? "")
        case "chat":
            self = .chat(
                prompt: raw["prompt"] as? String,
                system: raw["system"] as? String,
                provider: raw["provider"] as? String,
                model: raw["model"] as? String,
                autoSend: raw["autoSend"] as? Bool ?? false,
                temperature: raw["temperature"] as? Double,
                maxTokens: raw["maxTokens"] as? Int,
                output: AiOutput(rawValue: raw["output"] as? String ?? "chat") ?? .chat,
                preset: raw["preset"] as? String
            )
        case "window":
            self = .window(action: raw["action"] as? String ?? "", frac: raw["frac"] as? [Double])
        case "layout":
            let apps: [AppWindows] = (raw["apps"] as? [[String: Any]])
                .flatMap { try? JSONSerialization.data(withJSONObject: $0) }
                .flatMap { try? JSONDecoder().decode([AppWindows].self, from: $0) } ?? []
            self = .layout(op: raw["op"] as? String ?? "", name: raw["name"] as? String ?? "", apps: apps)
        case "list":
            if let command = (raw["command"] as? [String: Any]).flatMap(Command.init) {
                let items = (raw["items"] as? [[String: Any]])?.compactMap(ListItem.init) ?? []
                self = .list(command: command, items: items, filter: raw["filter"] as? Bool ?? true, options: ListOptions(raw))
            } else {
                self = .failure("Bad list payload")
            }
        case "quit":
            self = .quit
        case "ui":
            self = .ui(raw["action"] as? String ?? "", raw["args"] as? [String] ?? [])
        case "resumeChat":
            let chat = raw["chat"] as? [String: Any] ?? [:]
            let turns: [ChatTurn] = (chat["messages"] as? [[String: Any]] ?? []).compactMap { m in
                guard let role = m["role"] as? String, role == "user" || role == "assistant" else { return nil }
                let content = m["content"] as? String ?? ""
                if role == "assistant", content.isEmpty { return nil }
                return ChatTurn(role: role, content: content)
            }
            self = .resumeChat(
                id: chat["id"] as? String ?? "chat:\(UUID().uuidString)",
                provider: chat["provider"] as? String,
                model: chat["model"] as? String,
                system: chat["system"] as? String,
                turns: turns
            )
        case "args":
            if let command = (raw["command"] as? [String: Any]).flatMap(Command.init) {
                self = .args(command)
            } else {
                self = .failure("Bad arguments payload")
            }
        case "query":
            self = .query(raw["query"] as? String ?? "")
        case "theme":
            self = .theme(raw["theme"] as? String ?? "system")
        case "presentation":
            self = .presentation(raw["presentation"] as? String ?? "island")
        case "stayAwake":
            self = .stayAwake(raw["mode"] as? String ?? "off")
        case "runSelected":
            self = .runSelected
        case "doEscape":
            self = .doEscape
        case "hint":
            self = .hint(raw["text"] as? String ?? "")
        case "askAi":
            self = .askAi
        case "clickMode":
            self = .clickMode
        default:
            self = .failure(raw["error"] as? String ?? "Unknown result")
        }
    }
}
