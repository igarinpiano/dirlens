//! 出力言語（i18n）。英語がデフォルト、`--lang ja` / 設定ファイル /
//! `DIRLENS_LANG` で日本語に切り替える。`DIRLENS_COMPAT=python` は
//! 旧 Python 版とのバイト一致検証のため常に日本語（Ja）を強制する。
//!
//! 方針: 翻訳キーの動的ルックアップはせず、静的文字列は `Texts` 構造体、
//! 数値等を埋め込む文は関数で提供する（ゼロコスト・タイポはコンパイルエラー）。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Ja,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Lang> {
        match s.to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Lang::En),
            "ja" | "jp" | "japanese" => Some(Lang::Ja),
            _ => None,
        }
    }

    pub fn t(self) -> &'static Texts {
        match self {
            Lang::En => &EN,
            Lang::Ja => &JA,
        }
    }
}

/// 静的な UI 文字列のカタログ。
pub struct Texts {
    // ツリー内マーク
    pub cyclic_link: &'static str,
    pub access_denied: &'static str,
    pub no_test: &'static str,

    // サマリー行の注記
    pub gitignore_applied: &'static str,
    pub pruned: &'static str,
    pub dirs_only: &'static str,

    // サマリーセクションの見出し
    pub most_depended: &'static str,
    pub hotspots: &'static str,
    pub reading_order: &'static str,
    pub no_git_info: &'static str,

    // run.rs のメッセージ
    pub copy_ok: &'static str,
    pub copy_fail: &'static str,
    pub err_cwd_denied: &'static str,

    // check.rs
    pub check_title: &'static str,
    pub check_gitignore_git: &'static str,
    pub check_gitignore_builtin: &'static str,
    pub check_outline_label: &'static str,
    pub check_unsupported: &'static str,
    pub check_imports_ast: &'static str,
    pub check_imports_regex: &'static str,
    pub check_tokens_bpe: &'static str,
    pub check_tokens_heuristic: &'static str,
    pub check_all_best: &'static str,
    pub check_degraded_header: &'static str,
    pub deg_no_git: &'static str,
    pub deg_not_worktree: &'static str,
    pub deg_no_ast: &'static str,
    pub deg_no_ts_go: &'static str,
    pub deg_no_ts_c: &'static str,
    pub deg_no_clipboard: &'static str,
    pub deg_no_bpe: &'static str,

    // agent_note
    pub note_gitignore_git: &'static str,
    pub note_gitignore_builtin: &'static str,
    pub note_gitignore_unused: &'static str,
    pub note_outline_regex_only: &'static str,
    pub note_imports_ast: &'static str,
    pub note_imports_regex: &'static str,
    pub note_tokens_bpe: &'static str,
    pub note_tokens_char: &'static str,
}

pub static EN: Texts = Texts {
    cyclic_link: "[cyclic link]",
    access_denied: "[access denied]",
    no_test: "no test",

    gitignore_applied: "(.gitignore applied)",
    pruned: "(pruned)",
    dirs_only: "(directories only)",

    most_depended: "  Most depended-on files (imported by many):",
    hotspots: "  Frequently changed files (recent history):",
    reading_order: "  Suggested reading order (entry points → most depended-on):",
    no_git_info: "  (no commit info: not a git repository, or git is not installed)",

    copy_ok: "✓ copied to clipboard",
    copy_fail: "✗ copy failed (requires pbcopy / xclip / wl-copy)",
    err_cwd_denied: "error: permission denied for the current directory.\n\
                     Specify an absolute path explicitly (e.g. dirlens /path/to/project).",

    check_title: "dirlens capability report",
    check_gitignore_git: "git check-ignore (exact: nesting / negation / global excludes fully supported)",
    check_gitignore_builtin: "builtin matcher (fnmatch approximation, basic patterns only)",
    check_outline_label: "  outline (-O/-A):",
    check_unsupported: "unsupported",
    check_imports_ast: "AST + manifests (tsconfig paths / package.json imports / go.mod / Rust module tree)",
    check_imports_regex: "regex + relative path resolution",
    check_tokens_bpe: "exact BPE (o200k_base); files over 5MB are extrapolated",
    check_tokens_heuristic: "character-count estimate",
    check_all_best: "\nAll features run with the best available method.\n",
    check_degraded_header: "\nDegraded items:\n",
    deg_no_git: "git not found (-H unavailable; gitignore uses builtin matcher)",
    deg_not_worktree: "target is not a git work tree (gitignore uses builtin matcher)",
    deg_no_ast: "AST analysis disabled (regex only)",
    deg_no_ts_go: "tree-sitter-go not bundled (Go falls back to regex)",
    deg_no_ts_c: "tree-sitter-c not bundled (C unsupported)",
    deg_no_clipboard: "no clipboard tool found (-C unavailable)",
    deg_no_bpe: "BPE tokenizer not in use (-T is a character estimate)",

    note_gitignore_git: "git check-ignore (exact)",
    note_gitignore_builtin: "builtin matcher (fnmatch approx)",
    note_gitignore_unused: "unused",
    note_outline_regex_only: "regex only",
    note_imports_ast: "AST+manifest resolution",
    note_imports_regex: "regex+relative paths",
    note_tokens_bpe: "BPE(o200k)",
    note_tokens_char: "char estimate",
};

pub static JA: Texts = Texts {
    cyclic_link: "[循環リンク]",
    access_denied: "[アクセス拒否]",
    no_test: "テスト無し",

    gitignore_applied: "(.gitignore 適用済み)",
    pruned: "(剪定済み)",
    dirs_only: "(ディレクトリのみ)",

    most_depended: "  依存度が高いファイル（多くのファイルから参照されている）:",
    hotspots: "  変更頻度が高いファイル（直近の履歴内）:",
    reading_order: "  読み始めの候補（エントリーポイント→依存度の高い順）:",
    no_git_info: "  (gitリポジトリではないか、git未インストールのためコミット情報は取得できませんでした)",

    copy_ok: "✓ クリップボードにコピーしました",
    copy_fail: "✗ コピー失敗 (pbcopy / xclip / wl-copy が必要)",
    err_cwd_denied: "エラー: 現在のディレクトリへのアクセス権限がありません。\n\
                     絶対パスを明示的に指定してください（例: dirlens /path/to/project）。",

    check_title: "dirlens 能力レポート",
    check_gitignore_git: "git check-ignore（厳密・ネスト/否定/グローバル除外に完全対応）",
    check_gitignore_builtin: "内蔵マッチャ（fnmatch 近似・基本パターンのみ）",
    check_outline_label: "  アウトライン (-O/-A):",
    check_unsupported: "未対応",
    check_imports_ast: "AST + マニフェスト（tsconfig paths / package.json imports / go.mod / Rust モジュールツリー）",
    check_imports_regex: "正規表現 + 相対パス解決",
    check_tokens_bpe: "BPE（o200k_base）による正確値（5MB 超は比例概算）",
    check_tokens_heuristic: "文字数ベースの概算",
    check_all_best: "\nすべての機能が最良の方式で動作します。\n",
    check_degraded_header: "\n縮退している項目:\n",
    deg_no_git: "git が見つからない（-H 不可・gitignore は内蔵マッチャ）",
    deg_not_worktree: "対象が git work tree ではない（gitignore は内蔵マッチャ）",
    deg_no_ast: "AST 解析が無効（正規表現のみ）",
    deg_no_ts_go: "tree-sitter-go 未同梱（Go は正規表現）",
    deg_no_ts_c: "tree-sitter-c 未同梱（C は未対応）",
    deg_no_clipboard: "クリップボードツールが見つからない（-C 不可）",
    deg_no_bpe: "BPE トークナイザ未使用（-T は文字数概算）",

    note_gitignore_git: "git check-ignore(厳密)",
    note_gitignore_builtin: "内蔵マッチャ(fnmatch近似)",
    note_gitignore_unused: "未使用",
    note_outline_regex_only: "正規表現のみ",
    note_imports_ast: "AST+マニフェスト解決",
    note_imports_regex: "正規表現+相対パス解決",
    note_tokens_bpe: "BPE(o200k)",
    note_tokens_char: "文字数概算",
};

// ─── パラメータつきメッセージ ─────────────────────────────────

/// 相対日時（fmt_date の本体）。sec は経過秒。
pub fn rel_date(lang: Lang, sec: i64) -> String {
    match lang {
        Lang::Ja => {
            if sec < 60 {
                return "今".to_string();
            }
            if sec < 3600 {
                return format!("{}分前", sec / 60);
            }
            if sec < 86400 {
                return format!("{}時間前", sec / 3600);
            }
            let d = sec / 86400;
            if d == 1 {
                return "昨日".to_string();
            }
            if d < 7 {
                return format!("{}日前", d);
            }
            if d < 30 {
                return format!("{}週間前", d / 7);
            }
            if d < 365 {
                return format!("{}ヶ月前", d / 30);
            }
            format!("{}年前", d / 365)
        }
        Lang::En => {
            let plural = |n: i64, unit: &str| {
                if n == 1 {
                    format!("1 {} ago", unit)
                } else {
                    format!("{} {}s ago", n, unit)
                }
            };
            if sec < 60 {
                return "now".to_string();
            }
            if sec < 3600 {
                return plural(sec / 60, "min");
            }
            if sec < 86400 {
                return plural(sec / 3600, "hour");
            }
            let d = sec / 86400;
            if d == 1 {
                return "yesterday".to_string();
            }
            if d < 7 {
                return plural(d, "day");
            }
            if d < 30 {
                return plural(d / 7, "week");
            }
            if d < 365 {
                return plural(d / 30, "month");
            }
            plural(d / 365, "year")
        }
    }
}

pub fn invalid_size(lang: Lang, s: &str) -> String {
    match lang {
        Lang::Ja => format!("無効なサイズ: '{}'（例: 50M, 1G, 500K）", s),
        Lang::En => format!("invalid size: '{}' (e.g. 50M, 1G, 500K)", s),
    }
}

pub fn err_not_found(lang: Lang, path: &str) -> String {
    match lang {
        Lang::Ja => format!("エラー: '{}' が見つかりません", path),
        Lang::En => format!("error: '{}' not found", path),
    }
}

pub fn err_not_dir(lang: Lang, path: &str) -> String {
    match lang {
        Lang::Ja => format!("エラー: '{}' はディレクトリではありません", path),
        Lang::En => format!("error: '{}' is not a directory", path),
    }
}

pub fn err_prefix(lang: Lang, msg: &str) -> String {
    match lang {
        Lang::Ja => format!("エラー: {}", msg),
        Lang::En => format!("error: {}", msg),
    }
}

pub fn html_generated(lang: Lang, path: &str, size: &str) -> String {
    match lang {
        Lang::Ja => format!("✓ {} を生成しました ({})", path, size),
        Lang::En => format!("✓ generated {} ({})", path, size),
    }
}

pub fn summary_total_dirs(lang: Lang, dirs: u64) -> String {
    match lang {
        Lang::Ja => format!("  合計  {} ディレクトリ", dirs),
        Lang::En => format!(
            "  Total  {} {}",
            dirs,
            if dirs == 1 { "directory" } else { "directories" }
        ),
    }
}

pub fn summary_files(lang: Lang, files: u64) -> String {
    match lang {
        Lang::Ja => format!(",  {} ファイル", files),
        Lang::En => format!(
            ",  {} {}",
            files,
            if files == 1 { "file" } else { "files" }
        ),
    }
}

pub fn filter_note(lang: Lang, ext: &str) -> String {
    match lang {
        Lang::Ja => format!("  (フィルタ: {})", ext),
        Lang::En => format!("  (filter: {})", ext),
    }
}

pub fn exclude_note(lang: Lang, pats: &str) -> String {
    match lang {
        Lang::Ja => format!("  (除外: {})", pats),
        Lang::En => format!("  (excluded: {})", pats),
    }
}

pub fn include_note(lang: Lang, pats: &str) -> String {
    match lang {
        Lang::Ja => format!("  (抽出: {})", pats),
        Lang::En => format!("  (included: {})", pats),
    }
}

pub fn min_note(lang: Lang, sz: &str) -> String {
    match lang {
        Lang::Ja => format!("  (最小: {})", sz),
        Lang::En => format!("  (min: {})", sz),
    }
}

pub fn max_note(lang: Lang, sz: &str) -> String {
    match lang {
        Lang::Ja => format!("  (最大: {})", sz),
        Lang::En => format!("  (max: {})", sz),
    }
}

pub fn estimated_tokens(lang: Lang, toks: &str) -> String {
    match lang {
        Lang::Ja => format!("  推定トークン数: {}", toks),
        Lang::En => format!("  Estimated tokens: {}", toks),
    }
}

pub fn todo_count(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Ja => format!("  TODO/FIXME等: {}件", n),
        Lang::En => format!("  TODO/FIXME items: {}", n),
    }
}

pub fn more_items(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Ja => format!("    …他 {} 件", n),
        Lang::En => format!("    … {} more", n),
    }
}

pub fn missing_tests(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ja => format!("  テスト未整備: {} ファイル", n),
        Lang::En => format!(
            "  Files without tests: {} {}",
            n,
            if n == 1 { "file" } else { "files" }
        ),
    }
}

pub fn entry_points(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ja => format!("  エントリーポイント候補: {} 件検出", n),
        Lang::En => format!("  Entry point candidates: {} found", n),
    }
}

pub fn config_files(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ja => format!("  設定ファイル: {} 件検出", n),
        Lang::En => format!("  Config files: {} found", n),
    }
}

pub fn cycles_found(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ja => format!("  循環依存: {} 件検出", n),
        Lang::En => format!("  Circular dependencies: {} found", n),
    }
}

pub fn change_count(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Ja => format!("({} 回変更)", n),
        Lang::En => format!("(changed {} times)", n),
    }
}

pub fn write_failed(lang: Lang, path: &str, err: &str) -> String {
    match lang {
        Lang::Ja => format!("エラー: '{}' に書き込めません: {}", path, err),
        Lang::En => format!("error: cannot write to '{}': {}", path, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lang() {
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("JA"), Some(Lang::Ja));
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn rel_dates_en() {
        assert_eq!(rel_date(Lang::En, 10), "now");
        assert_eq!(rel_date(Lang::En, 90), "1 min ago");
        assert_eq!(rel_date(Lang::En, 7200), "2 hours ago");
        assert_eq!(rel_date(Lang::En, 86400 * 2), "2 days ago");
        assert_eq!(rel_date(Lang::En, 86400 * 29), "4 weeks ago");
        assert_eq!(rel_date(Lang::En, 86400 * 40), "1 month ago");
        assert_eq!(rel_date(Lang::En, 86400 * 800), "2 years ago");
    }

    #[test]
    fn rel_dates_ja_byte_compat() {
        // 旧 Python 版とのバイト一致（ゴールデン互換モード）
        assert_eq!(rel_date(Lang::Ja, 10), "今");
        assert_eq!(rel_date(Lang::Ja, 5400), "1時間前");
        assert_eq!(rel_date(Lang::Ja, 86400 * 100), "3ヶ月前");
        assert_eq!(rel_date(Lang::Ja, 3600 * 30), "昨日");
    }
}
