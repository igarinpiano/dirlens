//! 設定クラス（dirlens.py の Cfg 相当）。全設定＋プロジェクト解析結果を集約する。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use indexmap::IndexMap;

use crate::args::Args;
use crate::fmt::{parse_size, GitInfo};
use crate::i18n::Lang;

/// --heat のモード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heat {
    Age,
    Size,
    Churn,
}

impl Heat {
    pub fn parse(s: &str) -> Option<Heat> {
        match s {
            "age" => Some(Heat::Age),
            "size" => Some(Heat::Size),
            "churn" => Some(Heat::Churn),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Cfg {
    pub root: PathBuf,
    /// ルート行に出すラベル（target.name、空なら str(target)）
    pub root_label: String,
    pub max_depth: Option<i64>,
    pub show_all: bool,
    pub by_size: bool,
    pub sort_mtime: bool,
    pub sort_ctime: bool,
    pub show_date: bool,
    pub use_gitignore: bool,
    pub show_bar: bool,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub show_emoji: bool,
    pub type_ext: Option<String>,
    pub show_perms: bool,
    pub show_user: bool,
    pub show_group: bool,
    pub dirs_only: bool,
    pub follow_syms: bool,
    pub full_path: bool,
    pub prune: bool,
    pub reverse: bool,
    pub files_first: bool,

    // AI/エージェント向け解析フラグ
    pub show_tokens: bool,
    pub show_git: bool,
    pub show_todo: bool,
    pub show_tests: bool,
    pub show_entry: bool,
    pub show_outline: bool,
    pub show_imports: bool,
    pub show_config: bool,
    pub public_only: bool,
    pub has_extras: bool,

    /// Tier1（git check-ignore）を試すか。CLI が環境変数
    /// （DIRLENS_GITIGNORE=builtin / DIRLENS_COMPAT=python）に応じて false にする。
    /// wasm では GitProvider が常に失敗するため実質 Tier3 固定。
    pub gitignore_prefer_git: bool,
    /// 実際に使われた gitignore 層（"git" / "builtin"）。capabilities 出力用。
    pub gitignore_tier: Option<&'static str>,
    /// スキャンルート自体が gitignore 対象（-G で中身が全て隠れる状態）。
    /// Tier1（git check-ignore）が使える場合のみ検出し、出力に注記する。
    pub root_ignored: bool,
    /// AST 第1段＋マニフェスト読込による import 解決改善を使うか。
    /// false なら正規表現層のみ（DIRLENS_COMPAT=python / DIRLENS_AST=off）。
    pub enhanced_analysis: bool,
    /// トークン計数に BPE（Tier1）を使うか。false なら文字数ヒューリスティック
    /// （DIRLENS_TOKENS=heuristic / DIRLENS_COMPAT=python、または feature 無効ビルド）。
    pub tokens_bpe: bool,
    /// 精度注記・schema_version・capabilities を出さない
    /// （DIRLENS_COMPAT=python: Python 版とのバイト一致検証用）。
    pub suppress_notes: bool,
    /// --check（能力レポートモード）
    pub check: bool,

    // 出力モード
    pub lang: Lang,
    pub use_color: bool,
    pub markdown: bool,
    pub json: bool,
    pub html: Option<String>,
    pub copy: bool,
    pub agent: bool,

    // 表示モード・注釈（v1.2 拡張）
    pub top: Option<usize>,
    pub dupes: bool,
    pub compare: Option<String>,
    pub show_status: bool,
    pub heat: Option<Heat>,
    pub since: Option<String>,
    pub focus: Option<String>,
    pub stdin_files: Option<Vec<String>>,
    pub budget: Option<i64>,
    pub estimate: bool,
    /// --estimate の見積もり表で「この値を超える階層は呼び出し自体が失敗する」
    /// と警告する上限トークン数。MCP 層がホストの応答上限
    /// （MAX_MCP_OUTPUT_TOKENS または Claude Code 既定 25000）を注入する。
    pub estimate_cap: Option<i64>,
    pub api_diff: Option<String>,
    pub pack: Vec<String>,
    pub export_mermaid: bool,
    pub export_dot: bool,
    pub export_csv: bool,

    /// --status: rel path → porcelain XY コード（"M ", "??", …）
    pub status_map: HashMap<String, String>,
    /// --since: 変更ファイル集合（untracked 含む）と各ファイルの状態文字
    pub since_set: HashSet<String>,
    pub since_status: HashMap<String, char>,
    /// --since: ref 以降に削除されたファイル（ツリーには出ないため一覧で出す）
    pub since_deleted: Vec<String>,

    // main() 相当で必要に応じて埋める解析結果
    pub git_map: HashMap<String, GitInfo>,
    pub git_change_counts: IndexMap<String, u64>,
    pub untested_set: HashSet<String>,
    pub entry_set: BTreeSet<String>,
    pub config_set: HashSet<String>,
    pub imports_map: BTreeMap<String, Vec<String>>,
    pub imported_by_map: IndexMap<String, Vec<String>>,
    pub external_map: HashMap<String, Vec<String>>,
    pub cycles: Vec<Vec<String>>,
}

impl Cfg {
    /// Args（エイリアス統合済み）から Cfg を構築する。
    /// min/max サイズの解析エラーはメッセージを返す。
    pub fn from_args(args: &Args, root: PathBuf, root_label: String, use_color: bool)
        -> Result<Cfg, String>
    {
        let lang = args
            .lang
            .as_deref()
            .and_then(Lang::parse)
            .unwrap_or_default();
        let min_size = match &args.min_size {
            Some(s) => Some(parse_size(s, lang)?),
            None => None,
        };
        let max_size = match &args.max_size {
            Some(s) => Some(parse_size(s, lang)?),
            None => None,
        };
        let type_ext = args.type_ext.as_ref().map(|t| {
            format!(".{}", t.trim_start_matches('.')).to_lowercase()
        });
        let has_extras = args.tokens || args.git || args.todo || args.tests
            || args.entry || args.outline || args.imports || args.config;
        let heat = match &args.heat {
            Some(s) => match Heat::parse(s) {
                Some(h) => Some(h),
                None => {
                    return Err(match lang {
                        Lang::Ja => format!("--heat の値が不正です: '{}'（age / size / churn）", s),
                        Lang::En => format!("invalid --heat value: '{}' (age / size / churn)", s),
                    })
                }
            },
            None => None,
        };
        Ok(Cfg {
            root,
            root_label,
            max_depth: args.depth,
            show_all: args.all,
            by_size: args.sort_size,
            sort_mtime: args.sort_mtime,
            sort_ctime: args.sort_ctime,
            show_date: args.date,
            use_gitignore: args.gitignore,
            show_bar: args.bar,
            min_size,
            max_size,
            excludes: args.exclude.clone(),
            includes: args.include.clone(),
            show_emoji: args.emoji,
            type_ext,
            show_perms: args.perms,
            show_user: args.user,
            show_group: args.show_group,
            dirs_only: args.dirs_only,
            follow_syms: args.follow,
            full_path: args.full_path,
            // --since はフィルタ後に空になる枝を自動剪定する
            prune: args.prune || args.since.is_some(),
            reverse: args.reverse,
            files_first: args.filesfirst,
            show_tokens: args.tokens,
            show_git: args.git,
            show_todo: args.todo,
            show_tests: args.tests,
            show_entry: args.entry,
            show_outline: args.outline,
            show_imports: args.imports,
            show_config: args.config,
            public_only: args.api,
            has_extras,
            gitignore_prefer_git: true,
            gitignore_tier: None,
            root_ignored: false,
            enhanced_analysis: true,
            tokens_bpe: true,
            suppress_notes: false,
            check: args.check,
            top: args.top,
            dupes: args.dupes,
            compare: args.compare.clone(),
            show_status: args.status,
            heat,
            since: args.since.clone(),
            focus: args.focus.clone(),
            stdin_files: args.stdin_files.clone(),
            budget: args.budget,
            estimate: args.estimate,
            estimate_cap: args.estimate_cap,
            api_diff: args.api_diff.clone(),
            pack: args.pack.clone(),
            export_mermaid: args.mermaid,
            export_dot: args.dot,
            export_csv: args.csv,
            lang,
            use_color,
            markdown: args.markdown,
            json: args.json,
            html: args.html.clone(),
            copy: args.copy,
            agent: args.agent,
            ..Default::default()
        })
    }
}
