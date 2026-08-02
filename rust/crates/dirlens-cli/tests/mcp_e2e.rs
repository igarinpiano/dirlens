//! `dirlens --mcp` を実プロセスとして起動し、stdio 越しの改行区切り JSON-RPC を
//! 実際にやり取りするエンドツーエンドテスト。
//!
//! `src/mcp.rs` 内のユニットテスト（`run_tool()` を直接呼ぶだけ）は tools/call の
//! 中身のロジックは検証できるが、実バイナリの起動・引数解釈から `--mcp` への
//! ディスパッチ・stdin/stdout のフレーミング（1行=1メッセージ・flush）までは
//! 通らない。ここではエージェントホスト（Claude Code 等）と同じやり方——実プロセスを
//! spawn し、stdin に書いて stdout から1行読む——で initialize / tools/list /
//! tools/call / 未知メソッドを検証する。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use serde_json::{json, Value};

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl McpProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dirlens"))
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn `dirlens --mcp`");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self { child, stdin, stdout }
    }

    /// 1件リクエストを送り、対応する1行の JSON-RPC 応答を読んで返す。
    fn call(&mut self, id: i64, method: &str, params: Value) -> Value {
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{req}").expect("write request to dirlens --mcp stdin");
        self.stdin.flush().expect("flush dirlens --mcp stdin");

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("read response from dirlens --mcp stdout");
        assert!(!line.trim().is_empty(), "dirlens --mcp exited without responding to {method}");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("invalid JSON-RPC response to {method}: {e}\nline: {line}"))
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn initialize_and_tools_list_over_real_stdio() {
    let mut proc = McpProcess::spawn();

    let resp = proc.call(1, "initialize", json!({}));
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["result"]["serverInfo"]["name"], "dirlens");
    assert!(resp["result"]["protocolVersion"].is_string());

    let resp = proc.call(2, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().expect("tools/list result.tools must be an array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "analyze", "tree", "outline", "imports", "focus", "todos", "since", "history", "api_diff",
    ] {
        assert!(names.contains(&expected), "tools/list is missing '{expected}': {names:?}");
    }
}

#[test]
fn tools_call_analyze_over_real_stdio_returns_valid_json() {
    let mut proc = McpProcess::spawn();
    proc.call(1, "initialize", json!({}));

    let resp = proc.call(
        2,
        "tools/call",
        json!({"name": "analyze", "arguments": {"path": env!("CARGO_MANIFEST_DIR"), "depth": 1}}),
    );
    let result = &resp["result"];
    assert_eq!(result["isError"], json!(false), "analyze tool call reported an error: {result}");

    let text = result["content"][0]["text"].as_str().expect("content[0].text must be a string");
    let parsed: Value = serde_json::from_str(text).expect("analyze tool output must be valid JSON");
    assert!(parsed.get("project_summary").is_some(), "expected project_summary in analyze output");
}

#[test]
fn ping_and_unknown_method_over_real_stdio() {
    let mut proc = McpProcess::spawn();

    let resp = proc.call(1, "ping", json!({}));
    assert_eq!(resp["result"], json!({}));

    let resp = proc.call(2, "totally/unknown/method", json!({}));
    assert_eq!(resp["error"]["code"], json!(-32601));
}
