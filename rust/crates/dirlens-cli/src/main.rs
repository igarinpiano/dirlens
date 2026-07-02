// dirlens – ファイルサイズ付きディレクトリツリー表示ツール（Rust 版 CLI）
//
// Copyright 2026 Igarin
// Licensed under the Apache License, Version 2.0.
// See the LICENSE file or http://www.apache.org/licenses/LICENSE-2.0

mod providers;

use std::io::{IsTerminal, Write};

use clap::{Arg, ArgAction, Command};
use dirlens_core::{execute, prefetch_targets, prepare, Args, Session};

use providers::{StdClipboard, StdFs, StdGit};

fn build_command() -> Command {
    let flag = |short: char, long: Option<&'static str>, id: &'static str, help: &'static str| {
        let mut a = Arg::new(id).short(short).action(ArgAction::SetTrue).help(help);
        if let Some(l) = long {
            a = a.long(l);
        }
        a
    };
    Command::new("dirlens")
        .about("ファイルサイズ付きのディレクトリツリーを表示します")
        .disable_version_flag(true)
        .after_help(
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
        )
        // ── tree互換フラグ ────────────────────────────────────
        .arg(flag('d', None, "dirs_only", "ディレクトリのみ表示（tree -d 互換）"))
        .arg(flag('g', None, "show_group", "グループ名を表示（tree -g 互換）"))
        .arg(flag('s', None, "show_size_compat", "サイズ表示（常時有効・tree -s 互換）"))
        .arg(flag('t', None, "sort_mtime", "更新日時順にソート（tree -t 互換）"))
        .arg(flag('c', None, "sort_ctime", "ステータス変更日時順にソート（tree -c 互換）"))
        // ── dirlens独自フラグ ─────────────────────────────────
        .arg(flag('G', Some("gitignore"), "gitignore", ".gitignoreのファイルを除外（旧 -g）"))
        .arg(flag('S', Some("sort-size"), "sort_size", "サイズ順にソート（旧 -s）"))
        .arg(
            Arg::new("type")
                .short('e')
                .long("type")
                .value_name("EXT")
                .help("指定した拡張子のみ表示（旧 -t）"),
        )
        .arg(flag('C', Some("copy"), "copy", "クリップボードにコピー（旧 -c）"))
        // ── tree互換フラグ（変更なし） ────────────────────────
        .arg(flag('a', Some("all"), "all", "隠しファイルも表示"))
        .arg(flag('f', Some("full-path"), "full_path", "ルートからのフルパスで表示"))
        .arg(flag('l', Some("follow"), "follow", "シンボリックリンク先ディレクトリを展開"))
        .arg(flag('p', Some("perms"), "perms", "パーミッション文字列を表示"))
        .arg(flag('u', Some("user"), "user", "所有者のユーザー名を表示"))
        .arg(flag('r', Some("reverse"), "reverse", "ソート順を逆にする"))
        .arg(flag('n', None, "no_color_tree", "カラーなし（tree -n 互換）"))
        .arg(flag('J', None, "json_tree", "JSON形式で出力（tree -J 互換）"))
        .arg(
            Arg::new("level")
                .short('L')
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(clap::value_parser!(i64))
                .help("表示する最大の深さ（tree -L 互換）"),
        )
        .arg(flag('D', None, "date_tree", "最終更新日時を表示（tree -D 互換）"))
        .arg(
            Arg::new("include_tree")
                .short('P')
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help("このパターンのみ表示（tree -P 互換）"),
        )
        .arg(
            Arg::new("exclude_tree")
                .short('I')
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help("除外パターン（tree -I 互換）"),
        )
        // ── AI/エージェント向け解析フラグ ─────────────────────
        .arg(flag('T', Some("tokens"), "tokens", "ファイルごとの推定トークン数を表示（概算）"))
        .arg(flag('H', Some("git"), "git", "最終コミット情報を表示（要git、直近2000コミットまで走査）"))
        .arg(flag('K', Some("todo"), "todo", "TODO/FIXME/HACK/XXXコメントを抽出"))
        .arg(flag('V', Some("missing-tests"), "tests", "対応するテストファイルが見つからないソースファイルを表示"))
        .arg(flag('N', Some("entry"), "entry", "エントリーポイントらしきファイルを検出してマーク"))
        .arg(flag('O', Some("outline"), "outline", "関数・クラスの簡易アウトラインを表示（対応言語限定）"))
        .arg(flag('M', Some("imports"), "imports", "ローカルなimport/依存関係を解析して表示（外部パッケージは対象外）。循環依存も併せて検出"))
        .arg(flag('A', Some("api"), "api", "公開API（exportされたシンボル）のみに絞り込む（-O を自動的に有効化）"))
        .arg(flag('F', Some("config"), "config", "設定ファイル（.env, tsconfig.json等）を検出してマーク"))
        // ── dirlens独自オプション ─────────────────────────────
        .arg(Arg::new("path").default_value(".").help("対象ディレクトリ（省略時はカレント）"))
        .arg(
            Arg::new("depth")
                .long("depth")
                .value_name("N")
                .allow_negative_numbers(true)
                .value_parser(clap::value_parser!(i64))
                .help("表示する最大の深さ（-L と同じ）"),
        )
        .arg(Arg::new("date").long("date").action(ArgAction::SetTrue).help("最終更新日時を相対表示"))
        .arg(flag('m', Some("markdown"), "markdown", "Markdown コードブロック形式で出力"))
        .arg(Arg::new("no_color").long("no-color").action(ArgAction::SetTrue).help("カラー表示を無効化"))
        .arg(Arg::new("bar").long("bar").action(ArgAction::SetTrue).help("ディスク占有率バーを表示"))
        .arg(Arg::new("min_size").long("min-size").value_name("SIZE").help("指定サイズ以上のファイルのみ表示（例: 1M, 500K）"))
        .arg(Arg::new("max_size").long("max-size").value_name("SIZE").help("指定サイズ以下のファイルのみ表示"))
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help("除外パターン（複数指定可）"),
        )
        .arg(
            Arg::new("include")
                .long("include")
                .value_name("PATTERN")
                .action(ArgAction::Append)
                .help("このパターンのみ表示（複数指定可）"),
        )
        .arg(Arg::new("emoji").long("emoji").action(ArgAction::SetTrue).help("拡張子に応じた絵文字アイコンを表示"))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("JSON形式で出力"))
        .arg(
            Arg::new("html")
                .long("html")
                .value_name("FILE")
                .num_args(0..=1)
                .default_missing_value("dirlens.html")
                .help("HTMLレポートを生成（デフォルト: dirlens.html）"),
        )
        .arg(Arg::new("prune").long("prune").action(ArgAction::SetTrue).help("フィルタ後に空になるディレクトリを非表示"))
        .arg(Arg::new("filesfirst").long("filesfirst").action(ArgAction::SetTrue).help("ファイルをディレクトリより先に表示"))
        .arg(
            Arg::new("ai")
                .long("ai")
                .action(ArgAction::SetTrue)
                .help("-G --date -m -C のショートカット（人間がAIチャットに貼り付ける用）"),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .action(ArgAction::SetTrue)
                .help("-G -T -H -K -V -N -O -M -F --no-color のショートカット（エージェント向け解析、カラーなし・クリップボードは使わない）"),
        )
        .arg(
            Arg::new("check")
                .long("check")
                .action(ArgAction::SetTrue)
                .help("能力レポートを表示（gitignore層・言語別解析方式・git/クリップボード可否）。縮退があると終了コード 1。--json 併用可"),
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
    let m = build_command().get_matches();

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
            eprintln!("エラー: '{}' に書き込めません: {}", path, e);
            std::process::exit(1);
        }
    }

    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(res.stdout.as_bytes());
    let _ = stdout.flush();
    eprint!("{}", res.stderr);
    std::process::exit(res.exit_code);
}
