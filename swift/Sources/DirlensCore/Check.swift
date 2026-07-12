// `--check`: 人間向け能力レポート（精度の可視化・spec 機能5）。
// rust/crates/dirlens-core/src/check.rs の移植（Swift 版の層構成に合わせて再構成）。
//
// Swift 版の解析方式は 3 層:
//   ast     = 外部ツール（python3 の stdlib ast / node + プロジェクト内 typescript）
//   scanner = 内蔵の構造走査（文字列・コメント除去済みコード限定ビュー・ゼロ依存）
//   regex   = 正規表現（dirlens.py 互換層。DIRLENS_AST=off / DIRLENS_COMPAT=python）
// 終了コード: 最良 = 0、縮退あり = 1。

public struct EnvProbe {
    public var gitAvailable: Bool
    public var isWorkTree: Bool
    public var clipboard: Bool
    /// 外部 AST ツールの実行時可用性
    public var python3: Bool
    public var nodeTypescript: Bool

    public init(gitAvailable: Bool, isWorkTree: Bool, clipboard: Bool,
                python3: Bool, nodeTypescript: Bool) {
        self.gitAvailable = gitAvailable
        self.isWorkTree = isWorkTree
        self.clipboard = clipboard
        self.python3 = python3
        self.nodeTypescript = nodeTypescript
    }
}

private func langMethod(_ externalTool: Bool, _ enhanced: Bool) -> String {
    if !enhanced { return "regex" }
    return externalTool ? "ast" : "scanner"
}

/// capabilities メタブロック（--agent --json でも再利用）。
public func capabilitiesJson(_ cfg: Cfg, _ probe: EnvProbe) -> JSONValue {
    let e = cfg.enhancedAnalysis
    var outline = JSONObject()
    outline.insert("python", .string(langMethod(probe.python3, e)))
    outline.insert("js_ts", .string(langMethod(probe.nodeTypescript, e)))
    outline.insert("rust", .string(e ? "scanner" : "regex"))
    outline.insert("go", .string(e ? "scanner" : "regex"))
    outline.insert("c", .string(e ? "scanner" : "regex"))
    outline.insert("swift", .string(e ? "scanner" : "regex"))
    outline.insert("fallback", .string("regex"))

    var resolution = ["relative"]
    if e {
        resolution.append(contentsOf: ["tsconfig-paths", "package-imports", "go-module", "rust-module-tree"])
    } else {
        resolution.append("go-module")
    }

    let gitignoreTier = cfg.gitignoreTier
        ?? ((probe.gitAvailable && probe.isWorkTree && cfg.gitignorePreferGit) ? "git" : "builtin")

    var m = JSONObject()
    m.insert("gitignore_tier", .string(gitignoreTier))
    m.insert("outline", .object(outline))
    m.insert("imports_resolution", .array(resolution.map { .string($0) }))
    m.insert("git_log", .bool(probe.gitAvailable))
    m.insert("clipboard", .bool(probe.clipboard))
    m.insert("tokens", .string(tokensMode(cfg)))
    var tools = JSONObject()
    tools.insert("python3", .bool(probe.python3))
    tools.insert("node_typescript", .bool(probe.nodeTypescript))
    m.insert("external_tools", .object(tools))
    return .object(m)
}

/// この実行で使うトークン計数方式。
public func tokensMode(_ cfg: Cfg) -> String {
    if cfg.tokensBpe, bpeAvailable() {
        return "bpe-o200k_base"
    }
    return "char-heuristic"
}

/// --agent テキスト末尾の精度注記（1〜2 行）。
public func agentNote(_ cfg: Cfg, _ probe: EnvProbe) -> String {
    let e = cfg.enhancedAnalysis
    let gitignore: String
    switch cfg.gitignoreTier {
    case "git": gitignore = "git check-ignore(厳密)"
    case .some: gitignore = "内蔵マッチャ(fnmatch近似)"
    case nil: gitignore = "未使用"
    }
    let outline: String
    if !e {
        outline = "正規表現のみ"
    } else {
        var astLangs: [String] = []
        var scanLangs: [String] = []
        if probe.python3 { astLangs.append("py") } else { scanLangs.append("py") }
        if probe.nodeTypescript { astLangs.append("js/ts") } else { scanLangs.append("js/ts") }
        scanLangs.append(contentsOf: ["rs", "go", "c", "swift"])
        if astLangs.isEmpty {
            outline = "構文走査:\(scanLangs.joined(separator: ","))"
        } else {
            outline = "AST:\(astLangs.joined(separator: ","))・構文走査:\(scanLangs.joined(separator: ","))"
        }
    }
    let imports: String
    if !e {
        imports = "正規表現+相対パス解決"
    } else if probe.python3 || probe.nodeTypescript {
        imports = "AST+マニフェスト解決"
    } else {
        imports = "構文走査+マニフェスト解決"
    }
    let tokens = tokensMode(cfg) == "bpe-o200k_base" ? "BPE(o200k)" : "文字数概算"
    return "  解析方式: gitignore=\(gitignore) / outline=\(outline) / imports=\(imports) / tokens=\(tokens)"
}

/// --check の出力を組み立てる。戻り値: (stdout, exitCode)
public func renderCheck(_ cfg: Cfg, _ probe: EnvProbe, _ asJson: Bool) -> (String, Int32) {
    let e = cfg.enhancedAnalysis
    var degraded: [String] = []
    if !probe.gitAvailable {
        degraded.append("git が見つからない（-H 不可・gitignore は内蔵マッチャ）")
    } else if !probe.isWorkTree {
        degraded.append("対象が git work tree ではない（gitignore は内蔵マッチャ）")
    }
    if !e {
        degraded.append("構造解析が無効（正規表現のみ）")
    } else {
        if !probe.python3 {
            degraded.append("python3 が見つからない（Python は内蔵の構文走査）")
        }
        if !probe.nodeTypescript {
            degraded.append("node + typescript が見つからない（JS/TS は内蔵の構文走査）")
        }
    }
    if !probe.clipboard {
        degraded.append("クリップボードツールが見つからない（-C 不可）")
    }
    if tokensMode(cfg) != "bpe-o200k_base" {
        degraded.append("BPE トークナイザ未使用（-T は文字数概算）")
    }
    let exit: Int32 = degraded.isEmpty ? 0 : 1

    if asJson {
        var m = JSONObject()
        m.insert("schema_version", .int(Int64(schemaVersion)))
        m.insert("capabilities", capabilitiesJson(cfg, probe))
        m.insert("degraded", .array(degraded.map { .string($0) }))
        m.insert("best", .bool(exit == 0))
        return (JSONValue.object(m).pretty() + "\n", exit)
    }

    func onoff(_ b: Bool) -> String { b ? "✓" : "✗" }
    var out = ""
    out += "dirlens 能力レポート\n"
    out += "  gitignore (-G): "
        + (probe.gitAvailable && probe.isWorkTree
            ? "git check-ignore（厳密・ネスト/否定/グローバル除外に完全対応）"
            : "内蔵マッチャ（fnmatch 近似・基本パターンのみ）")
        + "\n"
    out += "  git 履歴 (-H): \(onoff(probe.gitAvailable)) git\n"
    out += "  アウトライン (-O/-A):\n"
    out += "    Python: \(langMethod(probe.python3, e))"
        + " / JS・TS: \(langMethod(probe.nodeTypescript, e))"
        + " / Rust: \(e ? "scanner" : "regex")"
        + " / Go: \(e ? "scanner" : "regex")"
        + " / C: \(e ? "scanner" : "regex")"
        + " / Swift: \(e ? "scanner" : "regex")\n"
    out += "  import 解決 (-M): "
        + (e
            ? "構文走査/AST + マニフェスト（tsconfig paths / package.json imports / go.mod / Rust モジュールツリー）"
            : "正規表現 + 相対パス解決")
        + "\n"
    out += "  トークン計数 (-T): "
        + (tokensMode(cfg) == "bpe-o200k_base"
            ? "BPE（o200k_base）による正確値（5MB 超は比例概算）"
            : "文字数ベースの概算")
        + "\n"
    out += "  クリップボード (-C): \(onoff(probe.clipboard))\n"
    out += "  外部ツール: python3 \(onoff(probe.python3)) / node+typescript \(onoff(probe.nodeTypescript))\n"
    if degraded.isEmpty {
        out += "\nすべての機能が最良の方式で動作します。\n"
    } else {
        out += "\n縮退している項目:\n"
        for d in degraded {
            out += "  - \(d)\n"
        }
    }
    return (out, exit)
}
