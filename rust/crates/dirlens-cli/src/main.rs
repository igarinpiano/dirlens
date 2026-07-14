// dirlens – ファイルサイズ付きディレクトリツリー表示ツール（Rust 版 CLI）
//
// Copyright 2026 Igarin
// Licensed under the Apache License, Version 2.0.
// See the LICENSE file or http://www.apache.org/licenses/LICENSE-2.0

mod config;
mod providers;

use std::io::{IsTerminal, Write};

use clap::{Arg, ArgAction, Command};
use dirlens_core::{execute, prefetch_targets, prepare, Args, Lang, Session};

use providers::{StdClipboard, StdFs, StdGit};

/// ヘルプ言語を argv / 環境変数から先読みする（clap パース前に必要なため）。
/// 優先順: DIRLENS_COMPAT=python（ja 固定）> --lang > DIRLENS_LANG > 英語。
fn detect_lang() -> Lang {
    if std::env::var("DIRLENS_COMPAT").as_deref() == Ok("python") {
        return Lang::Ja;
    }
    let argv: Vec<String> = std::env::args().collect();
    for (i, a) in argv.iter().enumerate() {
        if a == "--lang" {
            if let Some(l) = argv.get(i + 1).and_then(|v| Lang::parse(v)) {
                return l;
            }
        } else if let Some(v) = a.strip_prefix("--lang=") {
            if let Some(l) = Lang::parse(v) {
                return l;
            }
        }
    }
    if let Ok(v) = std::env::var("DIRLENS_LANG") {
        if let Some(l) = Lang::parse(&v) {
            return l;
        }
    }
    Lang::En
}

fn build_command(lang: Lang) -> Command {
    let ja = lang == Lang::Ja;
    // ヘルプ文字列の言語選択（en がデフォルト）
    let h = move |en: &'static str, ja_s: &'static str| if ja { ja_s } else { en };
    let flag = |short: char, long: Option<&'static str>, id: &'static str, help: &'static str| {
        let mut a = Arg::new(id).short(short).action(ArgAction::SetTrue).help(help);
        if let Some(l) = long {
            a = a.long(l);
        }
        a
    };
    Command::new("dirlens")
        .about(h(
            "Display a directory tree with file sizes and code analysis",
            "ファイルサイズ付きのディレクトリツリーを表示します",
        ))
        .version(env!("CARGO_PKG_VERSION"))
        // -V は --missing-tests が使うため、バージョンは --version のみ（下で定義）
        .disable_version_flag(true)
        .after_help(h(
            "Examples:\n\
             \x20 dirlens --ai             for pasting into an AI chat (human copy-paste)\n\
             \x20 dirlens --agent          agent-oriented analysis (no color, no clipboard)\n\
             \x20 dirlens -d               directories only (tree -d compatible)\n\
             \x20 dirlens -L 2             limit depth to 2 (tree -L compatible)\n\
             \x20 dirlens -G --prune       apply .gitignore + prune empty branches\n\
             \x20 dirlens -T               per-file token counts\n\
             \x20 dirlens -H               last commit info (requires git)\n\
             \x20 dirlens -K               extract TODO/FIXME/HACK comments\n\
             \x20 dirlens -V               source files without tests\n\
             \x20 dirlens -N               mark likely entry points\n\
             \x20 dirlens -O               function/class outline\n\
             \x20 dirlens -M               local import/dependency analysis\n\
             \x20 dirlens --no-color > dirlens.txt   write to a file",
            "使用例:\n\
             \x20 dirlens --ai             AIチャット貼り付け用（人間がコピペする想定）\n\
             \x20 dirlens --agent          エージェント向け解析（カラーなし・クリップボードは使わない）\n\
             \x20 dirlens -d               ディレクトリのみ表示（tree -d 互換）\n\
             \x20 dirlens -L 2             深さ 2 まで表示（tree -L 互換）\n\
             \x20 dirlens -G --prune       gitignore除外 + 空枝を剪定\n\
             \x20 dirlens -T               ファイルごとの推定トークン数を表示\n\
             \x20 dirlens -H               最終コミット情報を表示（要git）\n\
             \x20 dirlens -K               TODO/FIXME/HACKを抽出\n\
             \x20 dirlens -V               テストが無いソースファイルを表示\n\
             \x20 dirlens -N               エントリーポイントらしきファイルをマーク\n\
             \x20 dirlens -O               関数・クラスの簡易アウトラインを表示\n\
             \x20 dirlens -M               ローカルなimport/依存関係を解析\n\
             \x20 dirlens --no-color > dirlens.txt   ファイルに書き出す",
        ))
        // ── tree互換フラグ ────────────────────────────────────
        .arg(flag('d', None, "dirs_only", h("directories only (tree -d)", "ディレクトリのみ表示（tree -d 互換）")))
        .arg(flag('g', None, "show_group", h("show group name (tree -g)", "グループ名を表示（tree -g 互換）")))
        .arg(flag('s', None, "show_size_compat", h("show sizes (always on; tree -s)", "サイズ表示（常時有効・tree -s 互換）")))
        .arg(flag('t', None, "sort_mtime", h("sort by modification time (tree -t)", "更新日時順にソート（tree -t 互換）")))
        .arg(flag('c', None, "sort_ctime", h("sort by status-change time (tree -c)", "ステータス変更日時順にソート（tree -c 互換）")))
        // ── dirlens独自フラグ ─────────────────────────────────
        .arg(flag('G', Some("gitignore"), "gitignore", h("exclude files listed in .gitignore", ".gitignoreのファイルを除外（旧 -g）")))
        .arg(flag('S', Some("sort-size"), "sort_size", h("sort by size, largest first", "サイズ順にソート（旧 -s）")))
        .arg(
            Arg::new("type")
                .short('e')
                .long("type")
                .value_name("EXT")
                .help(h("show only files with this extension", "指定した拡張子のみ表示（旧 -t）")),
        )
        .arg(flag('C', Some("copy"), "copy", h("copy output to clipboard", "クリップボードにコピー（旧 -c）")))
        // ── tree互換フラグ（変更なし） ────────────────────────
        .arg(flag('a', Some("all"), "all", h("show hidden files too", "隠しファイルも表示")))
        .arg(flag('f', Some("full-path"), "full_path", h("print full paths from the root", "ルートからのフルパスで表示")))
        .arg(flag('l', Some("follow"), "follow", h("follow symlinked directories", "シンボリックリンク先ディレクトリを展開")))
        .arg(flag('p', Some("perms"), "perms", h("show permission string", "パーミッション文字列を表示")))
        .arg(flag('u', Some("user"), "user", h("show owner user name", "所有者のユーザー名を表示")))
        .arg(flag('r', Some("reverse"), "reverse", h("reverse sort order", "ソート順を逆にする")))
        .arg(flag('n', None, "no_color_tree", h("no colors (tree -n)", "カラーなし（tree -n 互換）")))
        .arg(flag('J', None, "json_tree", h("JSON output (tree -J)", "JSON形式で出力（tree -J 互換）")))
        .arg(
            Arg::new("level")
                .short('L')
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(clap::value_parser!(i64))
                .help(h("max display depth (tree -L)", "表示する最大の深さ（tree -L 互換）")),
        )
        .arg(flag('D', None, "date_tree", h("show last-modified time (tree -D)", "最終更新日時を表示（tree -D 互換）")))
        .arg(
            Arg::new("include_tree")
                .short('P')
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(h("show only entries matching pattern (tree -P)", "このパターンのみ表示（tree -P 互換）")),
        )
        .arg(
            Arg::new("exclude_tree")
                .short('I')
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(h("exclude entries matching pattern (tree -I)", "除外パターン（tree -I 互換）")),
        )
        // ── AI/エージェント向け解析フラグ ─────────────────────
        .arg(flag('T', Some("tokens"), "tokens", h("show per-file token counts (BPE)", "ファイルごとの推定トークン数を表示（概算）")))
        .arg(flag('H', Some("git"), "git", h("show last commit info (requires git; scans last 2000 commits)", "最終コミット情報を表示（要git、直近2000コミットまで走査）")))
        .arg(flag('K', Some("todo"), "todo", h("extract TODO/FIXME/HACK/XXX comments", "TODO/FIXME/HACK/XXXコメントを抽出")))
        .arg(flag('V', Some("missing-tests"), "tests", h("show source files without a matching test file", "対応するテストファイルが見つからないソースファイルを表示")))
        .arg(flag('N', Some("entry"), "entry", h("detect and mark likely entry points", "エントリーポイントらしきファイルを検出してマーク")))
        .arg(flag('O', Some("outline"), "outline", h("show function/class outline (AST-based)", "関数・クラスの簡易アウトラインを表示（対応言語限定）")))
        .arg(flag('M', Some("imports"), "imports", h("analyze local import/dependency graph (detects cycles)", "ローカルなimport/依存関係を解析して表示（外部パッケージは対象外）。循環依存も併せて検出")))
        .arg(flag('A', Some("api"), "api", h("public API symbols only (implies -O)", "公開API（exportされたシンボル）のみに絞り込む（-O を自動的に有効化）")))
        .arg(flag('F', Some("config"), "config", h("detect config files (.env, tsconfig.json, ...)", "設定ファイル（.env, tsconfig.json等）を検出してマーク")))
        // ── dirlens独自オプション ─────────────────────────────
        .arg(Arg::new("path").default_value(".").help(h("target directory (default: current)", "対象ディレクトリ（省略時はカレント）")))
        .arg(
            Arg::new("depth")
                .long("depth")
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(clap::value_parser!(i64))
                .help(h("max display depth (same as -L)", "表示する最大の深さ（-L と同じ）")),
        )
        .arg(Arg::new("date").long("date").action(ArgAction::SetTrue).help(h("show relative last-modified time", "最終更新日時を相対表示")))
        .arg(flag('m', Some("markdown"), "markdown", h("output as a Markdown code block", "Markdown コードブロック形式で出力")))
        .arg(Arg::new("no_color").long("no-color").action(ArgAction::SetTrue).help(h("disable colors", "カラー表示を無効化")))
        .arg(Arg::new("bar").long("bar").action(ArgAction::SetTrue).help(h("show disk-usage bars", "ディスク占有率バーを表示")))
        .arg(Arg::new("min_size").long("min-size").value_name("SIZE").help(h("only files at least this size (e.g. 1M, 500K)", "指定サイズ以上のファイルのみ表示（例: 1M, 500K）")))
        .arg(Arg::new("max_size").long("max-size").value_name("SIZE").help(h("only files at most this size", "指定サイズ以下のファイルのみ表示")))
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(h("exclude pattern (repeatable)", "除外パターン（複数指定可）")),
        )
        .arg(
            Arg::new("include")
                .long("include")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help(h("include-only pattern (repeatable)", "このパターンのみ表示（複数指定可）")),
        )
        .arg(Arg::new("emoji").long("emoji").action(ArgAction::SetTrue).help(h("show emoji icons by extension", "拡張子に応じた絵文字アイコンを表示")))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help(h("JSON output", "JSON形式で出力")))
        .arg(
            Arg::new("html")
                .long("html")
                .value_name("FILE")
                .num_args(0..=1)
                .default_missing_value("dirlens.html")
                .help(h("generate an HTML report (default: dirlens.html)", "HTMLレポートを生成（デフォルト: dirlens.html）")),
        )
        .arg(Arg::new("prune").long("prune").action(ArgAction::SetTrue).help(h("hide directories that become empty after filtering", "フィルタ後に空になるディレクトリを非表示")))
        .arg(Arg::new("filesfirst").long("filesfirst").action(ArgAction::SetTrue).help(h("list files before directories", "ファイルをディレクトリより先に表示")))
        // ── 表示モード・注釈（v1.2 拡張） ──────────────────────
        .arg(
            Arg::new("top")
                .long("top")
                .value_name("N")
                .value_parser(clap::value_parser!(usize))
                .help(h("flat list of the N largest files and directories (no tree)", "大きいファイル/ディレクトリ上位Nをフラット表示（ツリーなし）")),
        )
        .arg(Arg::new("dupes").long("dupes").action(ArgAction::SetTrue).help(h("find duplicate files by content (size + hash)", "内容が同一の重複ファイルを検出（サイズ+ハッシュ）")))
        .arg(
            Arg::new("compare")
                .long("compare")
                .value_name("DIR")
                .help(h("compare the target tree against DIR (added/removed/changed)", "対象ツリーと DIR を比較（追加/削除/変更）")),
        )
        .arg(Arg::new("status").long("status").action(ArgAction::SetTrue).help(h("overlay git status marks ([M]/[??]/[A]) on the tree", "git status のマーク（[M]/[??]/[A]）をツリーに重ねて表示")))
        .arg(
            Arg::new("heat")
                .long("heat")
                .value_name("MODE")
                .value_parser(["age", "size", "churn"])
                .help(h("color file names by age / size / git churn", "ファイル名を age / size / churn でグラデーション着色")),
        )
        .arg(
            Arg::new("since")
                .long("since")
                .value_name("REF")
                .help(h("show only files changed since the git ref (plus untracked)", "指定 git ref 以降に変更されたファイルのみ表示（未追跡含む）")),
        )
        .arg(
            Arg::new("focus")
                .long("focus")
                .value_name("FILE")
                .help(h("impact analysis: what imports FILE and what it imports (implies -M)", "影響範囲: FILE の依存元/依存先を推移的に表示（-M を暗黙有効化）")),
        )
        .arg(Arg::new("stdin").long("stdin").action(ArgAction::SetTrue).help(h("analyze only the files listed on stdin (one per line)", "stdin のファイルリスト（1行1ファイル）だけを解析")))
        .arg(
            Arg::new("budget")
                .long("budget")
                .value_name("TOKENS")
                .value_parser(clap::value_parser!(i64))
                .help(h("fit text output within a token budget (reduces depth/detail)", "テキスト出力を指定トークン数以内に自動調整（深さ・詳細を削減）")),
        )
        .arg(
            Arg::new("api_diff")
                .long("api-diff")
                .value_name("REF")
                .help(h("diff public API symbols against a git ref", "公開APIシンボルを指定 git ref と比較")),
        )
        .arg(
            Arg::new("pack")
                .long("pack")
                .value_name("FILE")
                .action(ArgAction::Append)
                .help(h("bundle files (tree context + contents + token count) for pasting", "指定ファイルの中身+文脈+トークン数を貼り付け用に整形（複数可）")),
        )
        .arg(Arg::new("mermaid").long("mermaid").action(ArgAction::SetTrue).help(h("output the import graph as a Mermaid diagram (implies -M)", "importグラフを Mermaid 形式で出力（-M を暗黙有効化）")))
        .arg(Arg::new("dot").long("dot").action(ArgAction::SetTrue).help(h("output the import graph as Graphviz DOT (implies -M)", "importグラフを Graphviz DOT 形式で出力（-M を暗黙有効化）")))
        .arg(Arg::new("csv").long("csv").action(ArgAction::SetTrue).help(h("output file metadata as CSV", "ファイルメタデータを CSV 形式で出力")))
        .arg(
            Arg::new("lang")
                .long("lang")
                .value_name("LANG")
                .value_parser(["en", "ja"])
                .help(h("output language (en/ja; default: en, or DIRLENS_LANG / config file)", "出力言語（en/ja。既定: en。DIRLENS_LANG や設定ファイルでも指定可）")),
        )
        .arg(
            Arg::new("preset")
                .long("preset")
                .value_name("NAME")
                .help(h("apply a named preset from the config file ([presets] table)", "設定ファイルの [presets] で定義した名前付きプリセットを適用")),
        )
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(ArgAction::SetTrue)
                .help(h("ignore all config files (also: DIRLENS_CONFIG=off)", "設定ファイルを一切読まない（DIRLENS_CONFIG=off も同じ）")),
        )
        .arg(
            Arg::new("ai")
                .long("ai")
                .action(ArgAction::SetTrue)
                .help(h("shortcut for -G --date -m -C (for pasting into AI chats)", "-G --date -m -C のショートカット（人間がAIチャットに貼り付ける用）")),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .action(ArgAction::SetTrue)
                .help(h("shortcut for -G -T -H -K -V -N -O -M -F --no-color (agent-oriented analysis; no clipboard)", "-G -T -H -K -V -N -O -M -F --no-color のショートカット（エージェント向け解析、カラーなし・クリップボードは使わない）")),
        )
        .arg(
            Arg::new("check")
                .long("check")
                .action(ArgAction::SetTrue)
                .help(h("capability report (gitignore tier, per-language analysis, git/clipboard). exit code 1 if degraded; supports --json", "能力レポートを表示（gitignore層・言語別解析方式・git/クリップボード可否）。縮退があると終了コード 1。--json 併用可")),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::Version)
                .help(h("print version", "バージョンを表示")),
        )
}

/// dirlens.py の _enable_color 相当。
fn enable_color() -> bool {
    if !std::io::stdout().is_terminal() {
        return false;
    }
    #[cfg(windows)]
    {
        // Win10+ の ANSI 対応前提（spec §2 により簡略化）
        return std::env::var_os("WT_SESSION").is_some()
            || std::env::var_os("TERM_PROGRAM").is_some()
            || std::env::var_os("TERM").is_some()
            || std::env::var_os("ANSICON").is_some();
    }
    #[cfg(not(windows))]
    true
}

fn main() {
    let mut lang = detect_lang();
    let mut m = build_command(lang).get_matches();

    // ── 設定ファイル（グローバル + プロジェクト） ───────────────
    let argv: Vec<String> = std::env::args().collect();
    let use_config = !config::config_disabled(&argv) && !m.get_flag("no_config");
    let (file_cfg, cfg_warnings) = if use_config {
        let target = m
            .get_one::<String>("path")
            .cloned()
            .unwrap_or_else(|| ".".into());
        config::load(std::path::Path::new(&target))
    } else {
        (Default::default(), Vec::new())
    };
    for w in &cfg_warnings {
        eprintln!("{}", w);
    }

    // --preset: 設定ファイルのプリセット引数を argv の先頭（プログラム名の直後）に
    // 差し込んで再パースする（CLI で明示した引数が常に勝つ）。
    if let Some(name) = m.get_one::<String>("preset").cloned() {
        match file_cfg.presets.get(&name) {
            Some(extra) => {
                let mut new_argv: Vec<String> = Vec::with_capacity(argv.len() + extra.len());
                new_argv.push(argv[0].clone());
                new_argv.extend(extra.iter().cloned());
                new_argv.extend(argv[1..].iter().cloned());
                m = build_command(lang).get_matches_from(new_argv);
            }
            None => {
                let known: Vec<&String> = file_cfg.presets.keys().collect();
                eprintln!(
                    "dirlens: unknown preset '{}' (defined: {:?})",
                    name, known
                );
                std::process::exit(2);
            }
        }
    }

    // 言語の最終解決: --lang > 設定ファイル > DIRLENS_LANG > en
    // （detect_lang は --lang / 環境変数 / compat を反映済み。設定ファイルは
    //   フラグ・環境変数どちらも無い場合のみ効く）
    if m.value_source("lang") != Some(clap::parser::ValueSource::CommandLine)
        && std::env::var("DIRLENS_LANG").is_err()
        && std::env::var("DIRLENS_COMPAT").as_deref() != Ok("python")
    {
        if let Some(l) = file_cfg.lang.as_deref().and_then(Lang::parse) {
            lang = l;
        }
    }

    let getb = |id: &str| m.get_flag(id);
    let mut args = Args {
        path: m.get_one::<String>("path").cloned().unwrap_or_else(|| ".".into()),
        depth: m.get_one::<i64>("depth").copied(),
        dirs_only: getb("dirs_only"),
        show_group: getb("show_group"),
        sort_mtime: getb("sort_mtime"),
        sort_ctime: getb("sort_ctime"),
        all: getb("all"),
        full_path: getb("full_path"),
        follow: getb("follow"),
        perms: getb("perms"),
        user: getb("user"),
        reverse: getb("reverse"),
        gitignore: getb("gitignore"),
        sort_size: getb("sort_size"),
        type_ext: m.get_one::<String>("type").cloned(),
        copy: getb("copy"),
        date: getb("date"),
        markdown: getb("markdown"),
        no_color: getb("no_color"),
        bar: getb("bar"),
        min_size: m.get_one::<String>("min_size").cloned(),
        max_size: m.get_one::<String>("max_size").cloned(),
        exclude: m
            .get_many::<String>("exclude")
            .map(|v| v.cloned().collect())
            .unwrap_or_default(),
        include: m
            .get_many::<String>("include")
            .map(|v| v.cloned().collect())
            .unwrap_or_default(),
        emoji: getb("emoji"),
        json: getb("json"),
        html: m.get_one::<String>("html").cloned(),
        prune: getb("prune"),
        filesfirst: getb("filesfirst"),
        ai: getb("ai"),
        agent: getb("agent"),
        tokens: getb("tokens"),
        git: getb("git"),
        todo: getb("todo"),
        tests: getb("tests"),
        entry: getb("entry"),
        outline: getb("outline"),
        imports: getb("imports"),
        api: getb("api"),
        config: getb("config"),
        check: getb("check"),
        top: m.get_one::<usize>("top").copied(),
        dupes: getb("dupes"),
        compare: m.get_one::<String>("compare").cloned(),
        status: getb("status"),
        heat: m.get_one::<String>("heat").cloned(),
        since: m.get_one::<String>("since").cloned(),
        focus: m.get_one::<String>("focus").cloned(),
        stdin_files: if getb("stdin") {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf);
            Some(
                buf.lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect(),
            )
        } else {
            None
        },
        budget: m.get_one::<i64>("budget").copied(),
        api_diff: m.get_one::<String>("api_diff").cloned(),
        pack: m
            .get_many::<String>("pack")
            .map(|v| v.cloned().collect())
            .unwrap_or_default(),
        mermaid: getb("mermaid"),
        dot: getb("dot"),
        csv: getb("csv"),
        lang: Some(match lang {
            Lang::En => "en".to_string(),
            Lang::Ja => "ja".to_string(),
        }),
    };

    // ── エイリアスのマージ（argparse 相当） ────────────────────
    if let Some(level) = m.get_one::<i64>("level").copied() {
        args.depth = Some(level); // -L は --depth より優先
    }
    if getb("date_tree") {
        args.date = true;
    }
    if let Some(v) = m.get_many::<String>("include_tree") {
        args.include.extend(v.cloned());
    }
    if let Some(v) = m.get_many::<String>("exclude_tree") {
        args.exclude.extend(v.cloned());
    }
    if getb("no_color_tree") {
        args.no_color = true;
    }
    if getb("json_tree") {
        args.json = true;
    }

    // ── 設定ファイルのデフォルト適用（CLI で指定が無い項目のみ） ──
    if use_config {
        macro_rules! def_true {
            ($($f:ident),*) => { $( if file_cfg.$f == Some(true) { args.$f = true; } )* };
        }
        def_true!(gitignore, all, date, emoji, markdown, no_color, bar, prune,
                  filesfirst, follow, full_path);
        if args.depth.is_none() {
            args.depth = file_cfg.depth;
        }
        if args.min_size.is_none() {
            args.min_size = file_cfg.min_size.clone();
        }
        if args.max_size.is_none() {
            args.max_size = file_cfg.max_size.clone();
        }
        args.exclude.extend(file_cfg.exclude.iter().cloned());
        args.include.extend(file_cfg.include.iter().cloned());
    }

    args.merge_aliases();

    let fs = StdFs;
    let git = StdGit;
    let clip = StdClipboard;

    let mut cfg = match prepare(&args, &fs, enable_color()) {
        Ok(cfg) => cfg,
        Err(res) => {
            eprint!("{}", res.stderr);
            std::process::exit(res.exit_code);
        }
    };

    // gitignore 層の選択（テスト・検証用の環境変数。通常は auto = Tier1 を試す）:
    //   DIRLENS_GITIGNORE=builtin … 内蔵マッチャ（Tier3）を強制
    //   DIRLENS_COMPAT=python     … Python 版完全互換モード（ゴールデン検証用）
    let compat_python = std::env::var("DIRLENS_COMPAT").as_deref() == Ok("python");
    match std::env::var("DIRLENS_GITIGNORE").as_deref() {
        Ok("builtin") => cfg.gitignore_prefer_git = false,
        Ok("git") => cfg.gitignore_prefer_git = true,
        _ => {
            if compat_python {
                cfg.gitignore_prefer_git = false;
            }
        }
    }
    // AST 第1段＋import 解決改善の無効化（DIRLENS_AST=off または互換モード）
    if compat_python || std::env::var("DIRLENS_AST").as_deref() == Ok("off") {
        cfg.enhanced_analysis = false;
    }
    // トークン計数層の選択（DIRLENS_TOKENS=heuristic で Tier2 固定）
    if compat_python || std::env::var("DIRLENS_TOKENS").as_deref() == Ok("heuristic") {
        cfg.tokens_bpe = false;
    }
    // 互換モードでは精度注記・schema_version・capabilities も出さない
    if compat_python {
        cfg.suppress_notes = true;
    }

    let mut sess = Session::new(&fs);

    // ── トップレベル dir サイズの並列プリフェッチ ──────────────
    #[cfg(feature = "parallel")]
    {
        let tops = prefetch_targets(&sess, &cfg);
        if tops.len() >= 2 {
            let workers = tops
                .len()
                .min(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
                .min(8);
            let queue = std::sync::Mutex::new(tops);
            let sess_ref = &sess;
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    scope.spawn(|| loop {
                        let next = queue.lock().unwrap().pop();
                        match next {
                            Some(p) => {
                                sess_ref.dir_size(&p);
                            }
                            None => break,
                        }
                    });
                }
            });
        }
    }

    let res = execute(&mut sess, &mut cfg, &git, &clip);

    if let Some((path, content)) = &res.html_file {
        if let Err(e) = std::fs::write(path, content) {
            eprintln!(
                "{}",
                dirlens_core::i18n::write_failed(lang, path, &e.to_string())
            );
            std::process::exit(1);
        }
    }

    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(res.stdout.as_bytes());
    let _ = stdout.flush();
    eprint!("{}", res.stderr);
    std::process::exit(res.exit_code);
}
