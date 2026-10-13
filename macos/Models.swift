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

    enum Kind: String, Equatable, Sendable {
        case builtin, ai, script, quicklink, app, calculator, window, layout

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
    }
}

enum ExecOutcome: Sendable {
    case hide
    case toast(String)
    case error(String)
    case output(title: String, text: String)
    case chat(prompt: String?, autoSend: Bool)
    case args(Command)
    case failure(String)

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
            self = .chat(prompt: raw["prompt"] as? String, autoSend: raw["autoSend"] as? Bool ?? false)
        case "args":
            if let command = (raw["command"] as? [String: Any]).flatMap(Command.init) {
                self = .args(command)
            } else {
                self = .failure("Bad arguments payload")
            }
        default:
            self = .failure(raw["error"] as? String ?? "Unknown result")
        }
    }
}
