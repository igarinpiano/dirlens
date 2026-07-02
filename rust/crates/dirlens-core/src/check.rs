//! `--check`: 人間向け能力レポート（精度の可視化・spec 機能5）。
//!
//! このビルド・この環境・この対象ディレクトリで dirlens がどの解析方式を
//! 使えるかを報告する。終了コード: 最良 = 0、縮退あり = 1。

use serde_json::{json, Map, Value};

use crate::analysis::ast::CAPABILITIES;
use crate::cfg::Cfg;

pub struct EnvProbe {
    pub git_available: bool,
    pub is_work_tree: bool,
    pub clipboard: bool,
}

fn lang_method(ast: bool, enhanced: bool) -> &'static str {
    if ast && enhanced {
        "ast"
    } else {
        "regex"
    }
}

/// capabilities メタブロック（--agent --json でも再利用）。
pub fn capabilities_json(cfg: &Cfg, probe: &EnvProbe) -> Value {
    let e = cfg.enhanced_analysis;
    let mut outline = Map::new();
    outline.insert("python".into(), json!(lang_method(CAPABILITIES.python, e)));
    outline.insert("js_ts".into(), json!(lang_method(CAPABILITIES.js_ts, e)));
    outline.insert("rust".into(), json!(lang_method(CAPABILITIES.rust, e)));
    outline.insert("go".into(), json!(lang_method(CAPABILITIES.go, e)));
    outline.insert(
        "c".into(),
        if CAPABILITIES.c && e {
            json!("ast")
        } else {
            json!("unsupported")
        },
    );
    outline.insert("fallback".into(), json!("regex"));

    let mut resolution = vec!["relative"];
    if e {
        resolution.extend(["tsconfig-paths", "package-imports", "go-module", "rust-module-tree"]);
    } else {
        resolution.extend(["go-module"]);
    }

    let gitignore_tier = cfg
        .gitignore_tier
        .unwrap_or(if probe.git_available && probe.is_work_tree && cfg.gitignore_prefer_git {
            "git"
        } else {
            "builtin"
        });

    let mut m = Map::new();
    m.insert("gitignore_tier".into(), json!(gitignore_tier));
    m.insert("outline".into(), Value::Object(outline));
    m.insert("imports_resolution".into(), json!(resolution));
    m.insert("git_log".into(), json!(probe.git_available));
    m.insert("clipboard".into(), json!(probe.clipboard));
    m.insert("tokens".into(), json!(tokens_mode(cfg)));
    Value::Object(m)
}

/// この実行で使うトークン計数方式。
pub fn tokens_mode(cfg: &Cfg) -> &'static str {
    if cfg.tokens_bpe && crate::analysis::text_metrics::bpe_available() {
        "bpe-o200k_base"
    } else {
        "char-heuristic"
    }
}

/// --agent テキスト末尾の精度注記（1〜2 行）。
pub fn agent_note(cfg: &Cfg) -> String {
    let e = cfg.enhanced_analysis;
    let gitignore = match cfg.gitignore_tier {
        Some("git") => "git check-ignore(厳密)",
        Some(_) => "内蔵マッチャ(fnmatch近似)",
        None => "未使用",
    };
    let mut ast_langs: Vec<&str> = Vec::new();
    if e {
        if CAPABILITIES.python {
            ast_langs.push("py");
        }
        if CAPABILITIES.js_ts {
            ast_langs.push("js/ts");
        }
        if CAPABILITIES.rust {
            ast_langs.push("rs");
        }
        if CAPABILITIES.go {
            ast_langs.push("go");
        }
        if CAPABILITIES.c {
            ast_langs.push("c");
        }
    }
    let outline = if ast_langs.is_empty() {
        "正規表現のみ".to_string()
    } else {
        format!("AST:{}(他は正規表現)", ast_langs.join(","))
    };
    let imports = if e {
        "AST+マニフェスト解決"
    } else {
        "正規表現+相対パス解決"
    };
    let tokens = if tokens_mode(cfg) == "bpe-o200k_base" {
        "BPE(o200k)"
    } else {
        "文字数概算"
    };
    format!(
        "  解析方式: gitignore={} / outline={} / imports={} / tokens={}",
        gitignore, outline, imports, tokens
    )
}

/// --check の出力を組み立てる。戻り値: (stdout, exit_code)
pub fn render_check(cfg: &Cfg, probe: &EnvProbe, as_json: bool) -> (String, i32) {
    let e = cfg.enhanced_analysis;
    let mut degraded: Vec<String> = Vec::new();
    if !probe.git_available {
        degraded.push("git が見つからない（-H 不可・gitignore は内蔵マッチャ）".into());
    } else if !probe.is_work_tree {
        degraded.push("対象が git work tree ではない（gitignore は内蔵マッチャ）".into());
    }
    if !e {
        degraded.push("AST 解析が無効（正規表現のみ）".into());
    }
    if !CAPABILITIES.go {
        degraded.push("tree-sitter-go 未同梱（Go は正規表現）".into());
    }
    if !CAPABILITIES.c {
        degraded.push("tree-sitter-c 未同梱（C は未対応）".into());
    }
    if !probe.clipboard {
        degraded.push("クリップボードツールが見つからない（-C 不可）".into());
    }
    if tokens_mode(cfg) != "bpe-o200k_base" {
        degraded.push("BPE トークナイザ未使用（-T は文字数概算）".into());
    }
    let exit = if degraded.is_empty() { 0 } else { 1 };

    if as_json {
        let mut m = Map::new();
        m.insert("schema_version".into(), json!(crate::render_json::SCHEMA_VERSION));
        m.insert("capabilities".into(), capabilities_json(cfg, probe));
        m.insert("degraded".into(), json!(degraded));
        m.insert("best".into(), json!(exit == 0));
        let mut s = serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default();
        s.push('\n');
        return (s, exit);
    }

    let onoff = |b: bool| if b { "✓" } else { "✗" };
    let mut out = String::new();
    out.push_str("dirlens 能力レポート\n");
    out.push_str(&format!(
        "  gitignore (-G): {}\n",
        if probe.git_available && probe.is_work_tree {
            "git check-ignore（厳密・ネスト/否定/グローバル除外に完全対応）"
        } else {
            "内蔵マッチャ（fnmatch 近似・基本パターンのみ）"
        }
    ));
    out.push_str(&format!(
        "  git 履歴 (-H): {} git\n",
        onoff(probe.git_available)
    ));
    out.push_str("  アウトライン (-O/-A):\n");
    out.push_str(&format!(
        "    Python: {} / JS・TS: {} / Rust: {} / Go: {} / C: {}\n",
        lang_method(CAPABILITIES.python, e),
        lang_method(CAPABILITIES.js_ts, e),
        lang_method(CAPABILITIES.rust, e),
        lang_method(CAPABILITIES.go, e),
        if CAPABILITIES.c && e { "ast" } else { "未対応" },
    ));
    out.push_str(&format!(
        "  import 解決 (-M): {}\n",
        if e {
            "AST + マニフェスト（tsconfig paths / package.json imports / go.mod / Rust モジュールツリー）"
        } else {
            "正規表現 + 相対パス解決"
        }
    ));
    out.push_str(&format!(
        "  トークン計数 (-T): {}\n",
        if tokens_mode(cfg) == "bpe-o200k_base" {
            "BPE（o200k_base）による正確値（5MB 超は比例概算）"
        } else {
            "文字数ベースの概算"
        }
    ));
    out.push_str(&format!(
        "  クリップボード (-C): {}\n",
        onoff(probe.clipboard)
    ));
    if degraded.is_empty() {
        out.push_str("\nすべての機能が最良の方式で動作します。\n");
    } else {
        out.push_str("\n縮退している項目:\n");
        for d in &degraded {
            out.push_str(&format!("  - {}\n", d));
        }
    }
    (out, exit)
}
