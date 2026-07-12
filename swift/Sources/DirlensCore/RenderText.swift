// テキスト（ツリー）レンダラ。rust/crates/dirlens-core/src/render_text.rs の等価移植。

private let PIPE = "│   "
private let FORK = "├── "
private let LAST = "└── "
private let BLANK = "    "

public struct TextStats {
    public var files: UInt64 = 0
    public var dirs: UInt64 = 0
    public var extensions = OrderedDict<UInt64>()
    public var tokens: Int64 = 0
    public var todoTotal: UInt64 = 0
    public var todoSamples: [(String, Int, String, String)] = []
}

private func fmtSizeI(_ n: Int64) -> String {
    if n < 0 {
        return "\(n) bytes"
    }
    return fmtSize(UInt64(n), false)
}

/// fmt_perm_info 相当。
private func permInfo(_ sess: Session, _ entry: FSEntry, _ cfg: Cfg) -> String {
    if !cfg.showPerms, !cfg.showUser, !cfg.showGroup {
        return ""
    }
    guard let st = sess.fs.stat(entry.path, follow: false) else { return "" }
    var parts: [String] = []
    if cfg.showPerms {
        parts.append(filemode(st.mode))
    }
    if cfg.showUser {
        parts.append(sess.fs.userName(st.uid) ?? String(st.uid))
    }
    if cfg.showGroup {
        parts.append(sess.fs.groupName(st.gid) ?? String(st.gid))
    }
    if parts.isEmpty {
        return ""
    }
    return c("[\(parts.joined(separator: " "))] ", [Ansi.dim], cfg.useColor)
}

private func esz(_ sess: Session, _ e: FSEntry) -> UInt64 {
    sess.fs.stat(e.path, follow: true)?.size ?? 0
}

private func emtime(_ sess: Session, _ e: FSEntry) -> Double {
    sess.fs.stat(e.path, follow: true)?.mtime ?? 0.0
}

private func renderNode(
    _ sess: Session, _ path: String, _ prefix: String, _ depth: Int64, _ cfg: Cfg,
    _ stats: inout TextStats, _ activePats: [String], _ seen: Set<String>?, _ out: inout String
) {
    if let md = cfg.maxDepth, depth >= md {
        return
    }

    var seenRef: Set<String>? = nil
    if cfg.followSyms {
        var s = seen ?? []
        let real = sess.fs.realPath(path)
        if s.contains(real) {
            out += "\(prefix)\(LAST)\(c("[循環リンク]", [Ansi.dim], cfg.useColor))\n"
            return
        }
        s.insert(real)
        seenRef = s
    }

    let curPats = extendPats(sess, activePats, path, cfg)
    guard var (dirs, files) = filterEntries(sess, path, cfg, curPats) else {
        out += "\(prefix)\(LAST)\(c("[アクセス拒否]", [Ansi.bold, Ansi.red], cfg.useColor))\n"
        return
    }

    if cfg.prune {
        dirs = dirs.filter { hasContent(sess, $0.path, depth + 1, cfg, curPats) }
    }

    sortEntries(sess, &dirs, &files, cfg)
    let combined: [FSEntry] = cfg.filesFirst ? files + dirs : dirs + files
    let (curDirSize, _) = sess.dirSize(path)

    let n = combined.count
    for (i, entry) in combined.enumerated() {
        let isLast = i == n - 1
        let branch = isLast ? LAST : FORK
        let cont = isLast ? BLANK : PIPE

        // 名前・リンク先は攻撃者制御になりうる（clone したリポジトリ等）。
        // 制御文字を無害化してエスケープ注入・行偽装を防ぐ。
        var symTarget = ""
        if entry.isSymlink {
            if let t = sess.fs.readLink(entry.path) {
                symTarget = " → \(sanitizeCtrl(t))"
            } else {
                symTarget = " →"
            }
        }

        let display: String
        if cfg.fullPath {
            display = sanitizeCtrl("./\(relpathSlash(entry.path, cfg.root))")
        } else {
            display = sanitizeCtrl(entry.name)
        }

        let permPrefix = permInfo(sess, entry, cfg)

        let isDirEntry = entry.isDirNofollow
            || (cfg.followSyms && entry.isSymlink && entry.isDirFollow)

        if isDirEntry {
            let (sz, szErr) = sess.dirSize(entry.path)
            let (nd, nf, denied) = countEntries(sess, entry.path, cfg, curPats)
            stats.dirs += 1

            let emoji = cfg.showEmoji ? "\(getEmoji(entry.name, isDir: true)) " : ""
            var parts = [fmtCount(nd, nf, denied), fmtSize(sz, szErr)]
            if cfg.showDate, let st = sess.fs.stat(entry.path, follow: false) {
                parts.append(fmtDate(sess.fs.now(), st.mtime))
            }
            let bar = (cfg.showBar && curDirSize != 0) ? " \(fmtBar(sz, curDirSize, 10))" : ""

            let name = c("\(emoji)\(display)\(symTarget)/", [Ansi.bold, Ansi.cyan], cfg.useColor)
            let meta = c("(\(parts.joined(separator: ", ")))\(bar)", [Ansi.dim], cfg.useColor)
            out += "\(prefix)\(branch)\(permPrefix)\(name) \(meta)\n"
            renderNode(sess, entry.path, prefix + cont, depth + 1, cfg, &stats, curPats, seenRef, &out)
        } else {
            let sz = esz(sess, entry)
            stats.files += 1
            let ext = splitext(entry.name).1.lowercased()
            let extKey = ext.isEmpty ? "(no ext)" : ext
            stats.extensions.update(extKey, default: 0) { $0 += 1 }

            let rel = relpathSlash(entry.path, cfg.root)
            let extras = cfg.hasExtras ? fileExtras(sess, entry, rel, cfg) : FileExtras()

            let entryMark = extras.isEntry ? (cfg.showEmoji ? "🎯 " : "* ") : ""
            let configMark = (extras.isConfig && entryMark.isEmpty) ? "⚙ " : ""

            // 注意: dirlens.py はここでファイル用の絵文字を計算するが、名前の表示には
            // 使っていない（entry_mark の 🎯 のみ）。ゴールデン一致のため同じ挙動にする。
            var parts = [fmtSize(sz, false)]
            if cfg.showDate {
                let mt = emtime(sess, entry)
                if mt != 0.0 {
                    parts.append(fmtDate(sess.fs.now(), mt))
                }
            }

            if cfg.showTokens, let tok = extras.tokens {
                parts.append(fmtTokens(tok))
                stats.tokens += tok
                if let lines = extras.lines {
                    parts.append("\(lines) lines")
                }
            }

            if cfg.showGit, let g = extras.git {
                parts.append(fmtGit(g))
            }

            if cfg.showTodo, !extras.todos.isEmpty {
                let nTodo = extras.todos.count
                parts.append("TODO×\(nTodo)")
                stats.todoTotal += UInt64(nTodo)
                for item in extras.todos.prefix(3) where stats.todoSamples.count < 20 {
                    stats.todoSamples.append((rel, item.0, item.1, item.2))
                }
            }

            if cfg.showTests, extras.noTest {
                parts.append("テスト無し")
            }

            if cfg.showConfig, extras.isConfig {
                parts.append("config")
            }

            if cfg.showOutline, let outline = extras.outline, !outline.isEmpty {
                if let ostr = fmtOutline(outline, 5) {
                    parts.append(ostr)
                }
            }

            if cfg.showImports {
                let impN = extras.imports.count
                let usedN = extras.importedBy.count
                if impN > 0 {
                    parts.append("imports×\(impN)")
                }
                if usedN > 0 {
                    parts.append("used-by×\(usedN)")
                }
            }

            let bar = (cfg.showBar && curDirSize != 0) ? " \(fmtBar(sz, curDirSize, 10))" : ""

            let name = c(
                "\(entryMark)\(configMark)\(display)\(symTarget)",
                [entry.isSymlink ? Ansi.magenta : Ansi.green],
                cfg.useColor
            )
            let meta = c("(\(parts.joined(separator: ", ")))\(bar)", [Ansi.dim], cfg.useColor)
            out += "\(prefix)\(branch)\(permPrefix)\(name) \(meta)\n"
        }
    }
}

/// 値の大きい順の安定ソート（Rust の sort_by(|a,b| b.1.cmp(&a.1)) 相当）。
func sortDescByCount<T>(_ items: [(T, Int)]) -> [(T, Int)] {
    var v = items
    stableSortByKey(&v, items.map { $0.1 }, true) { $0 < $1 }
    return v
}

func sortDescByCount64<T>(_ items: [(T, UInt64)]) -> [(T, UInt64)] {
    var v = items
    stableSortByKey(&v, items.map { $0.1 }, true) { $0 < $1 }
    return v
}

/// テキスト出力全体（markdown フェンス・ルート行・ツリー・サマリ）。
public func renderText(_ sess: Session, _ cfg: Cfg, _ activePats: [String], _ probe: EnvProbe) -> String {
    var out = ""
    let color = cfg.useColor

    if cfg.markdown {
        out += "```\n"
    }

    let (rootSz, rootSzErr) = sess.dirSize(cfg.root)
    let (rootNd, rootNf, rootDenied) = countEntries(sess, cfg.root, cfg, activePats)

    var rootParts = [fmtCount(rootNd, rootNf, rootDenied), fmtSize(rootSz, rootSzErr)]
    if cfg.showDate, let st = sess.fs.stat(cfg.root, follow: true) {
        rootParts.append(fmtDate(sess.fs.now(), st.mtime))
    }

    let rootEmoji = cfg.showEmoji ? "\(getEmoji(cfg.rootLabel, isDir: true)) " : ""
    out += "\(c("\(rootEmoji)\(sanitizeCtrl(cfg.rootLabel))/", [Ansi.bold, Ansi.blue], color)) "
        + "\(c("(\(rootParts.joined(separator: ", ")))", [Ansi.dim], color))\n"

    var stats = TextStats()
    renderNode(sess, cfg.root, "", 0, cfg, &stats, activePats, nil, &out)

    out += "\n"
    var summary = "  合計  \(stats.dirs) ディレクトリ"
    if !cfg.dirsOnly {
        summary += ",  \(stats.files) ファイル"
    }
    if cfg.useGitignore {
        summary += "  (.gitignore 適用済み)"
    }
    if let te = cfg.typeExt {
        summary += "  (フィルタ: \(te))"
    }
    if !cfg.excludes.isEmpty {
        summary += "  (除外: \(cfg.excludes.joined(separator: ", ")))"
    }
    if !cfg.includes.isEmpty {
        summary += "  (抽出: \(cfg.includes.joined(separator: ", ")))"
    }
    if let ms = cfg.minSize, ms != 0 {
        summary += "  (最小: \(fmtSizeI(ms)))"
    }
    if let ms = cfg.maxSize, ms != 0 {
        summary += "  (最大: \(fmtSizeI(ms)))"
    }
    if cfg.prune {
        summary += "  (剪定済み)"
    }
    if cfg.dirsOnly {
        summary += "  (ディレクトリのみ)"
    }
    out += "\(c(summary, [Ansi.dim], color))\n"

    if !cfg.dirsOnly, !stats.extensions.isEmpty {
        let exts = sortDescByCount64(stats.extensions.pairs) // 安定ソート: タイは出現順
        let line = exts.prefix(8)
            .map { "\(sanitizeCtrl($0.0)) ×\($0.1)" }
            .joined(separator: "  ")
        out += "\(c("  \(line)", [Ansi.dim], color))\n"
    }

    if cfg.showTokens {
        out += "\(c("  推定トークン数: \(fmtTokens(stats.tokens))", [Ansi.dim], color))\n"
    }

    if cfg.showTodo {
        if stats.todoTotal > 0 {
            out += "\(c("  TODO/FIXME等: \(stats.todoTotal)件", [Ansi.dim], color))\n"
            for (rel, ln, kind, snippet) in stats.todoSamples.prefix(8) {
                out += "\(c("    \(sanitizeCtrl(rel)):\(ln) [\(kind)] \(sanitizeCtrl(snippet))", [Ansi.dim], color))\n"
            }
            let shown = UInt64(min(stats.todoSamples.count, 8))
            if stats.todoTotal > shown {
                out += "\(c("    …他 \(stats.todoTotal - shown) 件", [Ansi.dim], color))\n"
            }
        } else {
            out += "\(c("  TODO/FIXME等: 0件", [Ansi.dim], color))\n"
        }
    }

    if cfg.showTests {
        out += "\(c("  テスト未整備: \(cfg.untestedSet.count) ファイル", [Ansi.dim], color))\n"
    }

    if cfg.showEntry {
        out += "\(c("  エントリーポイント候補: \(cfg.entrySet.count) 件検出", [Ansi.dim], color))\n"
    }

    if cfg.showConfig {
        out += "\(c("  設定ファイル: \(cfg.configSet.count) 件検出", [Ansi.dim], color))\n"
    }

    if cfg.showImports, !cfg.importedByMap.isEmpty {
        let items = sortDescByCount(cfg.importedByMap.pairs.map { ($0.0, $0.1.count) })
        out += "\(c("  依存度が高いファイル（多くのファイルから参照されている）:", [Ansi.dim], color))\n"
        for (relpath, n) in items.prefix(5) {
            out += "\(c("    \(sanitizeCtrl(relpath))  (used by \(n))", [Ansi.dim], color))\n"
        }
    }

    if cfg.showImports, !cfg.cycles.isEmpty {
        out += "\(c("  循環依存: \(cfg.cycles.count) 件検出", [Ansi.dim], color))\n"
        for cycle in cfg.cycles.prefix(5) {
            out += "\(c("    \(sanitizeCtrl(cycle.joined(separator: " → ")))", [Ansi.dim], color))\n"
        }
        if cfg.cycles.count > 5 {
            out += "\(c("    …他 \(cfg.cycles.count - 5) 件", [Ansi.dim], color))\n"
        }
    }

    if cfg.showGit, !cfg.gitChangeCounts.isEmpty {
        let items = sortDescByCount64(cfg.gitChangeCounts.pairs)
        let topHot = Array(items.prefix(5))
        if !topHot.isEmpty, topHot[0].1 > 1 {
            out += "\(c("  変更頻度が高いファイル（直近の履歴内）:", [Ansi.dim], color))\n"
            for (relpath, n) in topHot {
                out += "\(c("    \(sanitizeCtrl(relpath))  (\(n) 回変更)", [Ansi.dim], color))\n"
            }
        }
    }

    if cfg.showEntry, cfg.showImports, !cfg.entrySet.isEmpty || !cfg.importedByMap.isEmpty {
        let candidates = readingOrderCandidates(cfg, 3, 5)
        if !candidates.isEmpty {
            out += "\(c("  読み始めの候補（エントリーポイント→依存度の高い順）:", [Ansi.dim], color))\n"
            for (i, p) in candidates.enumerated() {
                out += "\(c("    \(i + 1). \(sanitizeCtrl(p))", [Ansi.dim], color))\n"
            }
        }
    }

    if cfg.showGit, cfg.gitMap.isEmpty {
        out += "\(c("  (gitリポジトリではないか、git未インストールのためコミット情報は取得できませんでした)", [Ansi.dim], color))\n"
    }

    // --agent の末尾に短い精度注記（spec 機能5）。互換モードでは出さない。
    if cfg.agent, !cfg.suppressNotes {
        out += "\(c(agentNote(cfg, probe), [Ansi.dim], color))\n"
    }

    if cfg.markdown {
        out += "```\n"
    }

    return out
}
