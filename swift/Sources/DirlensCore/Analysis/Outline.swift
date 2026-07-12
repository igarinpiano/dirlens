// シンボルアウトライン（-O / -A）の正規表現層（Tier2・互換層）。
// rust/crates/dirlens-core/src/analysis/outline.rs の等価移植＋Swift/C 言語を追加。
//
// 既定動作では Scanner.swift の構造走査（文字列・コメントを除いたコード限定
// ビュー上での抽出 = Tier1.5）が使われ、DIRLENS_AST=off / DIRLENS_COMPAT=python
// ではこの正規表現層に固定される（dirlens.py 互換）。

private struct Pat {
    let re: Regexp
    let kind: String
    let nameGroup: Int
}

private let pyPats = [
    Pat(re: Regexp("^(\\s*)class\\s+(\\w+)"), kind: "class", nameGroup: 2),
    Pat(re: Regexp("^(\\s*)(?:async\\s+)?def\\s+(\\w+)\\s*\\("), kind: "def", nameGroup: 2),
]

private let jsPats = [
    Pat(re: Regexp("^\\s*(?:export\\s+)?(?:default\\s+)?class\\s+(\\w+)"), kind: "class", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:export\\s+)?(?:default\\s+)?(?:async\\s+)?function\\s*\\*?\\s+(\\w+)\\s*\\("),
        kind: "func", nameGroup: 1),
    Pat(re: Regexp("^\\s*export\\s+(?:default\\s+)?(?:const|let|var)\\s+(\\w+)\\s*=\\s*(?:async\\s*)?\\("),
        kind: "func", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:const|let|var)\\s+(\\w+)\\s*=\\s*(?:async\\s*)?\\(.*\\)\\s*=>"),
        kind: "func", nameGroup: 1),
]

private let goPats = [
    Pat(re: Regexp("^func\\s+(?:\\([^)]*\\)\\s+)?(\\w+)\\s*\\("), kind: "func", nameGroup: 1),
    Pat(re: Regexp("^type\\s+(\\w+)\\s+struct"), kind: "struct", nameGroup: 1),
    Pat(re: Regexp("^type\\s+(\\w+)\\s+interface"), kind: "interface", nameGroup: 1),
]

private let rsPats = [
    Pat(re: Regexp("^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?fn\\s+(\\w+)"), kind: "fn", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?struct\\s+(\\w+)"), kind: "struct", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?enum\\s+(\\w+)"), kind: "enum", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?trait\\s+(\\w+)"), kind: "trait", nameGroup: 1),
]

// Swift 版で追加した言語（旧 Python 版・Rust 版には無い）
private let swiftPats = [
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)(?:\\(set\\))?\\s+)*(?:final\\s+|static\\s+|class\\s+)*func\\s+([\\w`]+)\\s*[(<]"),
        kind: "func", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*(?:final\\s+|indirect\\s+)*class\\s+(\\w+)"),
        kind: "class", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*struct\\s+(\\w+)"),
        kind: "struct", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*(?:indirect\\s+)?enum\\s+(\\w+)"),
        kind: "enum", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*protocol\\s+(\\w+)"),
        kind: "protocol", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*actor\\s+(\\w+)"),
        kind: "actor", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*extension\\s+([\\w.]+)"),
        kind: "extension", nameGroup: 1),
    Pat(re: Regexp("^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*(?:(?:public|open|internal|fileprivate|private|package)\\s+)*typealias\\s+(\\w+)"),
        kind: "typealias", nameGroup: 1),
]

// C は tree-sitter 相当の代替として控えめな関数/struct 検出のみ
private let cPats = [
    Pat(re: Regexp("^(?:[A-Za-z_][\\w]*[\\s\\*]+)+([A-Za-z_]\\w*)\\s*\\([^;]*$"), kind: "func", nameGroup: 1),
    Pat(re: Regexp("^(?:typedef\\s+)?struct\\s+(\\w+)\\s*\\{"), kind: "struct", nameGroup: 1),
]

private func outlinePats(_ ext: String) -> [Pat]? {
    switch ext {
    case ".py": return pyPats
    case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs": return jsPats
    case ".go": return goPats
    case ".rs": return rsPats
    case ".swift": return swiftPats
    case ".c", ".h": return cPats
    default: return nil
    }
}

private let exportRe = Regexp("\\bexport\\b")
private let pubRe = Regexp("\\bpub\\b")
private let swiftPubRe = Regexp("\\b(?:public|open)\\b")

/// 言語ごとの公開API判定（best-effort、dirlens.py の _is_public_symbol 相当）。
public func isPublicSymbol(_ ext: String, _ line: String, _ name: String) -> Bool {
    switch ext {
    case ".py":
        return !name.hasPrefix("_")
    case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs":
        return exportRe.isMatch(line)
    case ".go":
        guard let first = name.unicodeScalars.first else { return false }
        return first.properties.isUppercase
    case ".rs":
        return pubRe.isMatch(line)
    case ".swift":
        return swiftPubRe.isMatch(line)
    default:
        return true // 不明な場合は除外しない（保守的に倒す）
    }
}

let outlineLimitLines = 4000

/// 対応言語の関数・クラス名を正規表現で簡易抽出する（Tier2・互換層）。
/// 対応外の拡張子は nil（「対応していない」ことを空リストと区別する）。
public func extractOutline(_ text: String, _ ext: String) -> [OutlineItem]? {
    guard let patterns = outlinePats(ext) else { return nil }
    if text.isEmpty { return [] }
    var out: [OutlineItem] = []
    for line in splitLines(text).prefix(outlineLimitLines) {
        let lineStr = line
        for pat in patterns {
            // Python は re.match（行頭アンカー）。パターン自体が ^ 始まりなので同義。
            if let m = pat.re.firstMatch(lineStr) {
                let name = m.count > pat.nameGroup ? (m[pat.nameGroup] ?? "") : ""
                out.append(OutlineItem(pat.kind, name, isPublicSymbol(ext, lineStr, name)))
                break
            }
        }
    }
    return out
}
