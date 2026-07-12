// 共通フィルタリング＋ソート（rust/crates/dirlens-core/src/filter.rs の等価移植）。
// 表示可否の唯一のゲートキーパ。

/// _filter 相当。nil はアクセス拒否のシグナル。
public func filterEntries(
    _ sess: Session, _ path: String, _ cfg: Cfg, _ activePats: [String]
) -> ([FSEntry], [FSEntry])? {
    guard let raw = sess.fs.scanDir(path) else { return nil }

    var entries = raw.filter { cfg.showAll || !firstScalarIs($0.name, ".") }

    // Tier1（事前計算済み集合）が有効なら activePats が空でもフィルタする
    let engineActive = (cfg.useGitignore && sess.gitIgnored != nil) || !activePats.isEmpty
    if engineActive {
        entries = entries.filter { !ignoredByEngine(sess, cfg, $0, activePats) }
    }

    var dirs = entries.filter { $0.isDirNofollow }

    if cfg.followSyms {
        let symDirs = entries.filter { $0.isSymlink && !$0.isDirNofollow && $0.isDirFollow }
        dirs.append(contentsOf: symDirs)
    }

    var files: [FSEntry]
    if cfg.dirsOnly {
        files = []
    } else {
        files = entries.filter {
            !$0.isDirNofollow && !(cfg.followSyms && $0.isSymlink && $0.isDirFollow)
        }
    }

    if !cfg.excludes.isEmpty {
        dirs = dirs.filter { d in !cfg.excludes.contains { fnmatch(d.name, $0) } }
        files = files.filter { f in !cfg.excludes.contains { fnmatch(f.name, $0) } }
    }

    if !cfg.includes.isEmpty {
        files = files.filter { f in cfg.includes.contains { fnmatch(f.name, $0) } }
    }

    if let te = cfg.typeExt {
        files = files.filter { splitext($0.name).1.lowercased() == te }
    }

    if cfg.minSize != nil || cfg.maxSize != nil {
        files = files.filter { f in
            guard let st = sess.fs.stat(f.path, follow: true) else { return true }
            let sz = Int64(clamping: st.size)
            if let min = cfg.minSize, sz < min { return false }
            if let max = cfg.maxSize, sz > max { return false }
            return true
        }
    }

    return (dirs, files)
}

/// gitignore 判定。Tier1（git check-ignore の事前計算集合）があればそれを、
/// なければ Tier3（内蔵マッチャ）を使う。
private func ignoredByEngine(_ sess: Session, _ cfg: Cfg, _ e: FSEntry, _ activePats: [String]) -> Bool {
    if let gitSet = sess.gitIgnored {
        return gitSet.contains(relpathSlash(e.path, cfg.root))
    }
    return isIgnored(e.name, relpath(e.path, cfg.root), e.isDirNofollow, activePats)
}

/// count_entries 相当。(dirs, files, denied)
public func countEntries(
    _ sess: Session, _ path: String, _ cfg: Cfg, _ activePats: [String]
) -> (Int, Int, Bool) {
    let pats = extendPats(sess, activePats, path, cfg)
    guard let (dirs, files) = filterEntries(sess, path, cfg, pats) else {
        return (0, 0, true)
    }
    return (dirs.count, files.count, false)
}

/// _has_content 相当（--prune 用）。
public func hasContent(
    _ sess: Session, _ path: String, _ depth: Int64, _ cfg: Cfg, _ activePats: [String]
) -> Bool {
    if let md = cfg.maxDepth, depth >= md {
        return false
    }
    let pats = extendPats(sess, activePats, path, cfg)
    guard let (dirs, files) = filterEntries(sess, path, cfg, pats) else {
        return false
    }
    if !files.isEmpty { return true }
    for d in dirs {
        if hasContent(sess, d.path, depth + 1, cfg, pats) {
            return true
        }
    }
    return false
}

/// Python の list.sort(key=..., reverse=...) 相当の安定ソート。
/// reverse=True でも同キーの要素は元の順序を保つ（CPython と同じ）。
/// Swift の sort は安定性が保証されないため、元の添字をタイブレークに使う。
func stableSortByKey<T, K>(_ v: inout [T], _ keys: [K], _ reverse: Bool, _ less: (K, K) -> Bool) {
    precondition(v.count == keys.count)
    var idx = Array(0..<v.count)
    idx.sort { a, b in
        if less(keys[a], keys[b]) { return !reverse }
        if less(keys[b], keys[a]) { return reverse }
        return a < b // 安定
    }
    v = idx.map { v[$0] }
}

private func statF64(_ sess: Session, _ e: FSEntry, _ pick: (StatInfo) -> Double) -> Double {
    guard let st = sess.fs.stat(e.path, follow: true) else { return 0.0 }
    return pick(st)
}

private func statSize(_ sess: Session, _ e: FSEntry) -> UInt64 {
    return sess.fs.stat(e.path, follow: true)?.size ?? 0
}

/// _sort_entries 相当。
public func sortEntries(_ sess: Session, _ dirs: inout [FSEntry], _ files: inout [FSEntry], _ cfg: Cfg) {
    let rev = cfg.reverse
    if cfg.sortMtime {
        let dk = dirs.map { statF64(sess, $0) { $0.mtime } }
        stableSortByKey(&dirs, dk, !rev) { $0 < $1 }
        let fk = files.map { statF64(sess, $0) { $0.mtime } }
        stableSortByKey(&files, fk, !rev) { $0 < $1 }
    } else if cfg.sortCtime {
        let dk = dirs.map { statF64(sess, $0) { $0.ctime } }
        stableSortByKey(&dirs, dk, !rev) { $0 < $1 }
        let fk = files.map { statF64(sess, $0) { $0.ctime } }
        stableSortByKey(&files, fk, !rev) { $0 < $1 }
    } else if cfg.bySize {
        let dk = dirs.map { sess.dirSize($0.path).0 }
        stableSortByKey(&dirs, dk, !rev) { $0 < $1 }
        let fk = files.map { statSize(sess, $0) }
        stableSortByKey(&files, fk, !rev) { $0 < $1 }
    } else {
        let dk = dirs.map { pyCasefold($0.name) }
        stableSortByKey(&dirs, dk, rev, pyLess)
        let fk = files.map { pyCasefold($0.name) }
        stableSortByKey(&files, fk, rev, pyLess)
    }
}
