// BPE トークナイザ（o200k_base）。tiktoken / tiktoken-rs の encode_ordinary と
// 同一のトークン数を返す純 Swift 実装（外部依存なし・語彙はリソース同梱）。
//
// 語彙リソースが見つからない実行環境では shared が nil になり、呼び出し側が
// 文字数ヒューリスティック（Tier2）へ縮退する。

import Foundation

/// [UInt8] のキー化を速くするための FNV-1a ハッシュラッパ。
private struct Bytes: Hashable {
    let b: [UInt8]
    let h: Int

    init(_ b: [UInt8]) {
        self.b = b
        var x: UInt64 = 0xcbf29ce484222325
        for c in b {
            x = (x ^ UInt64(c)) &* 0x100000001b3
        }
        self.h = Int(bitPattern: UInt(truncatingIfNeeded: x))
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(h)
    }

    static func == (l: Bytes, r: Bytes) -> Bool {
        l.h == r.h && l.b == r.b
    }
}

public final class O200KTokenizer: @unchecked Sendable {
    private let ranks: [Bytes: UInt32]
    private let splitter: NSRegularExpression

    /// tiktoken の o200k_base と同一の分割パターン。
    private static let pattern: String = [
        "[^\\r\\n\\p{L}\\p{N}]?[\\p{Lu}\\p{Lt}\\p{Lm}\\p{Lo}\\p{M}]*[\\p{Ll}\\p{Lm}\\p{Lo}\\p{M}]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        "[^\\r\\n\\p{L}\\p{N}]?[\\p{Lu}\\p{Lt}\\p{Lm}\\p{Lo}\\p{M}]+[\\p{Ll}\\p{Lm}\\p{Lo}\\p{M}]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        "\\p{N}{1,3}",
        " ?[^\\s\\p{L}\\p{N}]+[\\r\\n/]*",
        "\\s*[\\r\\n]+",
        "\\s+(?!\\S)",
        "\\s+",
    ].joined(separator: "|")

    public static let shared: O200KTokenizer? = {
        // 検証・比較用: DIRLENS_TOKENS=heuristic は呼び出し側（cfg.tokensBpe）で
        // 処理されるが、リソース欠落時の縮退確認用に env でも無効化できる
        if ProcessInfo.processInfo.environment["DIRLENS_BPE"] == "off" {
            return nil
        }
        guard let url = Bundle.module.url(forResource: "o200k_base", withExtension: "tiktoken"),
              let data = try? Data(contentsOf: url)
        else {
            return nil
        }
        return O200KTokenizer(vocabData: data)
    }()

    private init?(vocabData: Data) {
        var ranks: [Bytes: UInt32] = [:]
        ranks.reserveCapacity(200_000)
        // 形式: 1 行 = "base64トークン 半角スペース rank"
        var lineStart = vocabData.startIndex
        let nl = UInt8(ascii: "\n")
        let sp = UInt8(ascii: " ")
        var i = vocabData.startIndex
        while i <= vocabData.endIndex {
            if i == vocabData.endIndex || vocabData[i] == nl {
                if lineStart < i {
                    let line = vocabData[lineStart..<i]
                    if let spIdx = line.firstIndex(of: sp) {
                        let b64 = line[line.startIndex..<spIdx]
                        let rankBytes = line[line.index(after: spIdx)...]
                        if let tok = Data(base64Encoded: Data(b64)),
                           let rank = UInt32(String(decoding: rankBytes, as: UTF8.self)) {
                            ranks[Bytes([UInt8](tok))] = rank
                        }
                    }
                }
                if i == vocabData.endIndex { break }
                lineStart = vocabData.index(after: i)
            }
            i = vocabData.index(after: i)
        }
        if ranks.isEmpty { return nil }
        self.ranks = ranks
        guard let re = try? NSRegularExpression(pattern: Self.pattern) else { return nil }
        self.splitter = re
    }

    /// encode_ordinary(text).len() 相当（特殊トークンは扱わない）。
    public func countTokens(_ text: String) -> Int {
        let ns = text as NSString
        var count = 0
        splitter.enumerateMatches(in: text, options: [], range: NSRange(location: 0, length: ns.length)) { m, _, _ in
            guard let m else { return }
            let piece = ns.substring(with: m.range)
            count += self.countPiece([UInt8](piece.utf8))
        }
        return count
    }

    private func countPiece(_ piece: [UInt8]) -> Int {
        if piece.count == 1 { return 1 }
        if ranks[Bytes(piece)] != nil { return 1 }
        return bytePairMergeCount(piece)
    }

    private func rank(of slice: ArraySlice<UInt8>) -> UInt32 {
        return ranks[Bytes([UInt8](slice))] ?? UInt32.max
    }

    /// tiktoken の _byte_pair_merge と同一のアルゴリズム（マージ後の分割数を返す）。
    private func bytePairMergeCount(_ piece: [UInt8]) -> Int {
        // parts[i] = (開始位置, 次パーツと結合したペアの rank)
        var parts: [(Int, UInt32)] = []
        parts.reserveCapacity(piece.count + 1)
        var minRank: (UInt32, Int) = (UInt32.max, Int.max)
        for i in 0..<(piece.count - 1) {
            let r = rank(of: piece[i...(i + 1)])
            if r < minRank.0 {
                minRank = (r, i)
            }
            parts.append((i, r))
        }
        parts.append((piece.count - 1, UInt32.max))
        parts.append((piece.count, UInt32.max))

        func getRank(_ parts: [(Int, UInt32)], _ i: Int) -> UInt32 {
            if i + 3 < parts.count {
                return rank(of: piece[parts[i].0..<parts[i + 3].0])
            }
            return UInt32.max
        }

        while minRank.0 != UInt32.max {
            let i = minRank.1
            if i > 0 {
                parts[i - 1].1 = getRank(parts, i - 1)
            }
            parts[i].1 = getRank(parts, i)
            parts.remove(at: i + 1)

            minRank = (UInt32.max, Int.max)
            for j in 0..<(parts.count - 1) where parts[j].1 < minRank.0 {
                minRank = (parts[j].1, j)
            }
        }
        return parts.count - 1
    }
}
