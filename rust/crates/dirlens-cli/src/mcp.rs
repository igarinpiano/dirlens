//! MCP（Model Context Protocol）サーバーモード（--mcp）。
//!
//! stdio 上の改行区切り JSON-RPC 2.0。エージェントホスト（Claude Code / Cursor 等）に
//! dirlens の解析をネイティブツールとして公開する。外部依存なしの手書き実装
//! （必要なのは initialize / tools/list / tools/call / ping のみ）。
//!
//! 登録例（Claude Code）: `claude mcp add dirlens -- dirlens --mcp`

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
            "description": "Full project analysis (tree + tokens + git + TODOs + missing tests + entry points + outline + import graph + config files) as JSON. Equivalent to `dirlens --agent --json`. Best first call to understand a project.",
            "inputSchema": obj(json!({"path": path_prop, "depth": depth_prop}), vec![])
        },
        {
            "name": "tree",
            "description": "Plain directory tree with sizes (gitignore applied), as text.",
            "inputSchema": obj(json!({"path": path_prop, "depth": depth_prop}), vec![])
        },
        {
            "name": "outline",
            "description": "Function/class outline, token count, and TODOs for specific files (AST-based; Python/JS/TS/Rust/Go/C/Java/Ruby/PHP/C#/Kotlin/Swift).",
            "inputSchema": obj(json!({"files": {"type": "array", "items": {"type": "string"}, "description": "file paths to analyze"}}), vec!["files"])
        },
        {
            "name": "imports",
            "description": "Local import/dependency graph with most-depended-on files and circular dependencies, as JSON.",
            "inputSchema": obj(json!({"path": path_prop}), vec![])
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
            a.json = true;
        }
        "tree" => {
            a.gitignore = true;
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
                return ("error: 'files' must be a non-empty array".to_string(), true);
            }
            a.stdin_files = Some(files);
            a.json = true;
            a.git = true;
        }
        "imports" => {
            a.imports = true;
            a.gitignore = true;
            a.json = true;
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
