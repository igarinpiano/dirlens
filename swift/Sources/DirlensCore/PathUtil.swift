// パス補助（String ベース・POSIX "/" 区切り）。
// Rust 版の std::path 利用箇所（components / file_name / join / relpath）に対応する。

public let mainSeparator = "/"

/// パスをコンポーネント列に分解する（Rust の Path::components 相当）。
/// 絶対パスは先頭に "/" コンポーネントを持つ。"." や空要素は除去する。
public func pathComponents(_ path: String) -> [String] {
    var comps: [String] = []
    if path.hasPrefix("/") {
        comps.append("/")
    }
    for part in path.split(separator: "/") where part != "." {
        comps.append(String(part))
    }
    return comps
}

/// 最終コンポーネント（Rust の file_name 相当）。"/" などでは nil。
public func fileName(_ path: String) -> String? {
    let comps = pathComponents(path)
    guard let last = comps.last, last != "/", last != ".." else { return nil }
    return last
}

/// パス結合（Rust の Path::join 相当。target が絶対ならそれを返す）。
public func joinPath(_ base: String, _ child: String) -> String {
    if child.hasPrefix("/") { return child }
    if base.isEmpty { return child }
    if base.hasSuffix("/") { return base + child }
    return base + "/" + child
}

/// os.path.relpath 相当（共通接頭辞方式・"/" で連結）。
public func relpath(_ path: String, _ start: String) -> String {
    let p = pathComponents(path)
    let s = pathComponents(start)
    var i = 0
    while i < p.count, i < s.count, p[i] == s[i] {
        i += 1
    }
    var parts = Array(repeating: "..", count: s.count - i)
    parts.append(contentsOf: p[i...])
    if parts.isEmpty {
        return "."
    }
    return parts.joined(separator: mainSeparator)
}

/// relpath を "/" 区切りに正規化したもの（POSIX では relpath と同一）。
public func relpathSlash(_ path: String, _ start: String) -> String {
    return relpath(path, start)
}
