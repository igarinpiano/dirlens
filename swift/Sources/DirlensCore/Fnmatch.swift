// CPython の fnmatch モジュール互換のグロブマッチャ
// （rust/crates/dirlens-core/src/fnmatch.rs の等価移植）。
//
// 重要な互換ポイント:
// - `*` は `/` を含む任意の文字列にマッチする（gitignore の `*` とは異なる）。
// - `?` は任意の 1 文字。
// - `[seq]` は文字クラス（`[!seq]` で否定、先頭 `]` はリテラル、`a-z` は
//   コードポイント範囲）。閉じ `]` が無い場合は `[` をリテラル扱い。

private enum Tok {
    case star
    case any
    case lit(Unicode.Scalar)
    case cls(neg: Bool, ranges: [(Unicode.Scalar, Unicode.Scalar)])
}

private func parsePattern(_ pat: [Unicode.Scalar]) -> [Tok] {
    var toks: [Tok] = []
    let n = pat.count
    var i = 0
    while i < n {
        let c = pat[i]
        switch c {
        case "*":
            toks.append(.star)
            i += 1
        case "?":
            toks.append(.any)
            i += 1
        case "[":
            // CPython fnmatch.translate と同じ走査: '!' と先頭 ']' を特別扱いし、
            // 閉じ ']' が見つからなければ '[' をリテラルにする。
            var j = i + 1
            if j < n, pat[j] == "!" { j += 1 }
            if j < n, pat[j] == "]" { j += 1 }
            while j < n, pat[j] != "]" { j += 1 }
            if j >= n {
                toks.append(.lit("["))
                i += 1
            } else {
                var k = i + 1
                let neg = pat[k] == "!"
                if neg { k += 1 }
                let stuff = Array(pat[k..<j])
                var ranges: [(Unicode.Scalar, Unicode.Scalar)] = []
                let m = stuff.count
                var p = 0
                while p < m {
                    if p + 2 < m, stuff[p + 1] == "-" {
                        ranges.append((stuff[p], stuff[p + 2]))
                        p += 3
                    } else {
                        ranges.append((stuff[p], stuff[p]))
                        p += 1
                    }
                }
                toks.append(.cls(neg: neg, ranges: ranges))
                i = j + 1
            }
        default:
            toks.append(.lit(c))
            i += 1
        }
    }
    return toks
}

private func classMatch(_ c: Unicode.Scalar, _ neg: Bool, _ ranges: [(Unicode.Scalar, Unicode.Scalar)]) -> Bool {
    let hit = ranges.contains { lo, hi in lo.value <= c.value && c.value <= hi.value }
    return hit != neg
}

private func matchToks(_ toks: [Tok], _ name: [Unicode.Scalar]) -> Bool {
    // 古典的な「最後の * に戻る」バックトラッキング
    var ti = 0
    var ni = 0
    var star: (Int, Int)? = nil
    while ni < name.count {
        if ti < toks.count {
            switch toks[ti] {
            case .star:
                star = (ti, ni)
                ti += 1
                continue
            case .any:
                ti += 1
                ni += 1
                continue
            case .lit(let l):
                if l == name[ni] {
                    ti += 1
                    ni += 1
                    continue
                }
            case .cls(let neg, let ranges):
                if classMatch(name[ni], neg, ranges) {
                    ti += 1
                    ni += 1
                    continue
                }
            }
        }
        guard let (sti, sni) = star else { return false }
        ti = sti + 1
        ni = sni + 1
        star = (sti, sni + 1)
    }
    while ti < toks.count {
        if case .star = toks[ti] {
            ti += 1
        } else {
            return false
        }
    }
    return true
}

/// fnmatch.fnmatchcase 相当（大文字小文字をそのまま比較）。
public func fnmatchCase(_ name: String, _ pat: String) -> Bool {
    let toks = parsePattern(Array(pat.unicodeScalars))
    return matchToks(toks, Array(name.unicodeScalars))
}

/// fnmatch.fnmatch 相当（POSIX では normcase は恒等）。
public func fnmatch(_ name: String, _ pat: String) -> Bool {
    return fnmatchCase(name, pat)
}
