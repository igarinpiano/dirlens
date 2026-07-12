// TODO/FIXME 抽出（-K）。rust/crates/dirlens-core/src/analysis/todo.rs の等価移植。

private let todoRe = Regexp("\\b(TODO|FIXME|HACK|XXX)\\b", caseInsensitive: true)

/// 戻り値: (行番号, 種別（大文字化）, スニペット)
public func scanTodos(_ text: String) -> [(Int, String, String)] {
    if text.isEmpty { return [] }
    var results: [(Int, String, String)] = []
    for (i, line) in splitLines(text).enumerated() {
        let lineStr = line
        if let m = todoRe.firstMatch(lineStr), let kind = m[1] {
            var snippet = pyStrip(lineStr)
            if charLen(snippet) > 80 {
                let (head, _) = truncateChars(snippet, 80)
                snippet = head + "…"
            }
            results.append((i + 1, kind.uppercased(), snippet))
        }
    }
    return results
}
