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
    let depth_prop = json!({"type": "integer", "description": "max tree depth (aggregates still cover the whole project)"});
    json!([
        {
            "name": "analyze",
            "description": "Full project analysis (tree + tokens + git + TODOs + missing tests + entry points + outline + import graph + config files) as JSON. Equivalent to `dirlens --agent --json`. Best first call to understand a project. On large projects the JSON can be huge: pass `budget` to get compact annotated text fitted to a token budget instead, or `estimate: true` first to see the cost per depth level.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "depth": depth_prop,
                "budget": {"type": "integer", "description": "fit output to about this many tokens (o200k BPE). Returns compact annotated text instead of JSON, trimming depth, then annotations, then tree rows"},
                "estimate": {"type": "boolean", "description": "return a few-line token-cost estimate per depth level instead of the analysis (use it to pick a budget). Overrides budget"}
            }), vec![])
        },
        {
            "name": "tree",
            "description": "Plain directory tree with sizes (gitignore applied), as text.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "depth": depth_prop,
                "budget": {"type": "integer", "description": "fit output to about this many tokens (o200k BPE) by trimming depth, then tree rows"},
                "top": {"type": "integer", "description": "if set, return a flat list of the N largest files and directories instead of a tree"}
            }), vec![])
        },
        {
            "name": "outline",
            "description": "Function/class outline, token count, and TODOs for specific files (AST-based; Python/JS/TS/Rust/Go/C/Java/Ruby/PHP/C#/Kotlin/Swift), as JSON. Accepts multiple files at once. If `files` is omitted, returns the project-wide public API instead (public symbols only, like `dirlens -A`).",
            "inputSchema": obj(json!({
                "files": {"type": "array", "items": {"type": "string"}, "description": "file paths to analyze (resolved against `path` when relative). Omit for a project-wide public API outline"},
                "path": path_prop,
                "depth": depth_prop
            }), vec![])
        },
        {
            "name": "imports",
            "description": "Local import/dependency graph with most-depended-on files and circular dependencies. JSON by default; set `format` to get a Mermaid or Graphviz DOT diagram.",
            "inputSchema": obj(json!({
                "path": path_prop,
                "format": {"type": "string", "enum": ["json", "mermaid", "dot"], "description": "output format (default: json)"}
            }), vec![])
        },
        {
            "name": "focus",
            "description": "Impact analysis for one file: what it imports and what imports it (direct + transitive), as JSON. Use before refactoring to see the blast radius.",
            "inputSchema": obj(json!({"path": path_prop, "file": {"type": "string", "description": "project-relative source file"}}), vec!["file"])
        },
        {
            "name": "todos",
            "description": "TODO/FIXME/HACK/XXX comments across the project with file/line, as JSON.",
            "inputSchema": obj(json!({"path": path_prop}), vec![])
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
            "description": "Recent git activity as compact text: tree annotated with each file's last commit, plus frequently-changed hotspot files. Depth defaults to 1 to stay small.",
            "inputSchema": obj(json!({"path": path_prop, "depth": depth_prop}), vec![])
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

    let mut a = Args {
        path,
        depth,
        lang: Some("en".to_string()),
        ..Default::default()
    };
    match name {
        "analyze" => {
            a.agent = true;
            // --budget / --estimate はコアがテキスト経路でのみ処理するため、
            // 指定時は JSON ではなく予算調整済みテキスト / 見積もりテキストを返す
            if args_val
                .get("estimate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                a.estimate = true;
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
                // files 省略時はプロジェクト全体の公開API（-A 相当）
                a.api = true;
                a.gitignore = true;
                a.json = true;
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
            // 深さも既定 1 に抑える（ホットスポット一覧は深さに依らず全体を反映）
            if a.depth.is_none() {
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

    let res = run(a, &StdFs, &StdGit, &NoClipboard, false);
    if res.exit_code != 0 {
        (
            if res.stderr.is_empty() { res.stdout } else { res.stderr },
            true,
        )
    } else {
        (res.stdout, false)
    }
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
