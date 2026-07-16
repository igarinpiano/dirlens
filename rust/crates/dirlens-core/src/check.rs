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
    let ast_or_unsupported = |on: bool| if on && e { json!("ast") } else { json!("unsupported") };
    outline.insert("c".into(), ast_or_unsupported(CAPABILITIES.c));
    outline.insert("java".into(), ast_or_unsupported(CAPABILITIES.java));
    outline.insert("ruby".into(), ast_or_unsupported(CAPABILITIES.ruby));
    outline.insert("php".into(), ast_or_unsupported(CAPABILITIES.php));
    outline.insert("csharp".into(), ast_or_unsupported(CAPABILITIES.csharp));
    outline.insert("kotlin".into(), ast_or_unsupported(CAPABILITIES.kotlin));
    outline.insert("swift".into(), ast_or_unsupported(CAPABILITIES.swift));
    // HTML はインライン <script> の JS を抽出する（正規表現の縮退先なし）
    outline.insert(
        "html".into(),
        if CAPABILITIES.js_ts && e {
            json!("ast (embedded js)")
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
    let t = cfg.lang.t();
    let gitignore = match cfg.gitignore_tier {
        Some("git") => t.note_gitignore_git,
        Some(_) => t.note_gitignore_builtin,
        None => t.note_gitignore_unused,
    };
    let mut ast_langs: Vec<&str> = Vec::new();
    if e {
        if CAPABILITIES.python {
            ast_langs.push("py");
        }
        if CAPABILITIES.js_ts {
            ast_langs.push("js/ts");
            ast_langs.push("html(embedded js)");
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
        if CAPABILITIES.java {
            ast_langs.push("java");
        }
        if CAPABILITIES.ruby {
            ast_langs.push("rb");
        }
        if CAPABILITIES.php {
            ast_langs.push("php");
        }
        if CAPABILITIES.csharp {
            ast_langs.push("cs");
        }
        if CAPABILITIES.kotlin {
            ast_langs.push("kt");
        }
        if CAPABILITIES.swift {
            ast_langs.push("swift");
        }
    }
    let outline = if ast_langs.is_empty() {
        t.note_outline_regex_only.to_string()
    } else {
        match cfg.lang {
            crate::i18n::Lang::Ja => format!("AST:{}(他は正規表現)", ast_langs.join(",")),
            crate::i18n::Lang::En => format!("AST:{} (regex otherwise)", ast_langs.join(",")),
        }
    };
    let imports = if e { t.note_imports_ast } else { t.note_imports_regex };
    let tokens = if tokens_mode(cfg) == "bpe-o200k_base" {
        t.note_tokens_bpe
    } else {
        t.note_tokens_char
    };
    match cfg.lang {
        crate::i18n::Lang::Ja => format!(
            "  解析方式: gitignore={} / outline={} / imports={} / tokens={} / ディレクトリsize=ディスク生値（gitignore非適用）",
            gitignore, outline, imports, tokens
        ),
        crate::i18n::Lang::En => format!(
            "  Analysis methods: gitignore={} / outline={} / imports={} / tokens={} / dir sizes=raw disk (gitignore not applied)",
            gitignore, outline, imports, tokens
        ),
    }
}

/// --check の出力を組み立てる。戻り値: (stdout, exit_code)
pub fn render_check(cfg: &Cfg, probe: &EnvProbe, as_json: bool) -> (String, i32) {
    let e = cfg.enhanced_analysis;
    let t = cfg.lang.t();
    let mut degraded: Vec<String> = Vec::new();
    if !probe.git_available {
        degraded.push(t.deg_no_git.into());
    } else if !probe.is_work_tree {
        degraded.push(t.deg_not_worktree.into());
    }
    if !e {
        degraded.push(t.deg_no_ast.into());
    }
    if !CAPABILITIES.go {
        degraded.push(t.deg_no_ts_go.into());
    }
    if !CAPABILITIES.c {
        degraded.push(t.deg_no_ts_c.into());
    }
    if !probe.clipboard {
        degraded.push(t.deg_no_clipboard.into());
    }
    if tokens_mode(cfg) != "bpe-o200k_base" {
        degraded.push(t.deg_no_bpe.into());
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
    let is_ja = cfg.lang == crate::i18n::Lang::Ja;
    let mut out = String::new();
    out.push_str(t.check_title);
    out.push('\n');
    out.push_str(&format!(
        "  gitignore (-G): {}\n",
        if probe.git_available && probe.is_work_tree {
            t.check_gitignore_git
        } else {
            t.check_gitignore_builtin
        }
    ));
    out.push_str(&format!(
        "  {} (-H): {} git\n",
        if is_ja { "git 履歴" } else { "git history" },
        onoff(probe.git_available)
    ));
    out.push_str(t.check_outline_label);
    out.push('\n');
    out.push_str(&format!(
        "    Python: {} / JS・TS: {} / Rust: {} / Go: {} / C: {}\n",
        lang_method(CAPABILITIES.python, e),
        lang_method(CAPABILITIES.js_ts, e),
        lang_method(CAPABILITIES.rust, e),
        lang_method(CAPABILITIES.go, e),
        if CAPABILITIES.c && e { "ast" } else { t.check_unsupported },
    ));
    let aou = |on: bool| if on && e { "ast" } else { t.check_unsupported };
    out.push_str(&format!(
        "    Java: {} / Ruby: {} / PHP: {} / C#: {} / Kotlin: {} / Swift: {}\n",
        aou(CAPABILITIES.java),
        aou(CAPABILITIES.ruby),
        aou(CAPABILITIES.php),
        aou(CAPABILITIES.csharp),
        aou(CAPABILITIES.kotlin),
        aou(CAPABILITIES.swift),
    ));
    out.push_str(&format!(
        "  {} (-M): {}\n",
        if is_ja { "import 解決" } else { "import resolution" },
        if e { t.check_imports_ast } else { t.check_imports_regex }
    ));
    out.push_str(&format!(
        "  {} (-T): {}\n",
        if is_ja { "トークン計数" } else { "token counting" },
        if tokens_mode(cfg) == "bpe-o200k_base" {
            t.check_tokens_bpe
        } else {
            t.check_tokens_heuristic
        }
    ));
    out.push_str(&format!(
        "  {} (-C): {}\n",
        if is_ja { "クリップボード" } else { "clipboard" },
        onoff(probe.clipboard)
    ));
    if degraded.is_empty() {
        out.push_str(t.check_all_best);
    } else {
        out.push_str(t.check_degraded_header);
        for d in &degraded {
            out.push_str(&format!("  - {}\n", d));
        }
    }
    (out, exit)
}
