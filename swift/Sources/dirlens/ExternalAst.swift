// 外部 AST ツール層（Tier1）。
//
// 外部依存で精度が上がる解析は、実行時にツールを探して使う:
//   - Python: python3 の標準ライブラリ `ast`（CPython そのもの = 最高忠実度）
//   - JS/TS:  node + 対象プロジェクトの node_modules 内 typescript
// どちらも見つからない / 起動失敗 / パース失敗の場合は nil を返し、
// コアが内蔵の構造走査（Tier1.5）→ 正規表現（Tier2）へフォールバックする。
//
// 通信は JSON Lines の常駐コプロセス（ファイルごとの起動コストを避ける）。

import Foundation
import DirlensCore

/// JSON Lines で会話する常駐コプロセス。応答がタイムアウトしたら破棄する。
final class ToolProcess {
    private let launchPath: String
    private let arguments: [String]
    private var proc: Process? = nil
    private var stdinFh: FileHandle? = nil
    private var broken = false
    private let lock = NSLock()
    // 受信行キュー（リーダスレッドが供給する）
    private let lineCond = NSCondition()
    private var lines: [String] = []
    private var readerEof = false

    init(launchPath: String, arguments: [String]) {
        self.launchPath = launchPath
        self.arguments = arguments
    }

    private func ensureStarted() -> Bool {
        if broken { return false }
        if proc != nil { return true }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: launchPath)
        p.arguments = arguments
        let inPipe = Pipe()
        let outPipe = Pipe()
        p.standardInput = inPipe
        p.standardOutput = outPipe
        p.standardError = FileHandle.nullDevice
        do {
            try p.run()
        } catch {
            broken = true
            return false
        }
        proc = p
        stdinFh = inPipe.fileHandleForWriting
        let readFh = outPipe.fileHandleForReading
        Thread.detachNewThread { [weak self] in
            var buf = Data()
            while true {
                let chunk = readFh.availableData
                if chunk.isEmpty { break } // EOF
                buf.append(chunk)
                while let nl = buf.firstIndex(of: 0x0A) {
                    let lineData = buf.subdata(in: buf.startIndex..<nl)
                    buf.removeSubrange(buf.startIndex...nl)
                    guard let self else { return }
                    self.lineCond.lock()
                    self.lines.append(String(decoding: lineData, as: UTF8.self))
                    self.lineCond.signal()
                    self.lineCond.unlock()
                }
            }
            guard let self else { return }
            self.lineCond.lock()
            self.readerEof = true
            self.lineCond.signal()
            self.lineCond.unlock()
        }
        return true
    }

    private func markBroken() {
        broken = true
        if let p = proc, p.isRunning {
            kill(p.processIdentifier, SIGKILL)
        }
        proc = nil
        stdinFh = nil
    }

    /// 1 リクエスト（JSON 1 行）を送り、応答 1 行を待つ。失敗時 nil（以後は常に nil）。
    func request(_ line: String, timeout: TimeInterval) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard ensureStarted(), let stdinFh else { return nil }
        var data = Data(line.utf8)
        data.append(0x0A)
        do {
            try stdinFh.write(contentsOf: data)
        } catch {
            markBroken()
            return nil
        }
        let deadline = Date().addingTimeInterval(timeout)
        lineCond.lock()
        defer { lineCond.unlock() }
        while lines.isEmpty {
            if readerEof || !lineCond.wait(until: deadline) {
                lineCond.unlock()
                markBroken()
                lineCond.lock()
                return nil
            }
        }
        return lines.removeFirst()
    }
}

/// 応答共通: {"id":n, "outline":[[kind,name,pub],...], "imports":[...]} / {"error":true}
private struct AstReply {
    var outline: [OutlineItem]? = nil
    var pyImports: [(String, UInt32, [String]?)]? = nil
    var jsImports: [String]? = nil
}

private func parseReply(_ line: String, js: Bool) -> AstReply? {
    guard let v = JSONParser.parse(line) else { return nil }
    if case .some(.bool(true)) = v.get("error") { return nil }
    var reply = AstReply()
    if let arr = v.get("outline")?.asArray {
        var items: [OutlineItem] = []
        for item in arr {
            guard let t = item.asArray, t.count == 3,
                  let kind = t[0].asString, let name = t[1].asString,
                  case .bool(let pub) = t[2]
            else { return nil }
            items.append(OutlineItem(kind, name, pub))
        }
        reply.outline = items
    }
    if let arr = v.get("imports")?.asArray {
        if js {
            reply.jsImports = arr.compactMap { $0.asString }
        } else {
            var imports: [(String, UInt32, [String]?)] = []
            for item in arr {
                guard let t = item.asArray, t.count == 3,
                      let module = t[0].asString, case .int(let level) = t[1]
                else { return nil }
                var names: [String]? = nil
                if let nameArr = t[2].asArray {
                    names = nameArr.compactMap { $0.asString }
                }
                imports.append((module, UInt32(clamping: level), names))
            }
            reply.pyImports = imports
        }
    }
    return reply
}

private func encodeRequest(_ id: Int, _ code: String, _ ext: String) -> String {
    // pretty() は複数行になるため 1 行 JSON を手組みする
    return "{\"id\": \(id), \"ext\": \(jsonEscape(ext)), \"code\": \(jsonEscape(code))}"
}

/// python3（stdlib ast）+ node/typescript の AstProvider 実装。
final class ExternalAst: AstProvider {
    private let pythonPath: String?
    private let nodePath: String?
    private let typescriptPath: String?
    private var pyProc: ToolProcess? = nil
    private var jsProc: ToolProcess? = nil
    private var pyCache: [String: AstReply?] = [:]
    private var jsCache: [String: AstReply?] = [:]
    private var nextId = 0
    private let lock = NSLock()

    init(root: String) {
        let disabled = ProcessInfo.processInfo.environment["DIRLENS_EXTERNAL"] == "off"
        pythonPath = disabled ? nil : findCmd("python3")
        nodePath = disabled ? nil : findCmd("node")
        // typescript は対象プロジェクト（またはその祖先）の node_modules から探す
        var tsPath: String? = nil
        if nodePath != nil {
            var dir = root
            for _ in 0..<10 {
                let cand = joinPath(dir, "node_modules/typescript/lib/typescript.js")
                if FileManager.default.fileExists(atPath: cand) {
                    tsPath = cand
                    break
                }
                let parent = dirname(dir)
                if parent.isEmpty || parent == dir { break }
                dir = parent
            }
        }
        typescriptPath = tsPath
    }

    func pythonAvailable() -> Bool {
        return pythonPath != nil
    }

    func jsAvailable() -> Bool {
        return nodePath != nil && typescriptPath != nil
    }

    private func pyRequest(_ text: String, _ cacheKey: String) -> AstReply? {
        lock.lock()
        defer { lock.unlock() }
        if let cached = pyCache[cacheKey] {
            return cached
        }
        guard let pythonPath else {
            pyCache.updateValue(nil, forKey: cacheKey)
            return nil
        }
        if pyProc == nil {
            pyProc = ToolProcess(launchPath: pythonPath, arguments: ["-c", pythonAstScript])
        }
        nextId += 1
        let line = encodeRequest(nextId, text, ".py")
        let reply = pyProc!.request(line, timeout: 20).flatMap { parseReply($0, js: false) }
        pyCache.updateValue(reply, forKey: cacheKey)
        return reply
    }

    private func jsRequest(_ text: String, _ ext: String, _ cacheKey: String) -> AstReply? {
        lock.lock()
        defer { lock.unlock() }
        if let cached = jsCache[cacheKey] {
            return cached
        }
        guard let nodePath, let typescriptPath else {
            jsCache.updateValue(nil, forKey: cacheKey)
            return nil
        }
        if jsProc == nil {
            jsProc = ToolProcess(launchPath: nodePath, arguments: ["-e", nodeAstScript, "--", typescriptPath])
        }
        nextId += 1
        let line = encodeRequest(nextId, text, ext)
        let reply = jsProc!.request(line, timeout: 20).flatMap { parseReply($0, js: true) }
        jsCache.updateValue(reply, forKey: cacheKey)
        return reply
    }

    func pythonOutline(_ text: String, cacheKey: String) -> [OutlineItem]? {
        return pyRequest(text, cacheKey)?.outline
    }

    func pythonImports(_ text: String, cacheKey: String) -> [(String, UInt32, [String]?)]? {
        return pyRequest(text, cacheKey)?.pyImports
    }

    func jsOutline(_ text: String, ext: String, cacheKey: String) -> [OutlineItem]? {
        return jsRequest(text, ext, cacheKey)?.outline
    }

    func jsImports(_ text: String, ext: String, cacheKey: String) -> [String]? {
        guard var found = jsRequest(text, ext, cacheKey)?.jsImports else { return nil }
        // require() / 動的 import() は構文走査で補完する（Rust 版が AST の後に
        // 正規表現で補完していたのと同じ役割。文字列内の偽 import は拾わない）
        let scanned = jsStructImports(text, ext)
        let stmtSet = Set(found)
        for s in scanned where !stmtSet.contains(s) {
            found.append(s)
        }
        return found
    }
}

// ─── 埋め込みスクリプト ───────────────────────────────────────

/// python3 -c で常駐させるスクリプト。stdin から JSON Lines を読み、
/// stdlib ast で outline（ソース順・ネスト込み）と imports（ast.walk = BFS）を返す。
let pythonAstScript = #"""
import ast, json, sys

def child_bodies(s):
    if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
        return [s.body]
    if isinstance(s, (ast.For, ast.AsyncFor, ast.While, ast.If)):
        return [s.body, s.orelse]
    if isinstance(s, (ast.With, ast.AsyncWith)):
        return [s.body]
    if isinstance(s, ast.Try) or (hasattr(ast, "TryStar") and isinstance(s, ast.TryStar)):
        v = [s.body]
        for h in s.handlers:
            v.append(h.body)
        v.append(s.orelse)
        v.append(s.finalbody)
        return v
    if hasattr(ast, "Match") and isinstance(s, ast.Match):
        return [c.body for c in s.cases]
    return []

def outline(tree):
    out = []
    def walk(stmts):
        for s in stmts:
            if isinstance(s, ast.ClassDef):
                out.append(["class", s.name, not s.name.startswith("_")])
                walk(s.body)
            elif isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)):
                out.append(["def", s.name, not s.name.startswith("_")])
                walk(s.body)
            else:
                for b in child_bodies(s):
                    walk(b)
    walk(tree.body)
    return out

def imports(tree):
    out = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for a in node.names:
                out.append([a.name, 0, None])
        elif isinstance(node, ast.ImportFrom):
            out.append([node.module or "", node.level or 0,
                        [a.name for a in node.names]])
    return out

for line in sys.stdin:
    if not line.strip():
        continue
    try:
        req = json.loads(line)
        tree = ast.parse(req["code"])
        res = {"id": req["id"], "outline": outline(tree), "imports": imports(tree)}
    except Exception:
        try:
            rid = req.get("id")
        except Exception:
            rid = None
        res = {"id": rid, "error": True}
    sys.stdout.write(json.dumps(res) + "\n")
    sys.stdout.flush()
"""#

/// node -e で常駐させるスクリプト。argv 末尾に typescript.js のパスを取り、
/// ts.createSourceFile で oxc 版と同じ対象（トップレベルの class / function /
/// 関数初期化の変数、import/export のソース）を抽出する。
let nodeAstScript = #"""
const tsPath = process.argv[process.argv.length - 1];
let ts;
try { ts = require(tsPath); } catch (e) { process.exit(1); }
const readline = require("readline");
const rl = readline.createInterface({ input: process.stdin, terminal: false });

function scriptKind(ext) {
  switch (ext) {
    case ".ts": return ts.ScriptKind.TS;
    case ".tsx": return ts.ScriptKind.TSX;
    case ".jsx": return ts.ScriptKind.JSX;
    default: return ts.ScriptKind.JS;
  }
}

function hasMod(node, kind) {
  const mods = ts.canHaveModifiers && ts.canHaveModifiers(node)
    ? ts.getModifiers(node) : node.modifiers;
  return !!(mods && mods.some((m) => m.kind === kind));
}

rl.on("line", (line) => {
  if (!line.trim()) return;
  let res;
  try {
    const req = JSON.parse(line);
    const sf = ts.createSourceFile("f" + req.ext, req.code,
      ts.ScriptTarget.Latest, false, scriptKind(req.ext));
    if (sf.parseDiagnostics && sf.parseDiagnostics.length > 0) {
      res = { id: req.id, error: true };
    } else {
      const outline = [];
      const imports = [];
      const isFn = (init) => !!init
        && (init.kind === ts.SyntaxKind.ArrowFunction
          || init.kind === ts.SyntaxKind.FunctionExpression);
      for (const stmt of sf.statements) {
        const exported = hasMod(stmt, ts.SyntaxKind.ExportKeyword);
        if (ts.isClassDeclaration(stmt)) {
          if (stmt.name) outline.push(["class", stmt.name.text, exported]);
        } else if (ts.isFunctionDeclaration(stmt)) {
          if (stmt.name) outline.push(["func", stmt.name.text, exported]);
        } else if (ts.isVariableStatement(stmt)) {
          for (const d of stmt.declarationList.declarations) {
            if (isFn(d.initializer) && ts.isIdentifier(d.name)) {
              outline.push(["func", d.name.text, exported]);
            }
          }
        } else if (ts.isImportDeclaration(stmt)) {
          if (ts.isStringLiteral(stmt.moduleSpecifier)) {
            imports.push(stmt.moduleSpecifier.text);
          }
        } else if (ts.isExportDeclaration(stmt)) {
          if (stmt.moduleSpecifier && ts.isStringLiteral(stmt.moduleSpecifier)) {
            imports.push(stmt.moduleSpecifier.text);
          }
        }
      }
      res = { id: req.id, outline, imports };
    }
  } catch (e) {
    res = { id: null, error: true };
  }
  process.stdout.write(JSON.stringify(res) + "\n");
});
"""#
