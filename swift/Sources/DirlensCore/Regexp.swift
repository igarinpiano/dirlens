// NSRegularExpression の薄いラッパ（Rust 版の regex クレート利用箇所に対応）。
// パターンは全て組込み定数のためコンパイル失敗は起きない（fatalError で検知）。

import Foundation

public final class Regexp: @unchecked Sendable {
    private let re: NSRegularExpression

    public init(_ pattern: String, caseInsensitive: Bool = false) {
        var opts: NSRegularExpression.Options = []
        if caseInsensitive { opts.insert(.caseInsensitive) }
        guard let re = try? NSRegularExpression(pattern: pattern, options: opts) else {
            fatalError("invalid regex: \(pattern)")
        }
        self.re = re
    }

    /// 先頭からの最初のマッチのキャプチャ群（[0] は全体）。無ければ nil。
    public func firstMatch(_ s: String) -> [String?]? {
        let ns = s as NSString
        guard let m = re.firstMatch(in: s, options: [], range: NSRange(location: 0, length: ns.length)) else {
            return nil
        }
        return extract(m, ns)
    }

    /// 全マッチのキャプチャ群。
    public func allMatches(_ s: String) -> [[String?]] {
        let ns = s as NSString
        let ms = re.matches(in: s, options: [], range: NSRange(location: 0, length: ns.length))
        return ms.map { extract($0, ns) }
    }

    public func isMatch(_ s: String) -> Bool {
        let ns = s as NSString
        return re.firstMatch(in: s, options: [], range: NSRange(location: 0, length: ns.length)) != nil
    }

    /// 全マッチの (UTF-16 開始位置, キャプチャ群)。
    public func allMatchesWithLocation(_ s: String) -> [(Int, [String?])] {
        let ns = s as NSString
        let ms = re.matches(in: s, options: [], range: NSRange(location: 0, length: ns.length))
        return ms.map { ($0.range.location, extract($0, ns)) }
    }

    private func extract(_ m: NSTextCheckingResult, _ ns: NSString) -> [String?] {
        var groups: [String?] = []
        for i in 0..<m.numberOfRanges {
            let r = m.range(at: i)
            if r.location == NSNotFound {
                groups.append(nil)
            } else {
                groups.append(ns.substring(with: r))
            }
        }
        return groups
    }
}
