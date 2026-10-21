import Foundation

struct Argument: Equatable, Sendable {
    var placeholder: String
    var optional: Bool
    var secure: Bool

    init?(_ raw: [String: Any]) {
        guard let placeholder = raw["placeholder"] as? String else { return nil }
        self.placeholder = placeholder
        optional = raw["optional"] as? Bool ?? false
        secure = raw["secure"] as? Bool ?? false
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

    enum Kind: String, Equatable, Sendable {
        case builtin, ai, script, quicklink, app, calculator, window, layout, file, snippet, clipboard

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
    }

    init(id: String, title: String, subtitle: String?, icon: String?, kind: Kind, arguments: [Argument], hotkey: String?) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.icon = icon
        self.kind = kind
        self.arguments = arguments
        self.hotkey = hotkey
    }
}

struct Provider: Identifiable, Equatable, Sendable {
    var id: String
    var models: [String]
    var defaultModel: String
    var isDefault: Bool

    init?(_ raw: [String: Any]) {
        guard let id = raw["id"] as? String else { return nil }
        self.id = id
        models = raw["models"] as? [String] ?? []
        defaultModel = raw["defaultModel"] as? String ?? ""
        isDefault = raw["isDefault"] as? Bool ?? false
    }
}

struct ChatTurn: Equatable, Sendable {
    var role: String
    var content: String
    var error: String?
}

enum ExecOutcome: Sendable {
    case hide
    case toast(String)
    case error(String)
    case output(title: String, text: String)
    case chat(prompt: String?, system: String?, provider: String?, model: String?, autoSend: Bool)
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
        case "chat":
            self = .chat(
                prompt: raw["prompt"] as? String,
                system: raw["system"] as? String,
                provider: raw["provider"] as? String,
                model: raw["model"] as? String,
                autoSend: raw["autoSend"] as? Bool ?? false
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
