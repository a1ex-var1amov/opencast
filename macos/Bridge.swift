import Foundation

func opencastCall(_ json: String) -> String {
    json.withCString { ptr in
        guard let out = opencast_call(ptr) else { return #"{"error":"null"}"# }
        defer { opencast_free(out) }
        return String(cString: out)
    }
}

enum Engine {
    static func call(_ body: [String: Any]) -> [String: Any] {
        let data = try! JSONSerialization.data(withJSONObject: body)
        let text = String(data: data, encoding: .utf8)!
        let raw = opencastCall(text)
        let parsed = try? JSONSerialization.jsonObject(with: Data(raw.utf8))
        return parsed as? [String: Any] ?? ["error": raw]
    }
}
