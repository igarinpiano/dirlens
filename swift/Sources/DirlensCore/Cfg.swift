// 設定クラス（rust/crates/dirlens-core/src/cfg.rs の等価移植）。
// 全設定＋プロジェクト解析結果を集約する。

public final class Cfg {
    public var root: String = ""
    /// ルート行に出すラベル（target の basename、無ければ target 全体）
    public var rootLabel: String = ""
    public var maxDepth: Int64? = nil
    public var showAll = false
    public var bySize = false
    public var sortMtime = false
    public var sortCtime = false
    public var showDate = false
    public var useGitignore = false
    public var showBar = false
    public var minSize: Int64? = nil
    public var maxSize: Int64? = nil
    public var excludes: [String] = []
    public var includes: [String] = []
    public var showEmoji = false
    public var typeExt: String? = nil
    public var showPerms = false
    public var showUser = false
    public var showGroup = false
    public var dirsOnly = false
    public var followSyms = false
    public var fullPath = false
    public var prune = false
    public var reverse = false
    public var filesFirst = false

    // AI/エージェント向け解析フラグ
    public var showTokens = false
    public var showGit = false
    public var showTodo = false
    public var showTests = false
    public var showEntry = false
    public var showOutline = false
    public var showImports = false
    public var showConfig = false
    public var publicOnly = false
    public var hasExtras = false

    /// Tier1（git check-ignore）を試すか。CLI が環境変数
    /// （DIRLENS_GITIGNORE=builtin / DIRLENS_COMPAT=python）に応じて false にする。
    public var gitignorePreferGit = true
    /// 実際に使われた gitignore 層（"git" / "builtin"）。capabilities 出力用。
    public var gitignoreTier: String? = nil
    /// 構造走査（コード限定ビュー）＋外部 AST ツール＋マニフェスト読込による
    /// 解析改善を使うか。false なら正規表現層のみ
    /// （DIRLENS_COMPAT=python / DIRLENS_AST=off）。
    public var enhancedAnalysis = true
    /// トークン計数に BPE（Tier1）を使うか。false なら文字数ヒューリスティック
    /// （DIRLENS_TOKENS=heuristic / DIRLENS_COMPAT=python、または語彙リソース欠落）。
    public var tokensBpe = true
    /// 精度注記・schema_version・capabilities を出さない
    /// （DIRLENS_COMPAT=python: Python 版とのバイト一致検証用）。
    public var suppressNotes = false
    /// --check（能力レポートモード）
    public var check = false

    // 出力モード
    public var useColor = false
    public var markdown = false
    public var json = false
    public var html: String? = nil
    public var copy = false
    public var agent = false

    // main() 相当で必要に応じて埋める解析結果
    public var gitMap: [String: GitInfo] = [:]
    public var gitChangeCounts = OrderedDict<UInt64>()
    public var untestedSet: Set<String> = []
    public var entrySet: [String] = [] // ソート済み（BTreeSet 相当）
    public var configSet: Set<String> = []
    public var importsMap: [String: [String]] = [:]
    public var importedByMap = OrderedDict<[String]>()
    public var externalMap: [String: [String]] = [:]
    public var cycles: [[String]] = []

    public init() {}

    /// Args（エイリアス統合済み）から Cfg を構築する。
    /// min/max サイズの解析エラーはメッセージを返す。
    public static func fromArgs(
        _ args: Args, root: String, rootLabel: String, useColor: Bool
    ) -> Result<Cfg, String> {
        let cfg = Cfg()
        if let s = args.minSize {
            switch parseSize(s) {
            case .success(let v): cfg.minSize = v
            case .failure(let msg): return .failure(msg)
            }
        }
        if let s = args.maxSize {
            switch parseSize(s) {
            case .success(let v): cfg.maxSize = v
            case .failure(let msg): return .failure(msg)
            }
        }
        if let t = args.typeExt {
            var trimmed = Substring(t)
            while trimmed.hasPrefix(".") { trimmed = trimmed.dropFirst() }
            cfg.typeExt = ".\(trimmed)".lowercased()
        }
        cfg.root = root
        cfg.rootLabel = rootLabel
        cfg.maxDepth = args.depth
        cfg.showAll = args.all
        cfg.bySize = args.sortSize
        cfg.sortMtime = args.sortMtime
        cfg.sortCtime = args.sortCtime
        cfg.showDate = args.date
        cfg.useGitignore = args.gitignore
        cfg.showBar = args.bar
        cfg.excludes = args.exclude
        cfg.includes = args.include
        cfg.showEmoji = args.emoji
        cfg.showPerms = args.perms
        cfg.showUser = args.user
        cfg.showGroup = args.showGroup
        cfg.dirsOnly = args.dirsOnly
        cfg.followSyms = args.follow
        cfg.fullPath = args.fullPath
        cfg.prune = args.prune
        cfg.reverse = args.reverse
        cfg.filesFirst = args.filesfirst
        cfg.showTokens = args.tokens
        cfg.showGit = args.git
        cfg.showTodo = args.todo
        cfg.showTests = args.tests
        cfg.showEntry = args.entry
        cfg.showOutline = args.outline
        cfg.showImports = args.imports
        cfg.showConfig = args.config
        cfg.publicOnly = args.api
        cfg.hasExtras = args.tokens || args.git || args.todo || args.tests
            || args.entry || args.outline || args.imports || args.config
        cfg.check = args.check
        cfg.useColor = useColor
        cfg.markdown = args.markdown
        cfg.json = args.json
        cfg.html = args.html
        cfg.copy = args.copy
        cfg.agent = args.agent
        return .success(cfg)
    }
}
