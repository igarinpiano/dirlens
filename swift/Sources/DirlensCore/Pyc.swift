// CPython 互換の細部ヘルパ（rust/crates/dirlens-core/src/pyc.rs の等価移植）。
//
// ゴールデンテスト（dirlens.py / Rust 版とのバイト一致）のため、丸め・文字列処理の
// 細かい挙動を CPython と揃える必要がある箇所をここに集約する。

import Foundation

/// Result<_, String> を使うためのエラー適合（Rust の Result<T, String> に対応）。
extension String: Error {}

/// CPython の `round(x)`（銀行家丸め・half-to-even）相当。
public func pyRound(_ x: Double) -> Int64 {
    let r = x.rounded(.toNearestOrEven)
    if r >= 9.223372036854775e18 { return Int64.max }
    if r <= -9.223372036854775e18 { return Int64.min }
    return Int64(r)
}

/// CPython の `int(x)`（ゼロ方向切り捨て）相当。
public func pyTrunc(_ x: Double) -> Int64 {
    let r = x.rounded(.towardZero)
    if r >= 9.223372036854775e18 { return Int64.max }
    if r <= -9.223372036854775e18 { return Int64.min }
    return Int64(r)
}

/// CPython の `f"{x:.2f}"` 等に相当（printf の正確丸めで一致する）。
public func fmtPrec(_ x: Double, _ prec: Int) -> String {
    return String(format: "%.\(prec)f", x)
}

/// Python の `s.rstrip('0').rstrip('.')` 相当。
public func rstripZeros(_ s: String) -> String {
    var t = Substring(s)
    while t.hasSuffix("0") { t = t.dropLast() }
    while t.hasSuffix(".") { t = t.dropLast() }
    return String(t)
}

@inline(__always)
private func isPySpace(_ c: Unicode.Scalar) -> Bool {
    // Python の str.strip() は Unicode White_Space に加えて \x1c-\x1f も空白扱い
    return c.properties.isWhitespace || (0x1C...0x1F).contains(c.value)
}

/// Python の str.strip()（Unicode 空白除去）相当。
public func pyStrip(_ s: some StringProtocol) -> String {
    let scalars = Array(s.unicodeScalars)
    var lo = 0
    var hi = scalars.count
    while lo < hi, isPySpace(scalars[lo]) { lo += 1 }
    while hi > lo, isPySpace(scalars[hi - 1]) { hi -= 1 }
    var out = String.UnicodeScalarView()
    out.append(contentsOf: scalars[lo..<hi])
    return String(out)
}

/// Python の `bytes.decode("utf-8", errors="ignore")` 相当。
/// 不正なバイトは（置換文字ではなく）黙って捨てる。
public func decodeUTF8Ignore(_ data: [UInt8]) -> String {
    var out = String.UnicodeScalarView()
    out.reserveCapacity(data.count)
    var iter = data.makeIterator()
    var parser = Unicode.UTF8.ForwardParser()
    loop: while true {
        switch parser.parseScalar(from: &iter) {
        case .valid(let seq):
            out.append(Unicode.UTF8.decode(seq))
        case .error:
            // 不正なシーケンス（最大部分列単位）を読み飛ばす
            continue
        case .emptyInput:
            break loop
        }
    }
    return String(out)
}

/// Python の str.casefold() 近似（rust 版 py_casefold と同一のテーブル）。
/// ソートキーとしての利用が目的（ファイル名の大半は ASCII/CJK でどちらも影響なし）。
public func pyCasefold(_ s: String) -> String {
    var out = String()
    out.reserveCapacity(s.count)
    for c in s.unicodeScalars {
        switch c {
        case "ß", "ẞ": out += "ss"
        case "ﬁ": out += "fi"
        case "ﬂ": out += "fl"
        case "ﬀ": out += "ff"
        case "ﬃ": out += "ffi"
        case "ﬄ": out += "ffl"
        case "µ": out += "μ"
        default:
            if c.isASCII {
                if c.value >= 65 && c.value <= 90 {
                    out.unicodeScalars.append(Unicode.Scalar(c.value + 32)!)
                } else {
                    out.unicodeScalars.append(c)
                }
            } else {
                out += String(c).lowercased()
            }
        }
    }
    return out
}

/// 文字列をコードポイント数で切り詰める（Python の s[:n] 相当）。
/// 戻り値: (切り詰め後, 実際に切り詰めたか)
public func truncateChars(_ s: String, _ n: Int) -> (String, Bool) {
    let scalars = Array(s.unicodeScalars)
    if scalars.count <= n {
        return (s, false)
    }
    var v = String.UnicodeScalarView()
    v.append(contentsOf: scalars[0..<n])
    return (String(v), true)
}

/// コードポイント数（Python の len(str) 相当）。
public func charLen(_ s: String) -> Int {
    return s.unicodeScalars.count
}

/// 改行スカラー '\n' で分割する（Python の str.split("\n") / Rust の split('\n') 相当）。
/// Swift の String.split(separator: "\n") は "\r\n" を 1 書記素として扱い
/// CRLF 行を分割しないため、必ずこちらを使うこと。
public func splitLines(_ s: String) -> [String] {
    var out: [String] = []
    var cur = String.UnicodeScalarView()
    for sc in s.unicodeScalars {
        if sc == "\n" {
            out.append(String(cur))
            cur = String.UnicodeScalarView()
        } else {
            cur.append(sc)
        }
    }
    out.append(String(cur))
    return out
}

/// 最初に現れるスカラー sep より前の部分（Python の s.split(sep)[0] 相当）。
public func beforeScalar(_ s: some StringProtocol, _ sep: Unicode.Scalar) -> String {
    var cur = String.UnicodeScalarView()
    for sc in s.unicodeScalars {
        if sc == sep { break }
        cur.append(sc)
    }
    return String(cur)
}

/// 先頭スカラーの判定（Python の startswith 相当。書記素結合の影響を受けない）。
public func firstScalarIs(_ s: some StringProtocol, _ c: Unicode.Scalar) -> Bool {
    return s.unicodeScalars.first == c
}

/// Python の文字列比較（コードポイント順・正規化なし）相当。
/// Swift の String `<` は正規化を挟むため使わない。
public func pyLess(_ a: String, _ b: String) -> Bool {
    var ai = a.unicodeScalars.makeIterator()
    var bi = b.unicodeScalars.makeIterator()
    while true {
        let x = ai.next()
        let y = bi.next()
        switch (x, y) {
        case (nil, nil): return false
        case (nil, _): return true
        case (_, nil): return false
        case (let x?, let y?):
            if x.value != y.value { return x.value < y.value }
        }
    }
}
