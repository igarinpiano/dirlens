// フォーマット関数群（rust/crates/dirlens-core/src/fmt.rs の等価移植）。

import Foundation

/// 端末出力用サニタイズ: 制御文字（Cc カテゴリ = C0/DEL/C1）を '?' に置換する。
/// ファイル名・シンボリックリンク先・ファイル内容・git メタデータは攻撃者が
/// 制御しうるため、生の ESC/改行を通すとエスケープシーケンス注入や
/// ツリー行の偽装（--agent 出力の改竄）に使える。テキスト出力の直前で通すこと。
public func sanitizeCtrl(_ s: String) -> String {
    let scalars = s.unicodeScalars
    guard scalars.contains(where: { $0.properties.generalCategory == .control }) else {
        return s
    }
    var out = String.UnicodeScalarView()
    out.reserveCapacity(scalars.count)
    for c in scalars {
        out.append(c.properties.generalCategory == .control ? "?" : c)
    }
    return String(out)
}

/// os.path.splitext 相当（名前部分のみを対象）。ext は '.' を含む。
/// ".env" のような先頭ドットは拡張子とみなさない（CPython 互換）。
public func splitext(_ name: String) -> (String, String) {
    let scalars = Array(name.unicodeScalars)
    var sep = -1
    for (i, c) in scalars.enumerated() where c == "." {
        sep = i
    }
    // 先頭からセパレータ位置まで全部 '.' なら拡張子なし（".env" 等）
    if sep > 0, scalars[0..<sep].contains(where: { $0 != "." }) {
        var head = String.UnicodeScalarView()
        head.append(contentsOf: scalars[0..<sep])
        var tail = String.UnicodeScalarView()
        tail.append(contentsOf: scalars[sep...])
        return (String(head), String(tail))
    }
    return (name, "")
}

public func fmtSize(_ n: UInt64, _ partial: Bool) -> String {
    let sfx = partial ? "+" : ""
    if n == 0 {
        return "0\(sfx) bytes"
    }
    let units: [(String, UInt64)] = [
        ("TB", 1 << 40), ("GB", 1 << 30), ("MB", 1 << 20), ("KB", 1 << 10),
    ]
    for (unit, f) in units where n >= f {
        let num = rstripZeros(fmtPrec(Double(n) / Double(f), 2))
        return "\(num)\(sfx) \(unit)"
    }
    let word = (n == 1 && !partial) ? "byte" : "bytes"
    return "\(n)\(sfx) \(word)"
}

public func fmtCount(_ nd: Int, _ nf: Int, _ denied: Bool) -> String {
    let sfx = denied ? "+" : ""
    let dWord = (nd == 1 && !denied) ? "dir" : "dirs"
    let fWord = (nf == 1 && !denied) ? "file" : "files"
    return "\(nd)\(sfx) \(dWord), \(nf)\(sfx) \(fWord)"
}

/// now / mtime は epoch 秒。
public func fmtDate(_ now: Double, _ mtime: Double) -> String {
    let sec = pyTrunc(now - mtime)
    if sec < 60 { return "今" }
    if sec < 3600 { return "\(sec / 60)分前" }
    if sec < 86400 { return "\(sec / 3600)時間前" }
    let d = sec / 86400
    if d == 1 { return "昨日" }
    if d < 7 { return "\(d)日前" }
    if d < 30 { return "\(d / 7)週間前" }
    if d < 365 { return "\(d / 30)ヶ月前" }
    return "\(d / 365)年前"
}

public func fmtBar(_ part: UInt64, _ total: UInt64, _ width: Int64) -> String {
    let pct: Int64
    if total != 0 {
        pct = min(100, pyTrunc(Double(part) * 100.0 / Double(total)))
    } else {
        pct = 0
    }
    let filled = pyRound(Double(pct) * Double(width) / 100.0)
    let empty = width - filled
    let bar = String(repeating: "█", count: Int(max(filled, 0)))
        + String(repeating: "░", count: Int(max(empty, 0)))
    return "[\(bar)]" + String(format: "%4d", pct) + "%"
}

/// parse_size: "50M" / "1G" / "500K" / 素の整数。エラー時は Python と同じ文言を返す。
public func parseSize(_ raw: String) -> Result<Int64, String> {
    let s = pyStrip(raw)
    let upper = s.uppercased()
    let suffixes: [(String, Int64)] = [
        ("TB", 1 << 40), ("GB", 1 << 30), ("MB", 1 << 20), ("KB", 1 << 10),
        ("T", 1 << 40), ("G", 1 << 30), ("M", 1 << 20), ("K", 1 << 10),
    ]
    for (sfx, mult) in suffixes where upper.hasSuffix(sfx) {
        // 接尾辞は文字数ベースで取り除く（Python の s[:-len(sfx)] 相当）。
        // uppercased() で長さが変わる文字（例: 'ﬆ'→"ST"）があるため、
        // 常に元の文字列 s の末尾から接尾辞ぶんのスカラーを落とす。
        let scalars = Array(s.unicodeScalars)
        let keep = max(0, scalars.count - sfx.count)
        var head = String.UnicodeScalarView()
        head.append(contentsOf: scalars[0..<keep])
        let headStr = String(head).trimmingCharacters(in: .whitespaces)
        if let v = Double(headStr) {
            return .success(pyTrunc(v * Double(mult)))
        }
        break // Python 同様、int(s) の解釈へフォールバック
    }
    if let v = Int64(s) {
        return .success(v)
    }
    return .failure("無効なサイズ: '\(s)'（例: 50M, 1G, 500K）")
}

public func fmtTokens(_ n: Int64) -> String {
    if n >= 1000 {
        let s = rstripZeros(fmtPrec(Double(n) / 1000.0, 1))
        return "~\(s)K tok"
    }
    return "~\(n) tok"
}

public struct GitInfo {
    public var hash: String
    public var date: String
    public var author: String
    public var subject: String

    public init(hash: String, date: String, author: String, subject: String) {
        self.hash = hash
        self.date = date
        self.author = author
        self.subject = subject
    }
}

public func fmtGit(_ g: GitInfo) -> String {
    var subj = pyStrip(g.subject)
    if charLen(subj) > 30 {
        let (head, _) = truncateChars(subj, 30)
        subj = head + "…"
    }
    // コミット件名・日付は攻撃者制御（clone したリポジトリ由来）
    return "\"\(sanitizeCtrl(subj))\" (\(sanitizeCtrl(g.date)))"
}

/// アウトライン 1 項目: (kind, name, is_public)
public struct OutlineItem: Equatable {
    public var kind: String
    public var name: String
    public var isPublic: Bool

    public init(_ kind: String, _ name: String, _ isPublic: Bool) {
        self.kind = kind
        self.name = name
        self.isPublic = isPublic
    }
}

public func fmtOutline(_ outline: [OutlineItem], _ limit: Int) -> String? {
    if outline.isEmpty { return nil }
    let items = outline.map { "\($0.kind) \($0.name)" }
    let shown = items.prefix(limit)
    var s = shown.joined(separator: ", ")
    if items.count > limit {
        s += ", +\(items.count - limit)"
    }
    return s
}

/// CPython の stat.filemode() 相当。
public func filemode(_ mode: UInt32) -> String {
    var out = ""
    let ifmt = mode & 0o170000
    switch ifmt {
    case 0o120000: out.append("l")
    case 0o140000: out.append("s")
    case 0o100000: out.append("-")
    case 0o060000: out.append("b")
    case 0o040000: out.append("d")
    case 0o020000: out.append("c")
    case 0o010000: out.append("p")
    default: out.append("?")
    }
    let triples: [(UInt32, UInt32, UInt32, UInt32, Character, Character)] = [
        (0o400, 0o200, 0o100, 0o4000, "s", "S"), // user + setuid
        (0o040, 0o020, 0o010, 0o2000, "s", "S"), // group + setgid
        (0o004, 0o002, 0o001, 0o1000, "t", "T"), // other + sticky
    ]
    for (r, w, x, special, both, onlySpecial) in triples {
        out.append(mode & r != 0 ? "r" : "-")
        out.append(mode & w != 0 ? "w" : "-")
        let hasX = mode & x != 0
        let hasS = mode & special != 0
        switch (hasX, hasS) {
        case (true, true): out.append(both)
        case (false, true): out.append(onlySpecial)
        case (true, false): out.append("x")
        case (false, false): out.append("-")
        }
    }
    return out
}
