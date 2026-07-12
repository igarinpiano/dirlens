// dirlens – ファイルサイズ付きディレクトリツリー表示ツール（Swift 版 CLI）
//
// Copyright 2026 Igarin
// Licensed under the Apache License, Version 2.0.
// See the LICENSE file or http://www.apache.org/licenses/LICENSE-2.0
//
// rust/crates/dirlens-cli/src/main.rs の等価移植。

import Foundation
import DirlensCore

/// dirlens.py の _enable_color 相当（POSIX: stdout が端末かどうか）。
func enableColor() -> Bool {
    return isatty(1) != 0
}

func writeStdout(_ s: String) {
    FileHandle.standardOutput.write(Data(s.utf8))
}

func writeStderr(_ s: String) {
    FileHandle.standardError.write(Data(s.utf8))
}

let argv = Array(CommandLine.arguments.dropFirst())
let action = parseCli(argv)

var args: Args
switch action {
case .help:
    writeStdout(helpText + "\n")
    exit(0)
case .version:
    writeStdout("dirlens \(dirlensVersion)\n")
    exit(0)
case .error(let msg):
    writeStderr("エラー: \(msg)\n（--help で使い方を表示）\n")
    exit(2)
case .run(let a):
    args = a
}

let fs = StdFs()
let git = StdGit()
let clip = StdClipboard()

var cfg: Cfg
switch prepare(args, fs, enableColor()) {
case .success(let c):
    cfg = c
case .failure(let res):
    writeStderr(res.stderr)
    exit(res.exitCode)
}

// gitignore 層の選択（テスト・検証用の環境変数。通常は auto = Tier1 を試す）:
//   DIRLENS_GITIGNORE=builtin … 内蔵マッチャ（Tier3）を強制
//   DIRLENS_COMPAT=python     … Python 版完全互換モード（ゴールデン検証用）
let env = ProcessInfo.processInfo.environment
let compatPython = env["DIRLENS_COMPAT"] == "python"
switch env["DIRLENS_GITIGNORE"] {
case "builtin":
    cfg.gitignorePreferGit = false
case "git":
    cfg.gitignorePreferGit = true
default:
    if compatPython {
        cfg.gitignorePreferGit = false
    }
}
// 構造走査＋外部 AST＋import 解決改善の無効化（DIRLENS_AST=off または互換モード）
if compatPython || env["DIRLENS_AST"] == "off" {
    cfg.enhancedAnalysis = false
}
// トークン計数層の選択（DIRLENS_TOKENS=heuristic で Tier2 固定）
if compatPython || env["DIRLENS_TOKENS"] == "heuristic" {
    cfg.tokensBpe = false
}
// 互換モードでは精度注記・schema_version・capabilities も出さない
if compatPython {
    cfg.suppressNotes = true
}

// 外部 AST ツール（python3 / node+typescript）。互換モードでは使わない。
let ast: AstProvider = cfg.enhancedAnalysis ? ExternalAst(root: cfg.root) : NoAst()
let sess = Session(fs: fs, ast: ast)

// ── トップレベル dir サイズの並列プリフェッチ ──────────────────
let tops = prefetchTargets(sess, cfg)
if tops.count >= 2 {
    DispatchQueue.concurrentPerform(iterations: tops.count) { i in
        _ = sess.dirSize(tops[i])
    }
}

let res = execute(sess, cfg, git, clip)

if let (path, content) = res.htmlFile {
    do {
        try Data(content.utf8).write(to: URL(fileURLWithPath: path))
    } catch {
        writeStderr("エラー: '\(path)' に書き込めません: \(error.localizedDescription)\n")
        exit(1)
    }
}

writeStdout(res.stdout)
writeStderr(res.stderr)
exit(res.exitCode)
