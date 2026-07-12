// 走査セッション（rust/crates/dirlens-core/src/session.rs の等価移植）。
// dirlens.py のモジュールグローバルキャッシュ（_sz_cache / _gi_cache）に相当する
// 状態を保持する。ロックは dir サイズの並列プリフェッチ（CLI がスレッドから
// dirSize を呼ぶ）のため。

import Foundation

public final class Session {
    public let fs: FsProvider
    /// 外部 AST ツール層（python3 / node+typescript）。無ければ NoAst。
    public let ast: AstProvider
    private let szLock = NSLock()
    private var szCache: [String: (UInt64, Bool)] = [:]
    private let giLock = NSLock()
    private var giCache: [String: [String]] = [:]
    /// Tier1（git check-ignore）で得た無視パス集合（rel path, "/" 区切り）。
    /// nil なら Tier3（内蔵マッチャ）に縮退する。
    public var gitIgnored: Set<String>? = nil

    public init(fs: FsProvider, ast: AstProvider = NoAst()) {
        self.fs = fs
        self.ast = ast
    }

    /// dir_size 相当。(合計サイズ, 読めない箇所があったか) を返す（メモ化つき）。
    /// symlink はサイズに算入しない。file でも dir でもないエントリも同様。
    public func dirSize(_ path: String) -> (UInt64, Bool) {
        szLock.lock()
        if let v = szCache[path] {
            szLock.unlock()
            return v
        }
        szLock.unlock()

        var total: UInt64 = 0
        var hasErrors = false
        if let entries = fs.scanDir(path) {
            for e in entries {
                if e.isFileNofollow {
                    if let st = fs.stat(e.path, follow: false) {
                        total &+= st.size
                    } else {
                        hasErrors = true
                    }
                } else if e.isDirNofollow {
                    let (sub, err) = dirSize(e.path)
                    total &+= sub
                    if err { hasErrors = true }
                }
            }
        } else {
            hasErrors = true
        }
        let result = (total, hasErrors)
        szLock.lock()
        szCache[path] = result
        szLock.unlock()
        return result
    }

    /// load_gitignore 相当（ディレクトリ単位でキャッシュ）。
    public func loadGitignore(_ dir: String) -> [String] {
        giLock.lock()
        if let v = giCache[dir] {
            giLock.unlock()
            return v
        }
        giLock.unlock()

        var pats: [String] = []
        let p = joinPath(dir, ".gitignore")
        // 巨大ファイルによる OOM を防ぐため上限つきで読む（正常な .gitignore は数 KB）
        if let data = fs.readPrefix(p, limit: textReadLimit) {
            let text = decodeUTF8Ignore(data)
            for line in splitLines(text) {
                let s = pyStrip(line)
                if !s.isEmpty, !s.hasPrefix("#") {
                    pats.append(s)
                }
            }
        }
        giLock.lock()
        giCache[dir] = pats
        giLock.unlock()
        return pats
    }
}
