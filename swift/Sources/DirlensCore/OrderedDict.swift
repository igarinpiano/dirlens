// 挿入順を保持する辞書（Rust 版の indexmap::IndexMap 相当の最小実装）。

public struct OrderedDict<Value> {
    public private(set) var keys: [String] = []
    private var map: [String: Value] = [:]

    public init() {}

    public var isEmpty: Bool { keys.isEmpty }
    public var count: Int { keys.count }

    public subscript(key: String) -> Value? {
        get { map[key] }
        set {
            if let newValue {
                if map[key] == nil {
                    keys.append(key)
                }
                map[key] = newValue
            } else if map[key] != nil {
                map[key] = nil
                if let idx = keys.firstIndex(of: key) {
                    keys.remove(at: idx)
                }
            }
        }
    }

    /// entry(key).or_insert(default) 相当。既存値または default を挿入して返す。
    public mutating func entry(_ key: String, default def: Value) -> Value {
        if let v = map[key] { return v }
        keys.append(key)
        map[key] = def
        return def
    }

    /// 値を更新する（無ければ default から）。
    public mutating func update(_ key: String, default def: Value, _ f: (inout Value) -> Void) {
        var v = map[key] ?? {
            keys.append(key)
            return def
        }()
        f(&v)
        map[key] = v
    }

    /// 挿入順の (key, value) 列。
    public var pairs: [(String, Value)] {
        keys.map { ($0, map[$0]!) }
    }
}
