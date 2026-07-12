// ファイル単位の追加情報（rust/crates/dirlens-core/src/analysis/extras.rs の等価移植）。

public struct FileExtras {
    public var tokens: Int64? = nil // nil = バイナリ/読めない（表示しない）
    public var lines: Int64? = nil
    public var git: GitInfo? = nil
    public var todos: [(Int, String, String)] = []
    public var isEntry = false
    public var isConfig = false
    public var noTest = false
    public var outline: [OutlineItem]? = nil // nil = 対応外言語
    public var imports: [String] = []
    public var importedBy: [String] = []
    public var externalImports: [String] = []

    public init() {}
}

/// アウトライン抽出のディスパッチ（3層）:
/// Tier1  = 外部ツール（python3 の ast / node+typescript）
/// Tier1.5 = 内蔵の構造走査（コード限定ビュー）
/// Tier2  = 正規表現（互換層。enhancedAnalysis=false 時はこれのみ）
func dispatchOutline(_ sess: Session, _ cfg: Cfg, _ text: String, _ ext: String, _ cacheKey: String) -> [OutlineItem]? {
    if !cfg.enhancedAnalysis {
        return extractOutline(text, ext)
    }
    switch ext {
    case ".py":
        if let items = sess.ast.pythonOutline(text, cacheKey: cacheKey) {
            return items
        }
    case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs":
        if let items = sess.ast.jsOutline(text, ext: ext, cacheKey: cacheKey) {
            return items
        }
    default:
        break
    }
    if let items = structOutline(text, ext) {
        return items
    }
    return extractOutline(text, ext)
}

public func fileExtras(_ sess: Session, _ entry: FSEntry, _ rel: String, _ cfg: Cfg) -> FileExtras {
    var ex = FileExtras()
    let lowerName = entry.name.lowercased()
    let ext = splitext(lowerName).1

    // 本文はここで一度だけ読み込んで共有する
    let needText = cfg.showTokens || cfg.showTodo || cfg.showOutline
    var isBinary = isProbablyBinary(entry.name)
    var text = ""
    var byteLen = 0
    var truncated = false
    if needText, !isBinary {
        if var data = sess.fs.readPrefix(entry.path, limit: textReadLimit + 1) {
            let headLen = min(data.count, 8192)
            if data[0..<headLen].contains(0) {
                isBinary = true
            } else {
                truncated = data.count > textReadLimit
                if truncated {
                    data.removeLast(data.count - textReadLimit)
                }
                byteLen = data.count
                text = decodeUTF8Ignore(data)
            }
        }
    }

    if cfg.showTokens {
        if isBinary {
            ex.tokens = nil
            ex.lines = nil
        } else {
            let sz = sess.fs.stat(entry.path, follow: true)?.size
            ex.tokens = countTokens(text, byteLen, sz, truncated, cfg.tokensBpe)
            ex.lines = countLines(text, byteLen, sz, truncated)
        }
    }

    if cfg.showGit {
        ex.git = cfg.gitMap[rel]
    }

    if cfg.showTodo {
        ex.todos = scanTodos(text)
    }

    if cfg.showEntry {
        ex.isEntry = cfg.entrySet.contains(rel)
    }

    if cfg.showConfig {
        ex.isConfig = cfg.configSet.contains(rel)
    }

    if cfg.showTests {
        ex.noTest = cfg.untestedSet.contains(rel)
    }

    if cfg.showOutline {
        var outline = dispatchOutline(sess, cfg, text, ext, entry.path)
        if outline != nil, !outline!.isEmpty, cfg.publicOnly {
            outline = outline!.filter { $0.isPublic }
        }
        ex.outline = outline
    }

    if cfg.showImports {
        ex.imports = cfg.importsMap[rel] ?? []
        ex.importedBy = cfg.importedByMap[rel] ?? []
        ex.externalImports = cfg.externalMap[rel] ?? []
    }

    return ex
}

/// 「読み始めの候補」（reading_order_candidates 相当）。
public func readingOrderCandidates(_ cfg: Cfg, _ topN: Int, _ limit: Int) -> [String] {
    var cand = cfg.entrySet
    if !cfg.importedByMap.isEmpty {
        var items = cfg.importedByMap.pairs.map { ($0.0, $0.1.count) }
        // 安定ソート: タイは挿入順を保つ
        stableSortByKey(&items, items.map { $0.1 }, true) { $0 < $1 }
        for (p, _) in items.prefix(topN) where !cand.contains(p) {
            cand.append(p)
        }
    }
    if cand.count > limit {
        cand.removeLast(cand.count - limit)
    }
    return cand
}
