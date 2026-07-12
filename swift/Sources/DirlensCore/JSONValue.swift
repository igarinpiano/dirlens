// 挿入順を保持する JSON 値と、serde_json::to_string_pretty /
// Python の json.dumps(..., ensure_ascii=False, indent=2) と同一形式のプリンタ。
//
// `--json` は安定した公開契約（spec §8）なので、キー順・エスケープ・インデントを
// Rust 版（= Python 版）とバイト一致させる。

public indirect enum JSONValue {
    case null
    case bool(Bool)
    case int(Int64)
    case uint(UInt64)
    case string(String)
    case array([JSONValue])
    case object(JSONObject)
}

public struct JSONObject {
    public private(set) var pairs: [(String, JSONValue)] = []
    public init() {}

    public mutating func insert(_ key: String, _ value: JSONValue) {
        if let idx = pairs.firstIndex(where: { $0.0 == key }) {
            pairs[idx].1 = value
        } else {
            pairs.append((key, value))
        }
    }

    public var isEmpty: Bool { pairs.isEmpty }
}

/// serde_json / json.dumps と同一の文字列エスケープ
/// （`"` `\` と 0x20 未満の制御文字のみ。非 ASCII はそのまま = ensure_ascii=False）。
public func jsonEscape(_ s: String) -> String {
    var out = "\""
    out.reserveCapacity(s.utf8.count + 2)
    for c in s.unicodeScalars {
        switch c {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\u{08}": out += "\\b"
        case "\u{09}": out += "\\t"
        case "\u{0a}": out += "\\n"
        case "\u{0c}": out += "\\f"
        case "\u{0d}": out += "\\r"
        default:
            if c.value < 0x20 {
                out += String(format: "\\u%04x", c.value)
            } else {
                out.unicodeScalars.append(c)
            }
        }
    }
    out += "\""
    return out
}

extension JSONValue {
    /// serde_json::to_string_pretty 相当（インデント 2・`": "`・末尾改行なし）。
    public func pretty() -> String {
        var out = ""
        write(to: &out, indent: 0)
        return out
    }

    private func write(to out: inout String, indent: Int) {
        switch self {
        case .null:
            out += "null"
        case .bool(let b):
            out += b ? "true" : "false"
        case .int(let n):
            out += String(n)
        case .uint(let n):
            out += String(n)
        case .string(let s):
            out += jsonEscape(s)
        case .array(let items):
            if items.isEmpty {
                out += "[]"
                return
            }
            out += "[\n"
            let pad = String(repeating: "  ", count: indent + 1)
            for (i, item) in items.enumerated() {
                out += pad
                item.write(to: &out, indent: indent + 1)
                out += i + 1 < items.count ? ",\n" : "\n"
            }
            out += String(repeating: "  ", count: indent) + "]"
        case .object(let obj):
            if obj.isEmpty {
                out += "{}"
                return
            }
            out += "{\n"
            let pad = String(repeating: "  ", count: indent + 1)
            for (i, (k, v)) in obj.pairs.enumerated() {
                out += pad + jsonEscape(k) + ": "
                v.write(to: &out, indent: indent + 1)
                out += i + 1 < obj.pairs.count ? ",\n" : "\n"
            }
            out += String(repeating: "  ", count: indent) + "}"
        }
    }
}

// ─── 最小 JSON パーサ（package.json / tsconfig.json の読込用） ───
// 外部依存なしの方針のため自前実装。挿入順は JSONObject が保持する。

public enum JSONParser {
    public static func parse(_ text: String) -> JSONValue? {
        var scalars = Array(text.unicodeScalars)
        var i = 0
        skipWS(scalars, &i)
        guard let v = parseValue(&scalars, &i) else { return nil }
        skipWS(scalars, &i)
        return i == scalars.count ? v : nil
    }

    private static func skipWS(_ s: [Unicode.Scalar], _ i: inout Int) {
        while i < s.count, s[i] == " " || s[i] == "\t" || s[i] == "\n" || s[i] == "\r" {
            i += 1
        }
    }

    private static func parseValue(_ s: inout [Unicode.Scalar], _ i: inout Int) -> JSONValue? {
        guard i < s.count else { return nil }
        switch s[i] {
        case "{": return parseObject(&s, &i)
        case "[": return parseArray(&s, &i)
        case "\"": return parseString(&s, &i).map { .string($0) }
        case "t":
            if matchLit(&s, &i, "true") { return .bool(true) }
            return nil
        case "f":
            if matchLit(&s, &i, "false") { return .bool(false) }
            return nil
        case "n":
            if matchLit(&s, &i, "null") { return .null }
            return nil
        default:
            return parseNumber(&s, &i)
        }
    }

    private static func matchLit(_ s: inout [Unicode.Scalar], _ i: inout Int, _ lit: String) -> Bool {
        let l = Array(lit.unicodeScalars)
        guard i + l.count <= s.count else { return false }
        for (k, c) in l.enumerated() where s[i + k] != c {
            return false
        }
        i += l.count
        return true
    }

    private static func parseNumber(_ s: inout [Unicode.Scalar], _ i: inout Int) -> JSONValue? {
        let start = i
        if i < s.count, s[i] == "-" { i += 1 }
        var isDouble = false
        while i < s.count {
            let c = s[i]
            if c.value >= 48, c.value <= 57 {
                i += 1
            } else if c == "." || c == "e" || c == "E" || c == "+" || c == "-" {
                isDouble = true
                i += 1
            } else {
                break
            }
        }
        guard i > start else { return nil }
        var v = String.UnicodeScalarView()
        v.append(contentsOf: s[start..<i])
        let str = String(v)
        if !isDouble, let n = Int64(str) {
            return .int(n)
        }
        if let d = Double(str) {
            return .int(pyTrunc(d))
        }
        return nil
    }

    private static func parseString(_ s: inout [Unicode.Scalar], _ i: inout Int) -> String? {
        guard i < s.count, s[i] == "\"" else { return nil }
        i += 1
        var out = String.UnicodeScalarView()
        while i < s.count {
            let ch = s[i]
            if ch == "\"" {
                i += 1
                return String(out)
            }
            if ch == "\\" {
                i += 1
                guard i < s.count else { return nil }
                switch s[i] {
                case "\"": out.append("\"")
                case "\\": out.append("\\")
                case "/": out.append("/")
                case "b": out.append("\u{08}")
                case "f": out.append("\u{0c}")
                case "n": out.append("\n")
                case "r": out.append("\r")
                case "t": out.append("\t")
                case "u":
                    guard i + 4 < s.count else { return nil }
                    var hex = String.UnicodeScalarView()
                    hex.append(contentsOf: s[(i + 1)...(i + 4)])
                    guard let code = UInt32(String(hex), radix: 16) else { return nil }
                    i += 4
                    var value = code
                    // サロゲートペア
                    if (0xD800...0xDBFF).contains(code), i + 6 < s.count, s[i + 1] == "\\", s[i + 2] == "u" {
                        var hex2 = String.UnicodeScalarView()
                        hex2.append(contentsOf: s[(i + 3)...(i + 6)])
                        if let low = UInt32(String(hex2), radix: 16), (0xDC00...0xDFFF).contains(low) {
                            value = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                            i += 6
                        }
                    }
                    out.append(Unicode.Scalar(value) ?? "\u{FFFD}")
                default:
                    return nil
                }
                i += 1
            } else {
                out.append(ch)
                i += 1
            }
        }
        return nil
    }

    private static func parseArray(_ s: inout [Unicode.Scalar], _ i: inout Int) -> JSONValue? {
        i += 1 // '['
        var items: [JSONValue] = []
        skipWS(s, &i)
        if i < s.count, s[i] == "]" {
            i += 1
            return .array(items)
        }
        while true {
            skipWS(s, &i)
            guard let v = parseValue(&s, &i) else { return nil }
            items.append(v)
            skipWS(s, &i)
            guard i < s.count else { return nil }
            if s[i] == "," {
                i += 1
            } else if s[i] == "]" {
                i += 1
                return .array(items)
            } else {
                return nil
            }
        }
    }

    private static func parseObject(_ s: inout [Unicode.Scalar], _ i: inout Int) -> JSONValue? {
        i += 1 // '{'
        var obj = JSONObject()
        skipWS(s, &i)
        if i < s.count, s[i] == "}" {
            i += 1
            return .object(obj)
        }
        while true {
            skipWS(s, &i)
            guard let key = parseString(&s, &i) else { return nil }
            skipWS(s, &i)
            guard i < s.count, s[i] == ":" else { return nil }
            i += 1
            skipWS(s, &i)
            guard let v = parseValue(&s, &i) else { return nil }
            obj.insert(key, v)
            skipWS(s, &i)
            guard i < s.count else { return nil }
            if s[i] == "," {
                i += 1
            } else if s[i] == "}" {
                i += 1
                return .object(obj)
            } else {
                return nil
            }
        }
    }
}

extension JSONValue {
    public var asObject: JSONObject? {
        if case .object(let o) = self { return o }
        return nil
    }
    public var asArray: [JSONValue]? {
        if case .array(let a) = self { return a }
        return nil
    }
    public var asString: String? {
        if case .string(let s) = self { return s }
        return nil
    }
    public func get(_ key: String) -> JSONValue? {
        guard case .object(let o) = self else { return nil }
        return o.pairs.first(where: { $0.0 == key })?.1
    }
}
