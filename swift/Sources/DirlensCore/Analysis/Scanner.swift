// 構造走査層（Tier1.5・ゼロ依存）。
//
// Rust 版の「言語別最良パーサ（ruff/oxc/syn/tree-sitter）」に相当する精度を
// 外部依存なしで実現するため、まず言語ごとの字句規則で「コメントを除去し、
// 文字列リテラルの中身を空白化したコード限定ビュー（マスク）」を作り、
// その上でシンボル・import を抽出する。
//
// - 文字列リテラル内の偽シンボル / 偽 import を拾わない（AST 層と同じ利点）。
// - パース不能なコードでも行単位で抽出できる（正規表現層と同じ資質 = 縮退不要）。
// - さらに上の Tier1（外部ツール: python3 の ast / node+typescript）が使える環境では
//   そちらが優先される（Provider.swift の AstProvider）。

// ─── コード限定ビュー ─────────────────────────────────────────

public struct MaskedSource {
    /// 元テキスト（スカラー配列）
    public let original: [Unicode.Scalar]
    /// コメント→空白、文字列の中身→空白（引用符自体は残す）にしたビュー
    public let masked: [Unicode.Scalar]
    /// 文字列リテラルの中身の範囲（original のスカラー添字）
    public let stringRanges: [Range<Int>]
    /// masked を文字列化したもの
    public let maskedText: String

    init(original: [Unicode.Scalar], masked: [Unicode.Scalar], stringRanges: [Range<Int>]) {
        self.original = original
        self.masked = masked
        self.stringRanges = stringRanges
        var v = String.UnicodeScalarView()
        v.append(contentsOf: masked)
        self.maskedText = String(v)
    }

    /// original の範囲を文字列として取り出す。
    public func slice(_ r: Range<Int>) -> String {
        var v = String.UnicodeScalarView()
        v.append(contentsOf: original[r])
        return String(v)
    }

    /// idx 位置から始まる文字列リテラルの中身（idx は開始引用符の次を指す）。
    func stringContent(startingAt idx: Int) -> String? {
        for r in stringRanges where r.lowerBound == idx {
            return slice(r)
        }
        return nil
    }
}

private func isIdentScalar(_ c: Unicode.Scalar) -> Bool {
    return c == "_" || c == "$" || c.properties.isAlphabetic
        || (c.value >= 48 && c.value <= 57)
}

private func isDigit(_ c: Unicode.Scalar) -> Bool {
    return c.value >= 48 && c.value <= 57
}

private final class Masker {
    let src: [Unicode.Scalar]
    var out: [Unicode.Scalar]
    var ranges: [Range<Int>] = []
    var i = 0

    init(_ text: String) {
        src = Array(text.unicodeScalars)
        out = src
    }

    var eof: Bool { i >= src.count }

    func peek(_ off: Int = 0) -> Unicode.Scalar? {
        let j = i + off
        return j < src.count ? src[j] : nil
    }

    /// i から n 文字が s と一致するか
    func lookahead(_ s: String, _ off: Int = 0) -> Bool {
        let target = Array(s.unicodeScalars)
        let start = i + off
        guard start + target.count <= src.count else { return false }
        for (k, c) in target.enumerated() where src[start + k] != c {
            return false
        }
        return true
    }

    func blank(_ idx: Int) {
        if src[idx] != "\n" && src[idx] != "\r" {
            out[idx] = " "
        }
    }

    /// 行コメント（開始位置から行末まで空白化）
    func lineComment() {
        while i < src.count, src[i] != "\n" {
            blank(i)
            i += 1
        }
    }

    /// ブロックコメント。nested = true なら入れ子対応（Rust / Swift）。
    func blockComment(nested: Bool) {
        blank(i); blank(i + 1)
        i += 2
        var depth = 1
        while i < src.count, depth > 0 {
            if lookahead("*/") {
                blank(i); blank(i + 1)
                i += 2
                depth -= 1
            } else if nested, lookahead("/*") {
                blank(i); blank(i + 1)
                i += 2
                depth += 1
            } else {
                blank(i)
                i += 1
            }
        }
    }

    /// 引用符 quote で囲まれた 1 行文字列（escapes 有効時 \ を処理）。
    /// 開始引用符の位置で呼ぶ。行末で打ち切る（壊れたコードへの耐性）。
    func singleLineString(_ quote: Unicode.Scalar, escapes: Bool) {
        i += 1 // open quote は残す
        let contentStart = i
        while i < src.count {
            let ch = src[i]
            if ch == "\n" { break } // 未終端
            if escapes, ch == "\\", i + 1 < src.count, src[i + 1] != "\n" {
                blank(i); blank(i + 1)
                i += 2
                continue
            }
            if ch == quote {
                ranges.append(contentStart..<i)
                i += 1 // close quote は残す
                return
            }
            blank(i)
            i += 1
        }
        ranges.append(contentStart..<min(i, src.count))
    }

    /// 複数行文字列（終端 delimiter まで。escapes 有効時 \ を処理）。
    /// 開始 delimiter の直後の位置で呼ぶ。
    func multiLineString(until delimiter: String, escapes: Bool) {
        let contentStart = i
        while i < src.count {
            if escapes, src[i] == "\\", i + 1 < src.count {
                blank(i); blank(i + 1)
                i += 2
                continue
            }
            if lookahead(delimiter) {
                ranges.append(contentStart..<i)
                i += Array(delimiter.unicodeScalars).count
                return
            }
            blank(i)
            i += 1
        }
        ranges.append(contentStart..<src.count)
    }

    func finish() -> MaskedSource {
        MaskedSource(original: src, masked: out, stringRanges: ranges)
    }
}

/// Python: # コメント、' " と ''' """（r/b/f プレフィックス対応）。
public func maskPython(_ text: String) -> MaskedSource {
    let m = Masker(text)
    while !m.eof {
        let c = m.peek()!
        if c == "#" {
            m.lineComment()
        } else if c == "'" || c == "\"" {
            // 直前の識別子に r/R が含まれるか（raw 文字列判定）
            var raw = false
            var k = m.i - 1
            var prefix: [Unicode.Scalar] = []
            while k >= 0, prefix.count < 3, m.src[k].properties.isAlphabetic {
                prefix.append(m.src[k])
                k -= 1
            }
            if k < 0 || !isIdentScalar(m.src[k]) {
                raw = prefix.contains("r") || prefix.contains("R")
            }
            let q = String(c)
            if m.lookahead(q + q + q) {
                m.i += 3
                m.multiLineString(until: q + q + q, escapes: !raw)
            } else {
                m.singleLineString(c, escapes: !raw)
            }
        } else {
            m.i += 1
        }
    }
    return m.finish()
}

/// JS/TS: // と /* */、' " ` （テンプレートは ${} 込みで空白化）、正規表現リテラル。
public func maskJs(_ text: String) -> MaskedSource {
    let m = Masker(text)
    // 直前の「意味のある」コード文字（正規表現リテラル判定用）
    var prevCode: Unicode.Scalar? = nil
    var prevToken = ""

    func template() {
        // 開始 ` の位置で呼ぶ。${ } の入れ子・ネストしたテンプレートも丸ごと空白化。
        m.i += 1
        let contentStart = m.i
        var braceDepth = 0
        while !m.eof {
            let ch = m.peek()!
            if ch == "\\", m.i + 1 < m.src.count {
                m.blank(m.i); m.blank(m.i + 1)
                m.i += 2
                continue
            }
            if braceDepth == 0, ch == "`" {
                m.ranges.append(contentStart..<m.i)
                m.i += 1
                return
            }
            if m.lookahead("${") {
                braceDepth += 1
                m.blank(m.i); m.blank(m.i + 1)
                m.i += 2
                continue
            }
            if braceDepth > 0 {
                if ch == "{" { braceDepth += 1 }
                if ch == "}" { braceDepth -= 1 }
            }
            m.blank(m.i)
            m.i += 1
        }
        m.ranges.append(contentStart..<m.src.count)
    }

    func regexLiteral() {
        // 開始 / の位置で呼ぶ。[...] クラスとエスケープを処理。
        m.blank(m.i)
        m.i += 1
        var inClass = false
        while !m.eof {
            let ch = m.peek()!
            if ch == "\n" { return } // 未終端 → 中断
            if ch == "\\", m.i + 1 < m.src.count {
                m.blank(m.i); m.blank(m.i + 1)
                m.i += 2
                continue
            }
            if ch == "[" { inClass = true }
            if ch == "]" { inClass = false }
            if ch == "/", !inClass {
                m.blank(m.i)
                m.i += 1
                // フラグ
                while !m.eof, m.peek()!.properties.isAlphabetic {
                    m.blank(m.i)
                    m.i += 1
                }
                return
            }
            m.blank(m.i)
            m.i += 1
        }
    }

    let regexPrevTokens: Set<String> = [
        "return", "typeof", "case", "in", "of", "new", "delete", "void",
        "instanceof", "do", "else", "yield", "await", "throw",
    ]

    while !m.eof {
        let c = m.peek()!
        if m.lookahead("//") {
            m.lineComment()
        } else if m.lookahead("/*") {
            m.blockComment(nested: false)
        } else if c == "'" || c == "\"" {
            m.singleLineString(c, escapes: true)
            prevCode = c
            prevToken = ""
        } else if c == "`" {
            template()
            prevCode = "`"
            prevToken = ""
        } else if c == "/" {
            // 正規表現 or 除算: 直前トークンで判定する古典的ヒューリスティック
            let isRegex: Bool
            if let p = prevCode {
                if isIdentScalar(p) || p == ")" || p == "]" || p == "\"" || p == "'" || p == "`" {
                    isRegex = regexPrevTokens.contains(prevToken)
                } else {
                    isRegex = true
                }
            } else {
                isRegex = true
            }
            if isRegex {
                regexLiteral()
            } else {
                m.i += 1
            }
            prevCode = "/"
            prevToken = ""
        } else {
            if isIdentScalar(c) {
                if prevToken.isEmpty || !(prevCode.map(isIdentScalar) ?? false) {
                    prevToken = String(c)
                } else {
                    prevToken.unicodeScalars.append(c)
                }
                prevCode = c
            } else if !c.properties.isWhitespace {
                prevCode = c
                prevToken = ""
            }
            m.i += 1
        }
    }
    return m.finish()
}

/// Rust: // と /* */（入れ子）、" と r#"..."#、char リテラル（lifetime と区別）。
public func maskRust(_ text: String) -> MaskedSource {
    let m = Masker(text)
    while !m.eof {
        let c = m.peek()!
        if m.lookahead("//") {
            m.lineComment()
        } else if m.lookahead("/*") {
            m.blockComment(nested: true)
        } else if c == "r" || c == "b" {
            // raw / byte 文字列: r"..." r#"..."# br"..." など
            var off = 1
            if c == "b", m.peek(1) == "r" { off = 2 }
            var hashes = 0
            while m.peek(off + hashes) == "#" { hashes += 1 }
            if m.peek(off + hashes) == "\"",
               m.i == 0 || !isIdentScalar(m.src[m.i - 1]) {
                m.i += off + hashes + 1
                m.multiLineString(until: "\"" + String(repeating: "#", count: hashes), escapes: false)
            } else if c == "b", m.peek(1) == "\"", m.i == 0 || !isIdentScalar(m.src[m.i - 1]) {
                m.i += 1
                m.singleLineString("\"", escapes: true)
            } else {
                m.i += 1
            }
        } else if c == "\"" {
            m.i += 1
            m.multiLineString(until: "\"", escapes: true)
        } else if c == "'" {
            // char リテラル vs lifetime
            if m.peek(1) == "\\" {
                // '\n' '\u{...}' など: 次の ' まで
                var j = m.i + 2
                while j < m.src.count, m.src[j] != "'", j - m.i < 12 { j += 1 }
                if j < m.src.count, m.src[j] == "'" {
                    for k in (m.i + 1)..<j { m.blank(k) }
                    m.i = j + 1
                    continue
                }
                m.i += 1
            } else if let ch1 = m.peek(1), m.peek(2) == "'", ch1 != "'" {
                m.blank(m.i + 1)
                m.i += 3
            } else {
                m.i += 1 // lifetime
            }
        } else {
            m.i += 1
        }
    }
    return m.finish()
}

/// Go: // と /* */、" ' と ` raw 文字列。
public func maskGo(_ text: String) -> MaskedSource {
    let m = Masker(text)
    while !m.eof {
        let c = m.peek()!
        if m.lookahead("//") {
            m.lineComment()
        } else if m.lookahead("/*") {
            m.blockComment(nested: false)
        } else if c == "\"" || c == "'" {
            m.singleLineString(c, escapes: true)
        } else if c == "`" {
            m.i += 1
            m.multiLineString(until: "`", escapes: false)
        } else {
            m.i += 1
        }
    }
    return m.finish()
}

/// C: // と /* */、" '。
public func maskC(_ text: String) -> MaskedSource {
    let m = Masker(text)
    while !m.eof {
        let c = m.peek()!
        if m.lookahead("//") {
            m.lineComment()
        } else if m.lookahead("/*") {
            m.blockComment(nested: false)
        } else if c == "\"" || c == "'" {
            m.singleLineString(c, escapes: true)
        } else {
            m.i += 1
        }
    }
    return m.finish()
}

/// Swift: // と /* */（入れ子）、" と """ と #"..."#（補間 \(...) 込みで空白化）。
public func maskSwift(_ text: String) -> MaskedSource {
    let m = Masker(text)

    func swiftString(multi: Bool, hashes: Int) {
        // 開始デリミタの直後の位置で呼ぶ。
        let close = (multi ? "\"\"\"" : "\"") + String(repeating: "#", count: hashes)
        let escape = "\\" + String(repeating: "#", count: hashes)
        let contentStart = m.i
        var parenDepth = 0
        while !m.eof {
            if parenDepth == 0, m.lookahead(close) {
                m.ranges.append(contentStart..<m.i)
                m.i += Array(close.unicodeScalars).count
                return
            }
            if !multi, parenDepth == 0, m.peek() == "\n" {
                m.ranges.append(contentStart..<m.i) // 未終端
                return
            }
            if m.lookahead(escape + "(") {
                parenDepth += 1
                for k in 0..<(escape.count + 1) { m.blank(m.i + k) }
                m.i += escape.count + 1
                continue
            }
            if m.lookahead(escape), m.i + escape.count < m.src.count {
                for k in 0..<(escape.count + 1) { m.blank(m.i + k) }
                m.i += escape.count + 1
                continue
            }
            if parenDepth > 0 {
                if m.peek() == "(" { parenDepth += 1 }
                if m.peek() == ")" { parenDepth -= 1 }
            }
            m.blank(m.i)
            m.i += 1
        }
        m.ranges.append(contentStart..<m.src.count)
    }

    while !m.eof {
        let c = m.peek()!
        if m.lookahead("//") {
            m.lineComment()
        } else if m.lookahead("/*") {
            m.blockComment(nested: true)
        } else if c == "#" {
            var hashes = 0
            while m.peek(hashes) == "#" { hashes += 1 }
            if m.peek(hashes) == "\"" {
                if m.lookahead("\"\"\"", hashes) {
                    m.i += hashes + 3
                    swiftString(multi: true, hashes: hashes)
                } else {
                    m.i += hashes + 1
                    swiftString(multi: false, hashes: hashes)
                }
            } else {
                m.i += 1
            }
        } else if c == "\"" {
            if m.lookahead("\"\"\"") {
                m.i += 3
                swiftString(multi: true, hashes: 0)
            } else {
                m.i += 1
                swiftString(multi: false, hashes: 0)
            }
        } else {
            m.i += 1
        }
    }
    return m.finish()
}

public func maskSource(_ text: String, _ ext: String) -> MaskedSource? {
    switch ext {
    case ".py": return maskPython(text)
    case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs": return maskJs(text)
    case ".rs": return maskRust(text)
    case ".go": return maskGo(text)
    case ".c", ".h": return maskC(text)
    case ".swift": return maskSwift(text)
    default: return nil
    }
}

// ─── 構造走査: アウトライン ───────────────────────────────────

/// Python / Rust / Go / C / Swift: コード限定ビュー上で正規表現層と同じ
/// パターンを流す（文字列内の偽シンボルを拾わない）。
public func structOutline(_ text: String, _ ext: String) -> [OutlineItem]? {
    switch ext {
    case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs":
        guard let masked = maskSource(text, ext) else { return nil }
        return jsStructOutline(masked, ext)
    default:
        guard let masked = maskSource(text, ext) else { return nil }
        return extractOutline(masked.maskedText, ext)
    }
}

// JS/TS のトップレベル宣言抽出（oxc 版と同じ対象: class / function /
// 関数で初期化された const・let・var。export されたものは public 扱い）。
private let jsClassRe = Regexp(
    "^\\s*(?:export\\s+)?(?:default\\s+)?(?:abstract\\s+)?class\\s+([A-Za-z_$][\\w$]*)")
private let jsFuncRe = Regexp(
    "^\\s*(?:export\\s+)?(?:default\\s+)?(?:async\\s+)?function\\s*\\*?\\s*([A-Za-z_$][\\w$]*)\\s*\\(")
private let jsVarRe = Regexp(
    "^\\s*(?:export\\s+)?(?:declare\\s+)?(?:const|let|var)\\s+([A-Za-z_$][\\w$]*)[^=\\n]*=\\s*")
private let jsExportRe = Regexp("^\\s*export\\b")

func jsStructOutline(_ src: MaskedSource, _ ext: String) -> [OutlineItem] {
    var out: [OutlineItem] = []
    let masked = src.masked

    // 各行の開始オフセットと開始ネスト深さ（{ } ( ) [ ] を数える。
    // 文字列の中身は空白化済みなので単純にカウントしてよい）
    var lineStarts: [Int] = [0]
    for (i, c) in masked.enumerated() where c == "\n" {
        lineStarts.append(i + 1)
    }
    var depthAt: [Int] = []
    depthAt.reserveCapacity(lineStarts.count)
    var depth = 0
    var li = 0
    for (i, c) in masked.enumerated() {
        while li < lineStarts.count, lineStarts[li] == i {
            depthAt.append(depth)
            li += 1
        }
        switch c {
        case "{", "(", "[": depth += 1
        case "}", ")", "]": depth = max(0, depth - 1)
        default: break
        }
    }
    while depthAt.count < lineStarts.count { depthAt.append(depth) }

    func lineString(_ idx: Int) -> String {
        let start = lineStarts[idx]
        let end = idx + 1 < lineStarts.count ? lineStarts[idx + 1] - 1 : masked.count
        var v = String.UnicodeScalarView()
        v.append(contentsOf: masked[start..<max(start, end)])
        return String(v)
    }

    // 初期化子が関数（アロー/関数式）かどうかを masked 上で判定する
    func initializerIsFunction(_ pos: Int) -> Bool {
        var i = pos
        func skipWs() {
            while i < masked.count, masked[i].properties.isWhitespace { i += 1 }
        }
        func matchesWord(_ w: String) -> Bool {
            let scalars = Array(w.unicodeScalars)
            guard i + scalars.count <= masked.count else { return false }
            for (k, c) in scalars.enumerated() where masked[i + k] != c {
                return false
            }
            let after = i + scalars.count
            return after >= masked.count || !isIdentScalar(masked[after])
        }
        skipWs()
        if matchesWord("async") {
            i += 5
            skipWs()
        }
        if matchesWord("function") { return true }
        if i < masked.count, masked[i] == "(" {
            // 対応する ) を探し、その後（省略可能な TS 戻り型を挟んで）=> が来るか
            var d = 0
            while i < masked.count {
                if masked[i] == "(" { d += 1 }
                if masked[i] == ")" {
                    d -= 1
                    if d == 0 { break }
                }
                i += 1
            }
            guard i < masked.count else { return false }
            i += 1
            skipWs()
            if i + 1 < masked.count, masked[i] == "=", masked[i + 1] == ">" { return true }
            if i < masked.count, masked[i] == ":" {
                // TS 戻り型注釈: `): Kind =>` — 文末より手前に => があるか
                var j = i
                var jd = 0
                while j + 1 < masked.count, j - i < 512 {
                    let ch = masked[j]
                    if ch == "(" || ch == "{" || ch == "[" || ch == "<" { jd += 1 }
                    if ch == ")" || ch == "}" || ch == "]" || ch == ">" { jd = max(0, jd - 1) }
                    if jd == 0, ch == "=", masked[j + 1] == ">" { return true }
                    if jd == 0, ch == ";" || ch == "\n" { return false }
                    j += 1
                }
            }
            return false
        }
        // 単一引数のアロー: `x => ...`
        if i < masked.count, isIdentScalar(masked[i]), !isDigit(masked[i]) {
            let start = i
            while i < masked.count, isIdentScalar(masked[i]) { i += 1 }
            // キーワード（new 等）で始まる式は対象外
            let word = String(String.UnicodeScalarView(masked[start..<i]))
            let keywords: Set<String> = ["new", "await", "typeof", "void", "delete", "yield"]
            if keywords.contains(word) { return false }
            skipWs()
            if i + 1 < masked.count, masked[i] == "=", masked[i + 1] == ">" { return true }
        }
        return false
    }

    for idx in 0..<lineStarts.count where depthAt[idx] == 0 {
        let line = lineString(idx)
        let exported = jsExportRe.isMatch(line)
        if let m = jsClassRe.firstMatch(line), let name = m[1] {
            out.append(OutlineItem("class", name, exported))
            continue
        }
        if let m = jsFuncRe.firstMatch(line), let name = m[1] {
            out.append(OutlineItem("func", name, exported))
            continue
        }
        if let m = jsVarRe.firstMatch(line), let name = m[1], let whole = m[0] {
            // 初期化子の開始位置（行頭からのオフセット）
            let initOffset = lineStarts[idx] + whole.unicodeScalars.count
            if initializerIsFunction(initOffset) {
                out.append(OutlineItem("func", name, exported))
            }
            continue
        }
    }
    return out
}

// ─── 構造走査: import 抽出 ────────────────────────────────────

/// Python: コード限定ビューに行ベースパーサ（AST の Import/ImportFrom 相当）。
public func pyStructImports(_ text: String) -> [(String, UInt32, [String]?)] {
    let masked = maskPython(text)
    return extractImportsPyLines(masked.maskedText)
}

/// JS/TS: 文レベルの import/export と require()/動的 import() をトークン走査で抽出。
/// 文字列リテラル内の偽 import は拾わない。順序は Rust AST 層と同じ
/// （文レベル → require → 動的 import）。
public func jsStructImports(_ text: String, _ ext: String) -> [String] {
    let src = maskJs(text)
    let masked = src.masked
    var stmts: [String] = []
    var requires: [String] = []
    var dyns: [String] = []

    var depth = 0
    var i = 0
    var atStmtStart = true

    func skipWs(_ j: inout Int) {
        while j < masked.count, masked[j].properties.isWhitespace { j += 1 }
    }

    /// j が引用符位置なら、その文字列の中身を original から取り出す
    func stringAt(_ j: Int) -> String? {
        guard j < masked.count else { return nil }
        let c = masked[j]
        guard c == "\"" || c == "'" || c == "`" else { return nil }
        return src.stringContent(startingAt: j + 1)
    }

    while i < masked.count {
        let c = masked[i]
        if isIdentScalar(c) {
            let start = i
            while i < masked.count, isIdentScalar(masked[i]) { i += 1 }
            let word = String(String.UnicodeScalarView(masked[start..<i]))
            let prevOk = start == 0 || !isIdentScalar(masked[start - 1])
            if prevOk {
                switch word {
                case "import":
                    var j = i
                    skipWs(&j)
                    if j < masked.count, masked[j] == "(" {
                        // 動的 import()
                        var k = j + 1
                        skipWs(&k)
                        if let s = stringAt(k) { dyns.append(s) }
                    } else if depth == 0, atStmtStart {
                        // 文レベル import: 文中最初の文字列
                        var k = j
                        var guardLimit = 0
                        while k < masked.count, guardLimit < 4096 {
                            if masked[k] == ";" { break }
                            if let s = stringAt(k) {
                                stmts.append(s)
                                break
                            }
                            k += 1
                            guardLimit += 1
                        }
                    }
                case "export":
                    if depth == 0, atStmtStart {
                        // `export ... from 'x'` のみ import として扱う
                        var k = i
                        var sawFrom = false
                        var d = 0
                        var guardLimit = 0
                        while k < masked.count, guardLimit < 4096 {
                            let ch = masked[k]
                            if ch == "{" || ch == "(" || ch == "[" { d += 1 }
                            if ch == "}" || ch == ")" || ch == "]" { d = max(0, d - 1) }
                            if d == 0, ch == ";" { break }
                            if d == 0, isIdentScalar(ch), k == 0 || !isIdentScalar(masked[k - 1]) {
                                var e = k
                                while e < masked.count, isIdentScalar(masked[e]) { e += 1 }
                                let w = String(String.UnicodeScalarView(masked[k..<e]))
                                if w == "from" { sawFrom = true }
                                // 宣言 export はここで打ち切り
                                if ["class", "function", "const", "let", "var", "interface",
                                    "type", "enum", "namespace", "abstract", "declare"].contains(w) {
                                    break
                                }
                                k = e
                                guardLimit += e - k + 1
                                continue
                            }
                            if d == 0, sawFrom, let s = stringAt(k) {
                                stmts.append(s)
                                break
                            }
                            k += 1
                            guardLimit += 1
                        }
                    }
                case "require":
                    var j = i
                    skipWs(&j)
                    if j < masked.count, masked[j] == "(" {
                        var k = j + 1
                        skipWs(&k)
                        if let s = stringAt(k) { requires.append(s) }
                    }
                default:
                    break
                }
            }
            atStmtStart = false
            continue
        }
        switch c {
        case "{", "(", "[":
            depth += 1
            atStmtStart = false
        case "}", ")", "]":
            depth = max(0, depth - 1)
            atStmtStart = true
        case ";", "\n":
            atStmtStart = true
        default:
            if !c.properties.isWhitespace {
                atStmtStart = false
            }
        }
        i += 1
    }
    return stmts + requires + dyns
}

/// Go: import ブロック / 単行 import をトークン走査で抽出（文字列内は拾わない）。
public func goStructImports(_ text: String) -> [String] {
    let src = maskGo(text)
    let masked = src.masked
    var out: [String] = []
    var i = 0
    while i < masked.count {
        if isIdentScalar(masked[i]) {
            let start = i
            while i < masked.count, isIdentScalar(masked[i]) { i += 1 }
            let word = String(String.UnicodeScalarView(masked[start..<i]))
            let prevOk = start == 0 || !isIdentScalar(masked[start - 1])
            guard prevOk, word == "import" else { continue }
            var j = i
            while j < masked.count, masked[j].properties.isWhitespace { j += 1 }
            if j < masked.count, masked[j] == "(" {
                // ブロック: 対応する ) までの全文字列
                var k = j + 1
                while k < masked.count, masked[k] != ")" {
                    if masked[k] == "\"" || masked[k] == "`" {
                        if let s = src.stringContent(startingAt: k + 1) {
                            out.append(s)
                            k += 1 + s.unicodeScalars.count + 1
                            continue
                        }
                    }
                    k += 1
                }
                i = k
            } else if j < masked.count, masked[j] == "\"" || masked[j] == "`" {
                if let s = src.stringContent(startingAt: j + 1) {
                    out.append(s)
                }
                i = j + 1
            } else if j < masked.count, isIdentScalar(masked[j]) {
                // 別名 import: `import alias "path"`
                var k = j
                while k < masked.count, isIdentScalar(masked[k]) { k += 1 }
                while k < masked.count, masked[k].properties.isWhitespace { k += 1 }
                if k < masked.count, masked[k] == "\"" {
                    if let s = src.stringContent(startingAt: k + 1) {
                        out.append(s)
                    }
                    i = k + 1
                }
            }
        } else {
            i += 1
        }
    }
    return out
}

/// Rust: use ツリーを展開して "a::b::c" 形式で返す（syn 版と同じ展開規則:
/// `use a::{b, c};` → "a::b", "a::c"、glob は "a::*"、`x as y` は x を採る）。
/// 戻り値: (uses, mods)
public func rsStructImports(_ text: String) -> ([String], [String]) {
    let src = maskRust(text)
    let masked = src.maskedText
    var uses: [String] = []
    var mods: [String] = []

    let useRe = Regexp("(?m)^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?use\\s+([^;]+);")
    let modRe = Regexp("(?m)^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?mod\\s+(\\w+)\\s*;")
    for m in useRe.allMatches(masked) {
        if let body = m[1] {
            expandUseTree(body, prefix: "", into: &uses)
        }
    }
    for m in modRe.allMatches(masked) {
        if let name = m[1] {
            mods.append(name)
        }
    }
    return (uses, mods)
}

/// use ツリーのテキスト展開。
func expandUseTree(_ body: String, prefix: String, into out: inout [String]) {
    let s = Array(body.unicodeScalars)
    var i = 0
    parseUseTree(s, &i, prefix, &out)
}

private func parseUseTree(_ s: [Unicode.Scalar], _ i: inout Int, _ prefix: String, _ out: inout [String]) {
    func skipWs() {
        while i < s.count, s[i].properties.isWhitespace { i += 1 }
    }
    func join(_ p: String, _ seg: String) -> String {
        p.isEmpty ? seg : "\(p)::\(seg)"
    }
    skipWs()
    guard i < s.count else { return }
    if s[i] == "{" {
        i += 1
        while i < s.count {
            skipWs()
            if i < s.count, s[i] == "}" {
                i += 1
                return
            }
            parseUseTree(s, &i, prefix, &out)
            skipWs()
            if i < s.count, s[i] == "," {
                i += 1
                continue
            }
            if i < s.count, s[i] == "}" {
                i += 1
                return
            }
            if i >= s.count { return }
        }
        return
    }
    if s[i] == "*" {
        i += 1
        out.append(join(prefix, "*"))
        return
    }
    // セグメント（ident / crate / self / super / r#ident）
    var name = ""
    if i + 1 < s.count, s[i] == "r", s[i + 1] == "#" {
        i += 2
    }
    while i < s.count, isIdentScalar(s[i]) {
        name.unicodeScalars.append(s[i])
        i += 1
    }
    if name.isEmpty {
        i += 1 // 不正なトークンはスキップ
        return
    }
    skipWs()
    if i + 1 < s.count, s[i] == ":", s[i + 1] == ":" {
        i += 2
        parseUseTree(s, &i, join(prefix, name), &out)
        return
    }
    // `x as y` → x を採る（syn の UseTree::Rename と同じ）
    if i + 2 < s.count, s[i] == "a", s[i + 1] == "s", !isIdentScalar(s[i + 2]) {
        i += 2
        skipWs()
        while i < s.count, isIdentScalar(s[i]) { i += 1 }
    }
    out.append(join(prefix, name))
}

/// Swift: `import Foo` / `import class Foo.Bar`（モジュールは external 扱い）。
public func swiftStructImports(_ text: String) -> [String] {
    let src = maskSwift(text)
    let re = Regexp("(?m)^\\s*(?:@\\w+(?:\\([^)]*\\))?\\s+)*import\\s+(?:(?:typealias|struct|class|enum|protocol|let|var|func)\\s+)?([\\w.]+)")
    var out: [String] = []
    for m in re.allMatches(src.maskedText) {
        if let name = m[1] {
            out.append(name)
        }
    }
    return out
}
