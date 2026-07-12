// 実行オーケストレーション（rust/crates/dirlens-core/src/run.rs の等価移植）。
//
// I/O（stdout/stderr への書き出し・HTML ファイルの書き込み）は呼び出し側（CLI）が行う。
// コアは文字列と終了コードを返すだけ。

public struct RunResult: Error {
    public var stdout = ""
    public var stderr = ""
    public var exitCode: Int32 = 0
    /// --html 時: (書き込み先ファイル名, 内容)。書き込みは呼び出し側が行う。
    public var htmlFile: (String, String)? = nil

    public init() {}
}

private func earlyExit(_ msg: String) -> RunResult {
    var r = RunResult()
    r.stderr = msg + "\n"
    r.exitCode = 1
    return r
}

/// パス解決と Cfg 構築（dirlens.py main() の引数検証部分）。
public func prepare(_ args: Args, _ fs: FsProvider, _ useColorHint: Bool) -> Result<Cfg, RunResult> {
    let useColor = useColorHint && !(args.noColor || args.markdown || args.json)

    guard let target = fs.resolve(args.path) else {
        var r = RunResult()
        r.stderr = "エラー: 現在のディレクトリへのアクセス権限がありません。\n"
            + "絶対パスを明示的に指定してください（例: dirlens /path/to/project）。\n"
        r.exitCode = 1
        return .failure(r)
    }

    guard let st = fs.stat(target, follow: true) else {
        return .failure(earlyExit("エラー: '\(args.path)' が見つかりません"))
    }
    if st.mode & 0o170000 != 0o040000 {
        return .failure(earlyExit("エラー: '\(args.path)' はディレクトリではありません"))
    }

    let rootLabel = fileName(target) ?? target

    switch Cfg.fromArgs(args, root: target, rootLabel: rootLabel, useColor: useColor) {
    case .success(let cfg):
        return .success(cfg)
    case .failure(let msg):
        return .failure(earlyExit("エラー: \(msg)"))
    }
}

/// ルート直下のディレクトリ一覧（並列プリフェッチ用・_prefetch_sizes の対象列挙）。
public func prefetchTargets(_ sess: Session, _ cfg: Cfg) -> [String] {
    guard let entries = sess.fs.scanDir(cfg.root) else { return [] }
    return entries.filter { $0.isDirNofollow }.map { $0.path }
}

/// 解析＋レンダリング本体。
public func execute(
    _ sess: Session, _ cfg: Cfg, _ git: GitProvider, _ clip: ClipboardProvider
) -> RunResult {
    let probe = EnvProbe(
        gitAvailable: git.available(),
        isWorkTree: git.isWorkTree(root: cfg.root),
        clipboard: clip.available(),
        python3: cfg.enhancedAnalysis && sess.ast.pythonAvailable(),
        nodeTypescript: cfg.enhancedAnalysis && sess.ast.jsAvailable()
    )

    // ── --check（能力レポート） ───────────────────────────────
    if cfg.check {
        let (stdout, exitCode) = renderCheck(cfg, probe, cfg.json)
        var r = RunResult()
        r.stdout = stdout
        r.exitCode = exitCode
        return r
    }

    let activePats: [String] = cfg.useGitignore ? sess.loadGitignore(cfg.root) : []

    // gitignore 2層: Tier1（git check-ignore）を試し、失敗時は Tier3（内蔵マッチャ）へ縮退
    if cfg.useGitignore {
        var tier = "builtin"
        if cfg.gitignorePreferGit {
            if let set = buildGitIgnoredSet(sess, git, cfg.root) {
                sess.gitIgnored = set
                tier = "git"
            }
        }
        cfg.gitignoreTier = tier
    }

    if cfg.showTests || cfg.showEntry || cfg.showConfig || cfg.showImports {
        let idx = buildProjectIndex(sess, cfg.root, cfg, activePats)
        cfg.untestedSet = idx.untested
        cfg.entrySet = idx.entrySet
        cfg.configSet = idx.configSet
        cfg.importsMap = idx.importsMap
        cfg.importedByMap = idx.importedByMap
        cfg.externalMap = idx.externalMap
        cfg.cycles = idx.cycles
    }
    if cfg.showGit {
        let (map, counts) = loadGitLog(git, cfg.root)
        cfg.gitMap = map
        cfg.gitChangeCounts = counts
    }

    // ── JSON ─────────────────────────────────────────────────
    if cfg.json {
        var r = RunResult()
        r.stdout = renderJson(sess, cfg, activePats, probe)
        return r
    }

    // ── HTML ─────────────────────────────────────────────────
    if let htmlPath = cfg.html {
        let content = generateHtml(sess, cfg, activePats)
        let size = UInt64(content.utf8.count)
        var r = RunResult()
        r.stdout = "✓ \(htmlPath) を生成しました (\(fmtSize(size, false)))\n"
        r.htmlFile = (htmlPath, content)
        return r
    }

    // ── テキスト出力 ─────────────────────────────────────────
    let text = renderText(sess, cfg, activePats, probe)
    var result = RunResult()
    result.stdout = text
    if cfg.copy {
        let ok = clip.copy(stripAnsi(result.stdout))
        let msg = ok
            ? c("✓ クリップボードにコピーしました", [Ansi.bold, Ansi.green], cfg.useColor)
            : c("✗ コピー失敗 (pbcopy / xclip / wl-copy が必要)", [Ansi.bold, Ansi.dim], cfg.useColor)
        result.stderr = msg + "\n"
    }
    return result
}

/// prepare + execute の一括呼び出し（プリフェッチ無しの単純経路用）。
public func run(
    _ args: Args, _ fs: FsProvider, _ git: GitProvider, _ clip: ClipboardProvider,
    _ useColorHint: Bool, ast: AstProvider = NoAst()
) -> RunResult {
    var args = args
    args.mergeAliases()
    let cfg: Cfg
    switch prepare(args, fs, useColorHint) {
    case .success(let c): cfg = c
    case .failure(let res): return res
    }
    let sess = Session(fs: fs, ast: ast)
    return execute(sess, cfg, git, clip)
}
