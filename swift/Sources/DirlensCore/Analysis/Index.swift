// プロジェクト全体プレスキャン（-V/-N/-F/-M）。
// rust/crates/dirlens-core/src/analysis/index.rs の等価移植＋Swift 言語対応。

public let sourceExtsForTests: Set<String> = [".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".swift"]

private let entryNamesLower: Set<String> = [
    "main.py", "__main__.py", "app.py", "server.py", "manage.py", "wsgi.py", "asgi.py",
    "index.js", "index.ts", "index.mjs", "index.cjs",
    "main.js", "main.ts", "server.js", "server.ts", "app.js", "app.ts",
    "main.go", "main.rs", "main.swift",
    "makefile", "dockerfile", "docker-compose.yml", "docker-compose.yaml",
]

private let configNamesLower: Set<String> = [
    ".env", ".env.local", ".env.example", ".env.development", ".env.production",
    "tsconfig.json", "jsconfig.json", "babel.config.js", ".babelrc",
    "webpack.config.js", "vite.config.js", "vite.config.ts", "rollup.config.js",
    "eslint.config.js", ".eslintrc", ".eslintrc.json", ".eslintrc.js", ".prettierrc",
    "pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "pipfile",
    "cargo.toml", "go.mod", "go.sum",
    "package.swift", "package.resolved",
    "dockerfile", "docker-compose.yml", "docker-compose.yaml",
    ".gitignore", ".gitattributes",
    "tox.ini", "pytest.ini", "jest.config.js", "jest.config.ts",
    "next.config.js", "next.config.ts", "nuxt.config.js", "svelte.config.js",
    "tailwind.config.js", "tailwind.config.ts", "postcss.config.js",
    ".npmrc", ".nvmrc", ".python-version", ".ruby-version",
    "makefile", "cmakelists.txt",
]

public func isTestFile(_ name: String) -> Bool {
    let lower = name.lowercased()
    let (stem, _) = splitext(lower)
    return stem.hasPrefix("test_")
        || stem.hasSuffix("_test")
        || stem.hasSuffix(".test")
        || stem.hasSuffix(".spec")
        || stem.hasSuffix("tests") // Swift の XCTest 慣習（FooTests.swift）
}

/// posixpath.normpath 相当（"/" 区切りの相対/絶対パス用）。
public func normpath(_ p: String) -> String {
    let leading = p.hasPrefix("/")
    var comps: [String] = []
    for c in p.split(separator: "/") {
        if c.isEmpty || c == "." { continue }
        if c == ".." {
            if let last = comps.last, last != ".." {
                comps.removeLast()
            } else if !leading {
                comps.append("..")
            }
        } else {
            comps.append(String(c))
        }
    }
    let joined = comps.joined(separator: "/")
    if leading { return "/" + joined }
    return joined.isEmpty ? "." : joined
}

func pjoin(_ base: String, _ target: String) -> String {
    if target.hasPrefix("/") || base.isEmpty {
        return target
    }
    return "\(base)/\(target)"
}

/// "/" 区切りの dirname（os.path.dirname 相当）。
public func dirname(_ p: String) -> String {
    guard let idx = p.lastIndex(of: "/") else { return "" }
    return String(p[p.startIndex..<idx])
}

/// 'pkg/sub/mod.py' -> 'pkg.sub.mod'、'pkg/sub/__init__.py' -> 'pkg.sub'
func pyModuleKey(_ relpath: String) -> String {
    var parts = relpath.split(separator: "/").map(String.init)
    guard let last = parts.last else { return relpath }
    if last == "__init__.py" {
        parts.removeLast()
        return parts.joined(separator: ".")
    }
    if last.hasSuffix(".py") {
        parts[parts.count - 1] = String(last.dropLast(3))
        return parts.joined(separator: ".")
    }
    return parts.joined(separator: ".")
}

/// JS/TS の相対 import をプロジェクト内ファイルに解決する。
func resolveRelativePath(_ baseDir: String, _ target: String, _ projectFiles: Set<String>) -> String? {
    let candidate = normpath(pjoin(baseDir, target))
    let suffixes = [
        "", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
        "/index.js", "/index.ts", "/index.jsx", "/index.tsx",
    ]
    for suffix in suffixes {
        let cand = candidate + suffix
        if projectFiles.contains(cand) {
            return cand
        }
    }
    return nil
}

/// 'crate::foo::bar::Baz' をプロジェクト内ファイルに解決する（src/foo/bar.rs 等）。
func resolveRustCratePath(_ usePath: String, _ projectFiles: Set<String>) -> String? {
    guard usePath.hasPrefix("crate::") else { return nil }
    let body = String(usePath.dropFirst("crate::".count))
    let segments = body.components(separatedBy: "::")
        .filter { !$0.isEmpty && $0 != "self" && $0 != "*" }
    let n = segments.count
    for cut in [n, n - 1] where cut > 0 {
        let pathPart = segments[0..<cut].joined(separator: "/")
        for cand in ["src/\(pathPart).rs", "src/\(pathPart)/mod.rs"] {
            if projectFiles.contains(cand) {
                return cand
            }
        }
    }
    return nil
}

// ─── import 抽出（正規表現層 = Tier2・互換層） ─────────────────

/// Python import 抽出（行ベース簡易版・ast 相当の出力形式）。
/// 戻り値: (module, level, names)。level>0 は相対 import。
public func extractImportsPyLines(_ text: String) -> [(String, UInt32, [String]?)] {
    var out: [(String, UInt32, [String]?)] = []
    // 括弧が閉じるまで論理行を結合する（from x import (a,\n b) 対応）
    var logical: [String] = []
    var buf = ""
    var depth = 0
    for line in splitLines(text) {
        let code = beforeScalar(line, "#")
        if depth > 0 {
            buf += " " + code
        } else {
            buf = code
        }
        depth += code.filter { $0 == "(" }.count
        depth -= code.filter { $0 == ")" }.count
        if depth <= 0 {
            depth = 0
            logical.append(buf)
            buf = ""
        }
    }
    if !buf.isEmpty {
        logical.append(buf)
    }
    for line in logical {
        let t = pyStrip(line)
        if t.hasPrefix("import ") {
            let rest = String(t.dropFirst("import ".count))
            for item in rest.split(separator: ",", omittingEmptySubsequences: false) {
                let stripped = pyStrip(item)
                let module = stripped.split(separator: " ").first.map(String.init) ?? ""
                if !module.isEmpty {
                    out.append((module, 0, nil))
                }
            }
        } else if t.hasPrefix("from ") {
            let rest = String(t.dropFirst("from ".count))
            if let r = rest.range(of: " import ") {
                let modulePart = pyStrip(String(rest[rest.startIndex..<r.lowerBound]))
                var level: UInt32 = 0
                var idx = modulePart.startIndex
                while idx < modulePart.endIndex, modulePart[idx] == "." {
                    level += 1
                    idx = modulePart.index(after: idx)
                }
                let module = String(modulePart[idx...])
                let namesPart = String(rest[r.upperBound...])
                let trimmed = namesPart.trimmingCharacters(in: .init(charactersIn: " \t\r\n\u{0b}\u{0c}()"))
                let names = trimmed.split(separator: ",", omittingEmptySubsequences: false)
                    .map { pyStrip($0).split(separator: " ").first.map(String.init) ?? "" }
                    .filter { !$0.isEmpty }
                out.append((module, level, names))
            }
        }
    }
    return out
}

private let jsImportRes = [
    Regexp("import\\s+(?:[\\w*\\s{},]+\\s+from\\s+)?['\"]([^'\"]+)['\"]"),
    Regexp("export\\s+(?:[\\w*\\s{},]+\\s+from\\s+)?['\"]([^'\"]+)['\"]"),
    Regexp("require\\(\\s*['\"]([^'\"]+)['\"]\\s*\\)"),
    Regexp("import\\(\\s*['\"]([^'\"]+)['\"]\\s*\\)"),
]

public func extractImportsJs(_ text: String) -> [String] {
    var found: [String] = []
    for pat in jsImportRes {
        for m in pat.allMatches(text) {
            if let s = m[1] {
                found.append(s)
            }
        }
    }
    return found
}

private let goBlockRe = Regexp("(?s)import\\s*\\(([^)]*)\\)")
private let goLineRe = Regexp("import\\s+\"([^\"]+)\"")
private let goItemRe = Regexp("\"([^\"]+)\"")

public func extractImportsGo(_ text: String) -> [String] {
    var found: [String] = []
    if let b = goBlockRe.firstMatch(text), let inner = b[1] {
        for m in goItemRe.allMatches(inner) {
            if let s = m[1] {
                found.append(s)
            }
        }
    }
    for m in goLineRe.allMatches(text) {
        if let s = m[1] {
            found.append(s)
        }
    }
    return found
}

private let rsUseRe = Regexp("(?m)^\\s*(?:pub\\s+)?use\\s+([\\w:]+)")
private let rsModRe = Regexp("(?m)^\\s*(?:pub\\s+)?mod\\s+(\\w+)\\s*;")

/// 戻り値: (use 文のパスリスト, mod 宣言のモジュール名リスト)
public func extractImportsRs(_ text: String) -> ([String], [String]) {
    let uses = rsUseRe.allMatches(text).compactMap { $0[1] }
    let mods = rsModRe.allMatches(text).compactMap { $0[1] }
    return (uses, mods)
}

/// tsconfig.json 等の JSONC（コメント・末尾カンマ許容）を素の JSON に変換する。
func stripJsonc(_ text: String) -> String {
    var out = String.UnicodeScalarView()
    let chars = Array(text.unicodeScalars)
    var i = 0
    var inString = false
    while i < chars.count {
        let c = chars[i]
        if inString {
            out.append(c)
            if c == "\\", i + 1 < chars.count {
                out.append(chars[i + 1])
                i += 2
                continue
            }
            if c == "\"" {
                inString = false
            }
            i += 1
        } else if c == "\"" {
            inString = true
            out.append(c)
            i += 1
        } else if c == "/", i + 1 < chars.count, chars[i + 1] == "/" {
            while i < chars.count, chars[i] != "\n" {
                i += 1
            }
        } else if c == "/", i + 1 < chars.count, chars[i + 1] == "*" {
            i += 2
            while i + 1 < chars.count, !(chars[i] == "*" && chars[i + 1] == "/") {
                i += 1
            }
            i += 2
        } else {
            out.append(c)
            i += 1
        }
    }
    // 末尾カンマの除去
    let re = Regexp(",\\s*([}\\]])")
    var s = String(out)
    while let m = re.firstMatch(s), let whole = m[0], let tail = m[1] {
        guard let range = s.range(of: whole) else { break }
        s.replaceSubrange(range, with: tail)
    }
    return s
}

// ─── 循環依存検出 ────────────────────────────────────────────

public func detectCycles(_ importsMap: [String: [String]]) -> [[String]] {
    let white: UInt8 = 0
    let gray: UInt8 = 1
    let black: UInt8 = 2

    // 明示スタックによる DFS（3色法）。BTreeMap 相当のキー順（バイト昇順）で走査。
    var color: [String: UInt8] = [:]
    var cycles: [[String]] = []
    var seenKeys: Set<String> = []

    let sortedKeys = importsMap.keys.sorted(by: pyLess)
    for start in sortedKeys {
        if (color[start] ?? white) != white { continue }
        color[start] = gray
        var path: [String] = [start]
        var frames: [(String, Int)] = [(start, 0)]
        while let (node, i) = frames.last {
            let nexts = importsMap[node] ?? []
            if i >= nexts.count {
                path.removeLast()
                color[node] = black
                frames.removeLast()
                continue
            }
            let nxt = nexts[i]
            frames[frames.count - 1].1 += 1
            switch color[nxt] ?? white {
            case white:
                color[nxt] = gray
                path.append(nxt)
                frames.append((nxt, 0))
            case gray:
                let idx = path.firstIndex(of: nxt)!
                var cycle = Array(path[idx...])
                cycle.append(nxt)
                let key = Set(cycle.dropLast()).sorted(by: pyLess).joined(separator: "\u{0}")
                if !seenKeys.contains(key) {
                    seenKeys.insert(key)
                    cycles.append(cycle)
                }
            default:
                break
            }
        }
    }
    return cycles
}

// ─── プロジェクトインデックス本体 ─────────────────────────────

public struct ProjectIndex {
    public var untested: Set<String> = []
    public var entrySet: [String] = [] // ソート済み
    public var configSet: Set<String> = []
    public var importsMap: [String: [String]] = [:]
    public var importedByMap = OrderedDict<[String]>()
    public var externalMap: [String: [String]] = [:]
    public var cycles: [[String]] = []
}

private struct WalkState {
    var allNames: Set<String> = []
    var allRelpaths: Set<String> = []
    var sourceFiles: [(String, String, String)] = [] // (relpath, stem(原文), ext(小文字))
    var entrySet: Set<String> = []
    var pkgEntryCandidates: Set<String> = []
    var configSet: Set<String> = []
    var pyModuleMap: [String: String] = [:]
    var goModuleName: String? = nil
    // import 解決改善（マニフェスト読込・ルート直下のもののみ）
    var tsBaseUrl = ""
    var tsPaths: [(String, [String])] = []
    var pkgImports: [(String, String)] = []
}

/// Rust のモジュールツリー（module path → relpath）を src/ 配下から構築する。
/// src/lib.rs / src/main.rs = クレートルート、foo.rs / foo/mod.rs = crate::foo。
func buildRsModuleMap(_ allRelpaths: [String]) -> [[String]: String] {
    var map: [[String]: String] = [:]
    for r in allRelpaths {
        guard r.hasPrefix("src/"), r.hasSuffix(".rs") else { continue }
        let stem = String(r.dropFirst(4).dropLast(3))
        var parts = stem.split(separator: "/").map(String.init)
        let key: [String]
        if parts == ["main"] || parts == ["lib"] {
            key = []
        } else if parts.last == "mod" {
            parts.removeLast()
            key = parts
        } else {
            key = parts
        }
        // クレートルートは lib.rs を優先する
        if key.isEmpty, map[key] != nil, r.hasSuffix("main.rs") {
            continue
        }
        map[key] = r
    }
    return map
}

/// ファイルの属するモジュールパス（self:: / super:: 解決の基準）。
func rsModuleOf(_ relpath: String) -> [String]? {
    guard relpath.hasPrefix("src/"), relpath.hasSuffix(".rs") else { return nil }
    let stem = String(relpath.dropFirst(4).dropLast(3))
    var parts = stem.split(separator: "/").map(String.init)
    if parts == ["main"] || parts == ["lib"] {
        return []
    }
    if parts.last == "mod" {
        parts.removeLast()
    }
    return parts
}

/// use パスをモジュールツリーで解決する（crate:: / self:: / super:: 対応）。
func resolveRsModule(
    _ usePath: String, _ curMod: [String], _ map: [[String]: String], _ selfRelpath: String
) -> String? {
    let segs = usePath.components(separatedBy: "::").filter { !$0.isEmpty }
    guard !segs.isEmpty else { return nil }
    var base: [String]
    var idx: Int
    switch segs[0] {
    case "crate":
        base = []
        idx = 1
    case "self":
        base = curMod
        idx = 1
    case "super":
        base = curMod
        var i = 0
        while i < segs.count, segs[i] == "super" {
            if base.isEmpty { return nil }
            base.removeLast()
            i += 1
        }
        idx = i
    default:
        return nil // 外部 crate（or 2015 エディションの相対パス）は対象外
    }
    while idx < segs.count, segs[idx] == "self" || segs[idx] == "*" {
        idx += 1
    }
    for s in segs[idx...] where s != "*" && s != "self" {
        base.append(s)
    }
    let n = base.count
    for cut in [n, n - 1] where cut >= 0 {
        if let r = map[Array(base[0..<cut])], r != selfRelpath {
            return r
        }
    }
    return nil
}

/// パターン中の '*' を挟んだ前方/後方一致で spec をマッチし、'*' 部分を返す。
func starMatch(_ pat: String, _ spec: String) -> String? {
    guard let starIdx = pat.firstIndex(of: "*") else {
        return pat == spec ? "" : nil
    }
    let pre = String(pat[pat.startIndex..<starIdx])
    let post = String(pat[pat.index(after: starIdx)...])
    if spec.count >= pre.count + post.count, spec.hasPrefix(pre), spec.hasSuffix(post) {
        let start = spec.index(spec.startIndex, offsetBy: pre.count)
        let end = spec.index(spec.endIndex, offsetBy: -post.count)
        return String(spec[start..<end])
    }
    return nil
}

/// bare import を tsconfig paths / baseUrl / package.json imports で解決する（改善）。
private func resolveJsManifest(_ spec: String, _ st: WalkState) -> String? {
    if spec.hasPrefix("#") {
        for (key, target) in st.pkgImports {
            if let mid = starMatch(key, spec) {
                let cand = target.replacingOccurrences(of: "*", with: mid)
                if let r = resolveRelativePath("", cand, st.allRelpaths) {
                    return r
                }
            }
        }
        return nil
    }
    for (pat, targets) in st.tsPaths {
        if let mid = starMatch(pat, spec) {
            for t in targets {
                let cand = t.replacingOccurrences(of: "*", with: mid)
                let full = normpath(pjoin(st.tsBaseUrl, cand))
                if let r = resolveRelativePath("", full, st.allRelpaths) {
                    return r
                }
            }
        }
    }
    if !st.tsBaseUrl.isEmpty {
        let full = normpath(pjoin(st.tsBaseUrl, spec))
        if let r = resolveRelativePath("", full, st.allRelpaths) {
            return r
        }
    }
    return nil
}

public func buildProjectIndex(
    _ sess: Session, _ root: String, _ cfg: Cfg, _ activePats: [String]
) -> ProjectIndex {
    var st = WalkState()
    walk(sess, root, cfg, activePats, &st)

    // package.json の main/bin は実在するファイルのみエントリーポイントとして扱う
    for cand in st.pkgEntryCandidates where st.allRelpaths.contains(cand) {
        st.entrySet.insert(cand)
    }

    var untested: Set<String> = []
    for (relpath, stem, ext) in st.sourceFiles {
        // Rust 版と同一: 候補は原文 stem のまま照合（allNames は小文字化済み）
        var candidates = [
            "test_\(stem)\(ext)",
            "\(stem)_test\(ext)",
            "\(stem).test\(ext)",
            "\(stem).spec\(ext)",
        ]
        if ext == ".swift" {
            // XCTest 慣習: FooTests.swift / FooTest.swift（Swift 版で追加した言語のみ
            // 小文字化して照合する）
            candidates.append("\(stem.lowercased())tests\(ext)")
            candidates.append("\(stem.lowercased())test\(ext)")
        }
        if !candidates.contains(where: { st.allNames.contains($0) }) {
            untested.insert(relpath)
        }
    }

    var importsMap: [String: [String]] = [:]
    var importedByAcc = OrderedDict<Set<String>>()
    var externalMap: [String: [String]] = [:]

    if cfg.showImports {
        // Go のローカル import 解決用の前計算
        var goFilesByDir: [String: [String]] = [:]
        for r in st.allRelpaths.sorted(by: pyLess) where r.hasSuffix(".go") {
            goFilesByDir[dirname(r), default: []].append(r)
        }

        // Rust モジュールツリー（crate::/self::/super:: の解決改善用）
        let rsModuleMap = cfg.enhancedAnalysis
            ? buildRsModuleMap(st.allRelpaths.sorted(by: pyLess))
            : [:]

        for relpath in st.allRelpaths.sorted(by: pyLess) {
            let base = relpath.split(separator: "/").last.map(String.init) ?? relpath
            let ext = splitext(base).1.lowercased()
            let baseDir = dirname(relpath)
            var localTargets: Set<String> = []
            var externalRaw: [String] = []

            func readText() -> String {
                let full = joinPath(root, relpath)
                // -T の本文読込と同じ上限。import 文はファイル先頭に集中するため
                // 打ち切りの影響は実質なく、巨大ファイルによる OOM を防ぐ
                guard let data = sess.fs.readPrefix(full, limit: textReadLimit) else {
                    return ""
                }
                return decodeUTF8Ignore(data)
            }

            switch ext {
            case ".py":
                let text = readText()
                let imports: [(String, UInt32, [String]?)]
                if cfg.enhancedAnalysis {
                    imports = sess.ast.pythonImports(text, cacheKey: relpath)
                        ?? pyStructImports(text)
                } else {
                    imports = extractImportsPyLines(text)
                }
                for (module, level, names) in imports {
                    if level > 0 {
                        var pkgParts = baseDir.isEmpty
                            ? []
                            : baseDir.split(separator: "/").map(String.init)
                        let up = Int(level) - 1
                        if up > 0 {
                            if up <= pkgParts.count {
                                pkgParts.removeLast(up)
                            } else {
                                pkgParts.removeAll()
                            }
                        }
                        var keyParts = pkgParts
                        if !module.isEmpty {
                            keyParts.append(module)
                        }
                        let targetKey = keyParts.joined(separator: ".")
                        var resolved: String? = nil
                        if let names {
                            for nm in names {
                                let candKey = targetKey.isEmpty ? nm : "\(targetKey).\(nm)"
                                if let r = st.pyModuleMap[candKey] {
                                    resolved = r
                                    break
                                }
                            }
                        }
                        if resolved == nil {
                            resolved = st.pyModuleMap[targetKey]
                        }
                        if let r = resolved, r != relpath {
                            localTargets.insert(r)
                        } else {
                            externalRaw.append(String(repeating: ".", count: Int(level)) + module)
                        }
                    } else {
                        if let r = st.pyModuleMap[module], r != relpath {
                            localTargets.insert(r)
                        } else {
                            externalRaw.append(module)
                        }
                    }
                }
            case ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs":
                let text = readText()
                let specs: [String]
                if cfg.enhancedAnalysis {
                    specs = sess.ast.jsImports(text, ext: ext, cacheKey: relpath)
                        ?? jsStructImports(text, ext)
                } else {
                    specs = extractImportsJs(text)
                }
                for spec in specs {
                    if spec.hasPrefix(".") || spec.hasPrefix("/") {
                        if let r = resolveRelativePath(baseDir, spec, st.allRelpaths), r != relpath {
                            localTargets.insert(r)
                        } else {
                            externalRaw.append(spec)
                        }
                    } else {
                        // 改善: tsconfig paths / baseUrl / package.json imports で
                        // エイリアスをローカルファイルに解決してから external に落とす
                        let resolved = cfg.enhancedAnalysis ? resolveJsManifest(spec, st) : nil
                        if let r = resolved, r != relpath {
                            localTargets.insert(r)
                        } else {
                            externalRaw.append(spec)
                        }
                    }
                }
            case ".go":
                let text = readText()
                let specs = cfg.enhancedAnalysis ? goStructImports(text) : extractImportsGo(text)
                for spec in specs {
                    if let modName = st.goModuleName, spec.hasPrefix(modName) {
                        var sub = String(spec.dropFirst(modName.count))
                        while sub.hasPrefix("/") { sub = String(sub.dropFirst()) }
                        var candidates: [String] = []
                        for (d, files) in goFilesByDir.sorted(by: { pyLess($0.key, $1.key) }) {
                            if d == sub || (!sub.isEmpty && d.hasPrefix("\(sub)/")) {
                                candidates.append(contentsOf: files)
                            }
                        }
                        if !candidates.isEmpty {
                            for cand in candidates where cand != relpath {
                                localTargets.insert(cand)
                            }
                        } else {
                            externalRaw.append(spec)
                        }
                    } else {
                        externalRaw.append(spec)
                    }
                }
            case ".rs":
                let text = readText()
                let (uses, mods) = cfg.enhancedAnalysis ? rsStructImports(text) : extractImportsRs(text)
                for m in mods {
                    let cands = baseDir.isEmpty
                        ? ["\(m).rs", "\(m)/mod.rs"]
                        : ["\(baseDir)/\(m).rs", "\(baseDir)/\(m)/mod.rs"]
                    for cand in cands where st.allRelpaths.contains(cand) {
                        localTargets.insert(cand)
                    }
                }
                for u in uses {
                    // 改善: モジュールツリーで crate::/self::/super:: を解決し、
                    // 失敗時は従来の src/ ヒューリスティックへ
                    var resolved: String? = nil
                    if cfg.enhancedAnalysis, let cur = rsModuleOf(relpath) {
                        resolved = resolveRsModule(u, cur, rsModuleMap, relpath)
                    }
                    if resolved == nil {
                        if let r = resolveRustCratePath(u, st.allRelpaths), r != relpath {
                            resolved = r
                        }
                    }
                    if let r = resolved {
                        localTargets.insert(r)
                    } else {
                        externalRaw.append(u)
                    }
                }
            case ".swift":
                // Swift のモジュール import はファイル単位に対応しないため external 扱い
                // （Swift 版で追加した言語。-M では外部モジュール一覧として見える）
                guard cfg.enhancedAnalysis else { break }
                let text = readText()
                externalRaw.append(contentsOf: swiftStructImports(text))
            default:
                break
            }

            if !localTargets.isEmpty {
                let sorted = localTargets.sorted(by: pyLess)
                importsMap[relpath] = sorted
                for t in sorted {
                    importedByAcc.update(t, default: []) { $0.insert(relpath) }
                }
            }
            if !externalRaw.isEmpty {
                var seen: [String] = []
                for x in externalRaw where !x.isEmpty && !seen.contains(x) {
                    seen.append(x)
                }
                if seen.count > 10 {
                    seen.removeLast(seen.count - 10)
                }
                externalMap[relpath] = seen
            }
        }
    }

    var importedByMap = OrderedDict<[String]>()
    for (k, v) in importedByAcc.pairs {
        importedByMap[k] = v.sorted(by: pyLess)
    }
    let cycles = cfg.showImports ? detectCycles(importsMap) : []

    var index = ProjectIndex()
    index.untested = untested
    index.entrySet = st.entrySet.sorted(by: pyLess)
    index.configSet = st.configSet
    index.importsMap = importsMap
    index.importedByMap = importedByMap
    index.externalMap = externalMap
    index.cycles = cycles
    return index
}

private func walk(
    _ sess: Session, _ path: String, _ cfg: Cfg, _ activePats: [String], _ st: inout WalkState
) {
    let pats = cfg.useGitignore ? extendPats(sess, activePats, path, cfg) : activePats
    guard let rawEntries = sess.fs.scanDir(path) else { return }
    var entries = rawEntries.filter { cfg.showAll || !firstScalarIs($0.name, ".") }
    if let gitSet = sess.gitIgnored {
        if cfg.useGitignore {
            entries = entries.filter { !gitSet.contains(relpathSlash($0.path, cfg.root)) }
        }
    } else if !pats.isEmpty {
        entries = entries.filter {
            !isIgnored($0.name, relpath($0.path, cfg.root), $0.isDirNofollow, pats)
        }
    }
    for e in entries {
        if e.isDirNofollow {
            walk(sess, e.path, cfg, pats, &st)
            continue
        }
        let rel = relpathSlash(e.path, cfg.root)
        let (stem, extRaw) = splitext(e.name)
        let ext = extRaw.lowercased()
        let nameLower = e.name.lowercased()
        st.allNames.insert(nameLower)
        st.allRelpaths.insert(rel)
        if sourceExtsForTests.contains(ext), !isTestFile(e.name) {
            st.sourceFiles.append((rel, stem, ext))
        }
        if entryNamesLower.contains(nameLower) {
            st.entrySet.insert(rel)
        }
        if configNamesLower.contains(nameLower) {
            st.configSet.insert(rel)
        }
        if ext == ".py" {
            st.pyModuleMap[pyModuleKey(rel)] = rel
        }
        if e.name == "go.mod" {
            if let data = sess.fs.readPrefix(e.path, limit: textReadLimit) {
                let text = decodeUTF8Ignore(data)
                for line in splitLines(text) {
                    let s = pyStrip(line)
                    if s.hasPrefix("module ") {
                        st.goModuleName = pyStrip(String(s.dropFirst("module".count)))
                        break
                    }
                }
            }
        }
        if (e.name == "tsconfig.json" || e.name == "jsconfig.json"), !rel.contains("/") {
            // ルート直下のみ対象。tsconfig を優先（jsconfig は未設定時のみ反映）
            if e.name == "tsconfig.json" || (st.tsPaths.isEmpty && st.tsBaseUrl.isEmpty) {
                if let data = sess.fs.readPrefix(e.path, limit: textReadLimit) {
                    let text = stripJsonc(decodeUTF8Ignore(data))
                    if let v = JSONParser.parse(text), let co = v.get("compilerOptions") {
                        if let b = co.get("baseUrl")?.asString {
                            var url = b
                            if url.hasPrefix("./") { url = String(url.dropFirst(2)) }
                            st.tsBaseUrl = url == "." ? "" : url
                        }
                        if let paths = co.get("paths")?.asObject {
                            st.tsPaths = paths.pairs.map { key, value in
                                let targets = value.asArray?.compactMap { $0.asString } ?? []
                                return (key, targets)
                            }
                        }
                    }
                }
            }
        }
        if e.name == "package.json" {
            if let data = sess.fs.readPrefix(e.path, limit: textReadLimit),
               let text = String(bytes: data, encoding: .utf8),
               let pkg = JSONParser.parse(text) {
                let baseDir = dirname(rel)
                // ルート package.json の "imports"（# エイリアス）を記録
                if baseDir.isEmpty, let imp = pkg.get("imports")?.asObject {
                    for (k, v) in imp.pairs {
                        if let s = v.asString {
                            st.pkgImports.append((k, s))
                        }
                    }
                }
                func add(_ v: String) {
                    st.pkgEntryCandidates.insert(normpath(pjoin(baseDir, v)))
                }
                if let main = pkg.get("main")?.asString {
                    add(main)
                }
                switch pkg.get("bin") {
                case .some(.string(let s)):
                    add(s)
                case .some(.object(let map)):
                    for (_, v) in map.pairs {
                        if let s = v.asString {
                            add(s)
                        }
                    }
                default:
                    break
                }
            }
        }
    }
}
