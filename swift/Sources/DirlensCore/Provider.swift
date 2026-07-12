// I/O 抽象プロトコル（rust/crates/dirlens-core/src/provider.rs の等価移植）。
//
// コアは Foundation の FS / Process を直接呼ばない。CLI 側が std 実装を提供する。
// この分離はテスト容易性と、将来の別ホスト（wasm 相当）対応のために維持する。

/// os.stat_result 相当（コアが必要とするフィールドのみ）。
/// mtime/ctime は CPython と同じ計算（sec + 1e-9 * nsec）の Double。
public struct StatInfo {
    public var size: UInt64 = 0
    public var mtime: Double = 0
    public var ctime: Double = 0
    public var mode: UInt32 = 0
    public var uid: UInt32 = 0
    public var gid: UInt32 = 0

    public init() {}

    public init(size: UInt64, mtime: Double, ctime: Double, mode: UInt32, uid: UInt32, gid: UInt32) {
        self.size = size
        self.mtime = mtime
        self.ctime = ctime
        self.mode = mode
        self.uid = uid
        self.gid = gid
    }
}

/// os.DirEntry 相当。型フラグは走査時に確定させる（d_type 相当）。
public struct FSEntry {
    public var name: String
    public var path: String
    public var isDirNofollow: Bool
    public var isFileNofollow: Bool
    public var isSymlink: Bool
    /// symlink の場合のみ follow した結果（それ以外は isDirNofollow と同じ）
    public var isDirFollow: Bool

    public init(name: String, path: String, isDirNofollow: Bool, isFileNofollow: Bool,
                isSymlink: Bool, isDirFollow: Bool) {
        self.name = name
        self.path = path
        self.isDirNofollow = isDirNofollow
        self.isFileNofollow = isFileNofollow
        self.isSymlink = isSymlink
        self.isDirFollow = isDirFollow
    }
}

public protocol FsProvider {
    /// os.scandir 相当。列挙順は OS の返す順のまま（ソートしない）。
    /// 権限拒否等は nil を返す（Python の OSError catch に対応）。
    func scanDir(_ path: String) -> [FSEntry]?

    /// os.stat / os.lstat 相当。失敗時 nil（呼び出し側でフォールバック）。
    func stat(_ path: String, follow: Bool) -> StatInfo?

    /// ファイル先頭 limit バイトを読む（open+read 相当）。失敗時 nil。
    func readPrefix(_ path: String, limit: Int) -> [UInt8]?

    /// os.readlink 相当。失敗時 nil。
    func readLink(_ path: String) -> String?

    /// os.path.realpath 相当（失敗しても入力を返す）。
    func realPath(_ path: String) -> String

    /// Path.resolve() 相当（存在しなくても正規化した絶対パスを返す）。
    func resolve(_ path: String) -> String?

    /// 現在時刻（epoch 秒）。
    func now() -> Double

    /// pwd.getpwuid / grp.getgrgid 相当。未対応プラットフォームでは nil。
    func userName(_ uid: UInt32) -> String?
    func groupName(_ gid: UInt32) -> String?
}

public protocol GitProvider {
    /// `git -C root log -n max --name-only --date=relative --pretty=...` の
    /// stdout を返す。git が無い / リポジトリでない / タイムアウト時は nil。
    func logOutput(root: String, maxCommits: Int) -> String?

    /// `git -C root check-ignore --stdin -z` に relPaths を投入し、
    /// 無視されたパスの集合を返す。git 不在・非 work tree なら nil（Tier3 へ縮退）。
    func checkIgnore(root: String, relPaths: [String]) -> [String]?

    /// git バイナリが使えるか（--check 用）。
    func available() -> Bool

    /// root が git work tree 内か（--check 用）。
    func isWorkTree(root: String) -> Bool
}

public protocol ClipboardProvider {
    /// クリップボードにコピーする。成功なら true。
    func copy(_ text: String) -> Bool

    /// クリップボードツールが存在するか（--check 用）。
    func available() -> Bool
}

/// 外部 AST ツール層（Tier1）。Swift 版固有の設計:
/// 外部依存で精度が上がる解析は「実行時にツールを探して使い、
/// 使えなければ内蔵の構造走査（Tier1.5）→ 正規表現（Tier2）へフォールバック」する。
public protocol AstProvider {
    /// Python の AST 解析（python3 の stdlib ast）。
    /// nil = ツールが無い/起動失敗/パース失敗 → 内蔵層へ縮退。
    func pythonOutline(_ text: String, cacheKey: String) -> [OutlineItem]?
    func pythonImports(_ text: String, cacheKey: String) -> [(String, UInt32, [String]?)]?

    /// JS/TS の AST 解析（node + プロジェクト内 typescript）。
    func jsOutline(_ text: String, ext: String, cacheKey: String) -> [OutlineItem]?
    func jsImports(_ text: String, ext: String, cacheKey: String) -> [String]?

    /// ツールの存在（--check / capabilities 用）。
    func pythonAvailable() -> Bool
    func jsAvailable() -> Bool
}

/// GitProvider が存在しない環境向けのダミー。
public struct NoGit: GitProvider {
    public init() {}
    public func logOutput(root: String, maxCommits: Int) -> String? { nil }
    public func checkIgnore(root: String, relPaths: [String]) -> [String]? { nil }
    public func available() -> Bool { false }
    public func isWorkTree(root: String) -> Bool { false }
}

/// ClipboardProvider が存在しない環境向けのダミー。
public struct NoClipboard: ClipboardProvider {
    public init() {}
    public func copy(_ text: String) -> Bool { false }
    public func available() -> Bool { false }
}

/// 外部 AST ツールが存在しない環境向けのダミー（常に内蔵層へ縮退）。
public struct NoAst: AstProvider {
    public init() {}
    public func pythonOutline(_ text: String, cacheKey: String) -> [OutlineItem]? { nil }
    public func pythonImports(_ text: String, cacheKey: String) -> [(String, UInt32, [String]?)]? { nil }
    public func jsOutline(_ text: String, ext: String, cacheKey: String) -> [OutlineItem]? { nil }
    public func jsImports(_ text: String, ext: String, cacheKey: String) -> [String]? { nil }
    public func pythonAvailable() -> Bool { false }
    public func jsAvailable() -> Bool { false }
}
