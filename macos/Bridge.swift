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
        var body = body
        if body["cid"] == nil {
            body["cid"] = UUID().uuidString
        }
        let data: Data
        do {
            data = try JSONSerialization.data(withJSONObject: body)
        } catch {
            return ["error": "could not encode request"]
        }
        guard let text = String(data: data, encoding: .utf8) else {
            return ["error": "could not encode request"]
        }
        let raw = opencastCall(text)
        let parsed = try? JSONSerialization.jsonObject(with: Data(raw.utf8))
        return parsed as? [String: Any] ?? ["error": raw]
    }
}
