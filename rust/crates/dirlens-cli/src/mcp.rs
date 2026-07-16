//! MCP（Model Context Protocol）サーバーモード（--mcp）。
//!
//! stdio 上の改行区切り JSON-RPC 2.0。エージェントホスト（Claude Code / Cursor 等）に
//! dirlens の解析をネイティブツールとして公開する。外部依存なしの手書き実装
//! （必要なのは initialize / tools/list / tools/call / ping のみ）。
//!
//! 登録例（Claude Code）: `claude mcp add dirlens -s user -- dirlens --mcp`
//! （-s user を付けないと local scope になり、登録時のカレントディレクトリの
//! プロジェクトにしか紐づかない。dirlensは汎用ツールなのでuser scopeが前提）

use std::io::{BufRead, Write};

use serde_json::{json, Map, Value};

use dirlens_core::provider::NoClipboard;
use dirlens_core::{run, Args};

use crate::providers::{StdFs, StdGit};

const PROTOCOL_VERSION: &str = "2024-11-05";

fn tool_defs() -> Value {
    let obj = |props: Value, required: Vec<&str>| {
        json!({"type": "object", "properties": props, "required": required})
    };
    let path_prop = json!({"type": "string", "description": "target directory (default: current directory)"});
    let depth_prop = json!({"type": "integer", "description": "how many directory levels deep to show (default: unlimited, i.e. the full tree). Project-wide counts and summaries (TODO count, hotspots, etc.) always cover the whole project regardless of this value — only the tree/listing gets shallower. If you don't know how big the project is yet, don't omit this on the first call: pass a small value like 1 or 2, or use `estimate`/`budget` where available instead of guessing"});
    let unlimited_depth_prop = json!({"type": "boolean", "description": "set true to force the full, unlimited-depth tree even though this tool normally defaults `depth` to a small number when it's omitted. Ignored if `depth` is also set (an explicit `depth` always wins)"});
    json!([
        {
            "name": "analyze",
            "description": "Full project analysis (tree + tokens + git + TODOs + missing tests + entry points + outline + import graph + config files) as JSON. Equivalent to `dirlens --agent --json`. Best first call to understand an unfamiliar project. Guidance for the FIRST call on a project you know nothing about: call with `estimate: true` (returns a few lines, negligible cost) to see the token cost per depth level, THEN decide — either set `depth` to a small number (1-2) for a quick look, or set `budget` to a token ceiling and let it auto-fit. Calling this with no `depth`/`budget`/`estimate` on a large project can return tens of thousands of tokens and get truncated by the host.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "depth": depth_prop,
                "budget": {"type": "integer", "description": "cap the response to about this many tokens (o200k BPE) by auto-trimming depth, then annotations, then tree rows. When set, returns compact annotated TEXT instead of JSON — use this instead of guessing a `depth`"},
                "estimate": {"type": "boolean", "description": "instead of running the analysis, return a few-line table of token cost per depth level (-L 1, -L 2, -L 3, full). Use this FIRST on any project whose size you don't know, to pick a sensible `depth` or `budget`. Overrides `budget` if both are set"}
            }), vec![])
        },
        {
            "name": "tree",
            "description": "Plain directory tree with sizes (gitignore applied), as text. On a large or unfamiliar project, pass `depth` (e.g. 1-2) or `budget` on the first call to avoid a very long response — or use `top` if you only want the biggest files/directories rather than the project shape.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "depth": depth_prop,
                "budget": {"type": "integer", "description": "cap the response to about this many tokens (o200k BPE) by auto-trimming depth, then tree rows"},
                "top": {"type": "integer", "description": "return a flat list of the N largest files and directories instead of a tree — cheap and safe on any project size, no depth guessing needed"}
            }), vec![])
        },
        {
            "name": "outline",
            "description": "Function/class outline, token count, and TODOs for specific files (AST-based; Python/JS/TS/Rust/Go/C/Java/Ruby/PHP/C#/Kotlin/Swift), as JSON. Accepts multiple files at once — prefer this over calling it once per file. If `files` is omitted, it instead walks the whole project and returns its public API (public symbols only, like `dirlens -A`); in that mode `depth` defaults to 2 to keep the response small. Pass a larger `depth`, or `unlimited_depth: true` for no limit at all (may be a very large response on a big project), or pass `files` explicitly once you know which ones you need.",
            "inputSchema": obj(json!({
                "files": {"type": "array", "items": {"type": "string"}, "description": "file paths to analyze (resolved against `path` when relative). Omit for a project-wide public API outline"},
                "path": path_prop,
                "depth": depth_prop,
                "unlimited_depth": unlimited_depth_prop
            }), vec![])
        },
        {
            "name": "imports",
            "description": "Local import/dependency graph: most-depended-on files, circular dependencies, and a flat JSON list of files that actually import something or are imported (files with no import relationships at all are omitted, unlike `analyze`/`tree` annotations — cheap on any project size, no `depth`/`budget` needed). Set `format` to get a Mermaid or Graphviz DOT diagram of the whole graph instead (unfiltered, since diagrams need the full graph).",
            "inputSchema": obj(json!({
                "path": path_prop,
                "format": {"type": "string", "enum": ["json", "mermaid", "dot"], "description": "output format (default: json, flat and filtered as described above). mermaid/dot return the full unfiltered graph as a diagram"},
                "limit": {"type": "integer", "description": "cap the number of files in the flat JSON list (default: unlimited). Ignored for mermaid/dot format"}
            }), vec![])
        },
        {
            "name": "focus",
            "description": "Impact analysis for one file: what it imports and what imports it (direct + transitive), as JSON. Use before refactoring to see the blast radius.",
            "inputSchema": obj(json!({"path": path_prop, "file": {"type": "string", "description": "project-relative source file"}}), vec!["file"])
        },
        {
            "name": "todos",
            "description": "TODO/FIXME/HACK/XXX comments across the project, as a flat JSON list of {path, line, kind, text} — only files that actually have one are included (files with none are omitted, unlike `analyze`/`tree` annotations — cheap on any project size, no `depth`/`budget` needed).",
            "inputSchema": obj(json!({
                "path": path_prop,
                "limit": {"type": "integer", "description": "cap the number of TODO items returned (default: unlimited)"}
            }), vec![])
        },
        {
            "name": "since",
            "description": "Only the files changed since a git ref (default HEAD), as a JSON tree annotated with tokens, outline, and TODOs per changed file. Much cheaper than re-running analyze mid-session.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "ref": {"type": "string", "description": "git ref to diff against (default: HEAD, i.e. uncommitted changes)"}
            }), vec![])
        },
        {
            "name": "history",
            "description": "Recent git activity as compact text: tree annotated with each file's last commit, plus frequently-changed hotspot files. `depth` defaults to 1 here (unlike other tools) to stay small automatically — the hotspot list itself always covers the whole project regardless, so you rarely need to raise it. Pass `depth` explicitly, or `unlimited_depth: true`, to see every file's last-commit line instead of just the top level.",
            "inputSchema": obj(json!({"path": path_prop, "depth": depth_prop, "unlimited_depth": unlimited_depth_prop}), vec![])
        },
        {
            "name": "api_diff",
            "description": "Public API diff against a git ref (added/removed public symbols) to spot breaking changes, as text.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "ref": {"type": "string", "description": "git ref to compare the public API against (e.g. a release tag or HEAD~5)"}
            }), vec!["ref"])
        }
    ])
}

/// ツール実行。成功時は (テキスト, is_error=false)。
fn run_tool(name: &str, args_val: &Map<String, Value>) -> (String, bool) {
    let path = args_val
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    let depth = args_val.get("depth").and_then(|v| v.as_i64());
    // depth 未指定時に既定値を入れるツール（outline の -A 相当 / history）向けの
    // オプトアウト。true なら既定値を入れず None（無制限）のままにする
    let unlimited_depth = args_val
        .get("unlimited_depth")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // todos / imports のフラット出力用（該当なしファイルの空配列を除いた後の件数上限）
    let limit = args_val
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let mut a = Args {
        path,
        depth,
        lang: Some("en".to_string()),
        ..Default::default()
    };
    match name {
        "analyze" => {
            a.agent = true;
            // --budget はコアがテキスト経路でのみ処理するため、指定時は JSON では
            // なく予算調整済みテキストを返す
            if args_val
                .get("estimate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                a.estimate = true;
                // analyze の既定出力は JSON（このブロックの else 節）なので、
                // 見積もりも JSON 経路の実サイズで測る。ここを立てないと
                // テキスト経路で測ってしまい、JSON は装飾が多い分実際より
                // 大幅に軽く出て --budget の判断材料として役に立たない
                a.json = true;
            } else if let Some(b) = args_val.get("budget").and_then(|v| v.as_i64()) {
                a.budget = Some(b);
            } else {
                a.json = true;
            }
        }
        "tree" => {
            a.gitignore = true;
            a.budget = args_val.get("budget").and_then(|v| v.as_i64());
            if let Some(n) = args_val.get("top").and_then(|v| v.as_u64()) {
                a.top = Some(n as usize);
            }
        }
        "outline" => {
            let files: Vec<String> = args_val
                .get("files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if files.is_empty() {
                // files 省略時はプロジェクト全体の公開API（-A 相当）。
                // -A は全ツリーを歩く方式で depth 未指定だと肥大化しうるため
                // （実測: このリポジトリで 145,016 文字・MCP応答上限超過）、
                // 呼び出し側が depth を指定しなければ 2 を既定にして安全側に倒す
                a.api = true;
                a.gitignore = true;
                a.json = true;
                if a.depth.is_none() && !unlimited_depth {
                    a.depth = Some(2);
                }
            } else {
                // 相対パスは path 基準に解決する（サーバーの cwd はホスト依存のため）
                let base = std::path::Path::new(&a.path);
                let files = files
                    .into_iter()
                    .map(|f| {
                        if a.path != "." && std::path::Path::new(&f).is_relative() {
                            base.join(&f).to_string_lossy().into_owned()
                        } else {
                            f
                        }
                    })
                    .collect();
                a.stdin_files = Some(files);
                a.json = true;
                a.git = true;
            }
        }
        "imports" => {
            a.imports = true;
            a.gitignore = true;
            match args_val.get("format").and_then(|v| v.as_str()) {
                None | Some("json") => a.json = true,
                Some("mermaid") => a.mermaid = true,
                Some("dot") => a.dot = true,
                Some(other) => {
                    return (
                        format!("error: unknown format '{}' (expected json, mermaid, or dot)", other),
                        true,
                    )
                }
            }
        }
        "focus" => {
            let Some(file) = args_val.get("file").and_then(|v| v.as_str()) else {
                return ("error: 'file' is required".to_string(), true);
            };
            a.focus = Some(file.to_string());
            a.gitignore = true;
            a.json = true;
        }
        "todos" => {
            a.todo = true;
            a.gitignore = true;
            a.json = true;
        }
        "since" => {
            let git_ref = args_val
                .get("ref")
                .and_then(|v| v.as_str())
                .unwrap_or("HEAD");
            a.since = Some(git_ref.to_string());
            a.gitignore = true;
            a.json = true;
            // 変更ファイルにはトークン・アウトライン・TODO を注釈する
            // （`git diff --name-only | dirlens --stdin --json` と同じ粒度）
            a.tokens = true;
            a.outline = true;
            a.todo = true;
        }
        "history" => {
            a.git = true;
            a.gitignore = true;
            // ファイル毎の git 注釈で JSON は肥大化するためテキスト固定。
            // 深さも既定 1 に抑える（ホットスポット一覧は深さに依らず全体を反映）。
            // unlimited_depth:true が明示されれば無制限（None）のまま通す
            if a.depth.is_none() && !unlimited_depth {
                a.depth = Some(1);
            }
        }
        "api_diff" => {
            let Some(git_ref) = args_val.get("ref").and_then(|v| v.as_str()) else {
                return ("error: 'ref' is required".to_string(), true);
            };
            a.api_diff = Some(git_ref.to_string());
            a.gitignore = true;
        }
        _ => return (format!("error: unknown tool '{}'", name), true),
    }

    // todos は常に、imports は json フォーマット時のみフラット化する
    // （mermaid/dot は図として全体のグラフが要るので素通し）
    let flatten = match name {
        "todos" => true,
        "imports" => a.json,
        _ => false,
    };

    let res = run(a, &StdFs, &StdGit, &NoClipboard, false);
    if res.exit_code != 0 {
        (
            if res.stderr.is_empty() { res.stdout } else { res.stderr },
            true,
        )
    } else if flatten {
        match serde_json::from_str::<Value>(&res.stdout) {
            Ok(root) => {
                let flat = if name == "todos" {
                    flatten_todos_json(&root, limit)
                } else {
                    flatten_imports_json(&root, limit)
                };
                let mut s = serde_json::to_string_pretty(&flat).unwrap_or(res.stdout);
                s.push('\n');
                (s, false)
            }
            // 予期しない解析失敗時は元の出力をそのまま返す（フォールバック）
            Err(_) => (res.stdout, false),
        }
    } else {
        (res.stdout, false)
    }
}

/// build_json_tree が出す再帰ツリーから "type": "file" のノードだけを集める。
fn walk_files<'a>(node: &'a Value, out: &mut Vec<&'a Map<String, Value>>) {
    let Some(obj) = node.as_object() else { return };
    if obj.get("type").and_then(|v| v.as_str()) == Some("file") {
        out.push(obj);
        return;
    }
    if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
        for c in children {
            walk_files(c, out);
        }
    }
}

/// `todos` ツール向け: 全ツリーの `"todos": []` 注釈を、該当ファイルだけの
/// フラットな配列に潰す（TODO が無いファイル分の空配列を送らない）。
fn flatten_todos_json(root: &Value, limit: Option<usize>) -> Value {
    let mut files = Vec::new();
    walk_files(root, &mut files);

    let mut items: Vec<Value> = Vec::new();
    for f in files {
        let Some(todos) = f.get("todos").and_then(|v| v.as_array()) else {
            continue;
        };
        let path = f.get("path").cloned().unwrap_or_else(|| json!(""));
        for t in todos {
            let mut m = Map::new();
            m.insert("path".into(), path.clone());
            if let Some(tobj) = t.as_object() {
                for (k, v) in tobj {
                    m.insert(k.clone(), v.clone());
                }
            }
            items.push(Value::Object(m));
        }
    }
    let total = items.len();
    if let Some(n) = limit {
        items.truncate(n);
    }

    let todo_count = root
        .get("project_summary")
        .and_then(|p| p.get("todo_count"))
        .cloned()
        .unwrap_or_else(|| json!(total));
    let errors = root.get("errors").cloned().unwrap_or_else(|| json!([]));

    let mut out = Map::new();
    out.insert(
        "schema_version".into(),
        root.get("schema_version").cloned().unwrap_or(json!(1)),
    );
    out.insert("todo_count".into(), todo_count);
    out.insert("todos".into(), Value::Array(items));
    out.insert("errors".into(), errors);
    Value::Object(out)
}

/// `imports` ツール向け（json フォーマット時）: 全ツリーの
/// `imports`/`imported_by`/`external_imports` 注釈を、いずれか非空のファイルだけの
/// フラットな配列に潰す（import 関係が一切無いファイル分の空配列を送らない）。
fn flatten_imports_json(root: &Value, limit: Option<usize>) -> Value {
    let mut files = Vec::new();
    walk_files(root, &mut files);

    let has_content =
        |v: Option<&Value>| v.and_then(|x| x.as_array()).map(|a| !a.is_empty()).unwrap_or(false);

    let mut items: Vec<Value> = Vec::new();
    for f in files {
        let imports = f.get("imports");
        let imported_by = f.get("imported_by");
        let external = f.get("external_imports");
        if has_content(imports) || has_content(imported_by) || has_content(external) {
            let mut m = Map::new();
            m.insert("path".into(), f.get("path").cloned().unwrap_or_else(|| json!("")));
            m.insert("imports".into(), imports.cloned().unwrap_or_else(|| json!([])));
            m.insert(
                "imported_by".into(),
                imported_by.cloned().unwrap_or_else(|| json!([])),
            );
            m.insert(
                "external_imports".into(),
                external.cloned().unwrap_or_else(|| json!([])),
            );
            items.push(Value::Object(m));
        }
    }
    if let Some(n) = limit {
        items.truncate(n);
    }

    let summary = root.get("project_summary");
    let most_depended = summary
        .and_then(|s| s.get("most_depended_on"))
        .cloned()
        .unwrap_or(Value::Null);
    let cycles = summary
        .and_then(|s| s.get("circular_dependencies"))
        .cloned()
        .unwrap_or(Value::Null);
    let errors = root.get("errors").cloned().unwrap_or_else(|| json!([]));

    let mut out = Map::new();
    out.insert(
        "schema_version".into(),
        root.get("schema_version").cloned().unwrap_or(json!(1)),
    );
    out.insert("most_depended_on".into(), most_depended);
    out.insert("circular_dependencies".into(), cycles);
    out.insert("files".into(), Value::Array(items));
    out.insert("errors".into(), errors);
    Value::Object(out)
}

fn respond(out: &mut impl Write, id: Value, result: Value) {
    let msg = json!({"jsonrpc": "2.0", "id": id, "result": result});
    let _ = writeln!(out, "{}", msg);
    let _ = out.flush();
}

fn respond_err(out: &mut impl Write, id: Value, code: i64, message: &str) {
    let msg = json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}});
    let _ = writeln!(out, "{}", msg);
    let _ = out.flush();
}

/// --mcp-setup: 各 MCP ホスト向けの設定手順を、実行中バイナリの絶対パスを
/// 埋め込んで表示する。GUI ホストはシェル PATH を継がないことが多いため、
/// "command" には常に絶対パスを使う。
pub fn print_setup(host: &str, ja: bool) {
    let tr = |en: &'static str, ja_s: &'static str| if ja { ja_s } else { en };
    // npm ラッパー経由でも current_exe はネイティブバイナリ本体を指す
    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "dirlens".to_string());
    let json_block = serde_json::to_string_pretty(&json!({
        "mcpServers": {
            "dirlens": { "command": exe, "args": ["--mcp"] }
        }
    }))
    .unwrap_or_default();

    println!("{}", tr("dirlens MCP setup", "dirlens MCP セットアップ"));
    println!();
    println!("{}: {}", tr("binary", "バイナリ"), exe);
    println!();

    if host == "all" || host == "claude-code" {
        println!("── Claude Code ──────────────────────────────────");
        println!(
            "  {}",
            tr(
                "run this one-liner in your terminal:",
                "ターミナルで次の1行を実行:"
            )
        );
        println!();
        println!("  claude mcp add dirlens -s user -- \"{}\" --mcp", exe);
        println!();
    }
    if host == "all" || host == "claude-desktop" {
        println!("── Claude Desktop ───────────────────────────────");
        println!(
            "  {}",
            tr(
                "add the JSON below to claude_desktop_config.json:",
                "下の JSON を claude_desktop_config.json に追記:"
            )
        );
        println!("    macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json");
        println!("    Windows: %APPDATA%\\Claude\\claude_desktop_config.json");
        println!("    Linux:   ~/.config/Claude/claude_desktop_config.json");
        println!();
    }
    if host == "all" || host == "cursor" {
        println!("── Cursor ───────────────────────────────────────");
        println!(
            "  {}",
            tr(
                "add the JSON below to ~/.cursor/mcp.json (global) or .cursor/mcp.json (per project):",
                "下の JSON を ~/.cursor/mcp.json（全体）か .cursor/mcp.json（プロジェクト単位）に追記:"
            )
        );
        println!(
            "  {}",
            tr(
                "note: the path below is specific to this machine. If .cursor/mcp.json is committed and shared with teammates, each teammate must substitute their own absolute path.",
                "注意: 下のパスはこのマシン固有。.cursor/mcp.json をコミットしてチームで共有する場合、各メンバーが自分の絶対パスに置き換える必要がある。"
            )
        );
        println!();
    }
    if host != "claude-code" {
        for line in json_block.lines() {
            println!("  {}", line);
        }
        println!();
    }
    println!(
        "{}: analyze, tree, outline, imports, focus, todos, since, history, api_diff",
        tr("tools provided", "提供ツール")
    );
    println!(
        "{}",
        tr(
            "(restart the host app after editing its config)",
            "（設定ファイルを編集した場合はホストアプリを再起動してください）"
        )
    );
}

/// stdio で MCP サーバーを起動する（stdin が閉じるまでブロック）。
pub fn serve() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            respond_err(&mut stdout, Value::Null, -32700, "parse error");
            continue;
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        match method {
            "initialize" => respond(
                &mut stdout,
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "dirlens", "version": env!("CARGO_PKG_VERSION")}
                }),
            ),
            "notifications/initialized" | "notifications/cancelled" => {} // 通知には応答しない
            "ping" => respond(&mut stdout, id, json!({})),
            "tools/list" => respond(&mut stdout, id, json!({"tools": tool_defs()})),
            "tools/call" => {
                let params = msg.get("params").and_then(|p| p.as_object());
                let name = params
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let empty = Map::new();
                let arguments = params
                    .and_then(|p| p.get("arguments"))
                    .and_then(|a| a.as_object())
                    .unwrap_or(&empty);
                let (text, is_error) = run_tool(name, arguments);
                respond(
                    &mut stdout,
                    id,
                    json!({
                        "content": [{"type": "text", "text": text}],
                        "isError": is_error
                    }),
                );
            }
            _ => {
                if !id.is_null() {
                    respond_err(&mut stdout, id, -32601, "method not found");
                }
            }
        }
    }
}
