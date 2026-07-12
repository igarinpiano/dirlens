// .gitignore 内蔵マッチャ（Tier3）と Tier1 の事前計算
// （rust/crates/dirlens-core/src/gitignore.rs の等価移植）。
//
// Tier1（git check-ignore による厳密判定）は session.gitIgnored に事前計算した
// 無視集合が入っている場合に使われる（Run.swift で選択）。

/// パターンを順番に評価し、最後にマッチしたルールが勝つ（`!` 否定対応）。
public func isIgnored(_ name: String, _ relPath: String, _ isDir: Bool, _ patterns: [String]) -> Bool {
    let rel = relPath
    var result = false
    for pat in patterns {
        let negated = pat.hasPrefix("!")
        var p = Substring(pat)
        while p.hasPrefix("!") { p = p.dropFirst() }
        let dirOnly = p.hasSuffix("/")
        while p.hasSuffix("/") { p = p.dropLast() }
        if dirOnly, !isDir { continue }
        let matched: Bool
        if p.hasPrefix("/") {
            var anchored = p.dropFirst()
            while anchored.hasPrefix("/") { anchored = anchored.dropFirst() }
            matched = fnmatch(rel, String(anchored))
        } else {
            let ps = String(p)
            matched = fnmatch(name, ps) || fnmatch(rel, ps) || fnmatch(rel, "*/\(ps)")
        }
        if matched {
            result = !negated
        }
    }
    return result
}

/// Tier1: git check-ignore による無視集合の事前計算。
///
/// ルートから BFS でレベルごとに全エントリを `git check-ignore --stdin -z` へ
/// 一括投入し、無視された rel パス（"/" 区切り）の集合を作る。無視された
/// ディレクトリには降りない。git が使えない・非 work tree・途中で失敗した場合は
/// nil（Tier3 へ縮退）。
public func buildGitIgnoredSet(_ sess: Session, _ git: GitProvider, _ root: String) -> Set<String>? {
    var ignored: Set<String> = []
    var levelDirs: [String] = [root]
    while !levelDirs.isEmpty {
        var rels: [String] = []
        var children: [FSEntry] = []
        for d in levelDirs {
            if let entries = sess.fs.scanDir(d) {
                for e in entries {
                    rels.append(relpathSlash(e.path, root))
                    children.append(e)
                }
            }
        }
        if rels.isEmpty { break }
        guard let resp = git.checkIgnore(root: root, relPaths: rels) else { return nil }
        ignored.formUnion(resp)
        levelDirs = children
            .filter { $0.isDirNofollow && !ignored.contains(relpathSlash($0.path, root)) }
            .map { $0.path }
    }
    return ignored
}

/// _extend_pats 相当: 下位ディレクトリの .gitignore をルート相対に書き換えて追加する。
public func extendPats(_ sess: Session, _ active: [String], _ path: String, _ cfg: Cfg) -> [String] {
    if !cfg.useGitignore { return active }
    if path == cfg.root { return active }
    let local = sess.loadGitignore(path)
    if local.isEmpty { return active }
    let relDir = relpathSlash(path, cfg.root)
    var out = active
    for pat in local {
        let neg = pat.hasPrefix("!")
        var p = Substring(pat)
        while p.hasPrefix("!") { p = p.dropFirst() }
        if p.hasPrefix("/") {
            out.append("\(neg ? "!" : "")/\(relDir)\(p)")
        } else {
            out.append(pat)
        }
    }
    return out
}
