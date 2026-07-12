// git 連携（-H）。rust/crates/dirlens-core/src/analysis/gitlog.rs の等価移植。
// subprocess の実行は GitProvider（CLI 側）が担い、コアは stdout を解析する。

public let maxCommits = 2000

/// 戻り値: (fileMap: 最終コミット情報, changeCounts: 変更回数)
public func loadGitLog(_ git: GitProvider, _ root: String) -> ([String: GitInfo], OrderedDict<UInt64>) {
    guard let stdout = git.logOutput(root: root, maxCommits: maxCommits) else {
        return ([:], OrderedDict<UInt64>())
    }
    return parseGitLog(stdout)
}

public func parseGitLog(_ stdout: String) -> ([String: GitInfo], OrderedDict<UInt64>) {
    var fileMap: [String: GitInfo] = [:]
    var changeCounts = OrderedDict<UInt64>()
    var current: GitInfo? = nil
    for raw in splitLines(stdout) {
        var line = Substring(raw)
        while line.hasPrefix("\r") { line = line.dropFirst() }
        while line.hasSuffix("\r") { line = line.dropLast() }
        if line.hasPrefix("\u{1}") {
            var body = line.dropFirst()
            if body.hasSuffix("\u{3}") { body = body.dropLast() }
            let parts = body.split(separator: "\u{2}", maxSplits: 3, omittingEmptySubsequences: false)
            if parts.count == 4 {
                current = GitInfo(
                    hash: String(parts[0].unicodeScalars.prefix(7).map(Character.init)),
                    date: String(parts[1]),
                    author: String(parts[2]),
                    subject: String(parts[3])
                )
            } else {
                current = nil
            }
        } else if !pyStrip(line).isEmpty {
            if let cur = current {
                let fp = pyStrip(line).replacingOccurrences(of: "\\", with: "/")
                if fileMap[fp] == nil {
                    fileMap[fp] = cur
                }
                changeCounts.update(fp, default: 0) { $0 += 1 }
            }
        }
    }
    return (fileMap, changeCounts)
}
