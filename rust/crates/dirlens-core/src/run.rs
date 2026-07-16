//! 実行オーケストレーション（dirlens.py の main() の後半相当）。
//!
//! I/O（stdout/stderr への書き出し・HTML ファイルの書き込み）は呼び出し側
//! （CLI / wasm ホスト）が行う。コアは文字列と終了コードを返すだけ。

use std::sync::Arc;

use crate::analysis::gitlog::load_git_log;
use crate::analysis::gitstatus::{build_since_set, parse_status_porcelain, to_scan_relative};
use crate::analysis::index::build_project_index;
use crate::args::Args;
use crate::cfg::{Cfg, Heat};
use crate::colors::{c, strip_ansi, BOLD, DIM, GREEN, YELLOW};
use crate::fmt::fmt_size;
use crate::i18n::{self, Lang};
use crate::provider::{ClipboardProvider, FsProvider, GitProvider};
use crate::render_html::generate_html;
use crate::render_json::render_json;
use crate::render_text::render_text_with_stats;
use crate::session::Session;

#[derive(Debug, Default)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// --html 時: (書き込み先ファイル名, 内容)。書き込みは呼び出し側が行う。
    pub html_file: Option<(String, String)>,
}

fn early_exit(msg: &str) -> RunResult {
    RunResult {
        stderr: format!("{}\n", msg),
        exit_code: 1,
        ..Default::default()
    }
}

/// `--estimate` 用のレンダリング。cfg.json が立っていれば実際の呼び出しと同じ
/// JSON 経路で測る（テキスト経路は JSON より大幅に軽く、見積もりが乖離するため）。
fn render_for_estimate<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    probe: &crate::check::EnvProbe,
) -> String {
    if cfg.json {
        render_json(sess, cfg, active_pats, probe)
    } else {
        render_text_with_stats(sess, cfg, active_pats).0
    }
}

/// パス解決と Cfg 構築（dirlens.py main() の引数検証部分）。
pub fn prepare<F: FsProvider>(
    args: &Args,
    fs: &F,
    use_color_hint: bool,
) -> Result<Cfg, RunResult> {
    let use_color = use_color_hint && !(args.no_color || args.markdown || args.json);
    let lang = args.lang.as_deref().and_then(Lang::parse).unwrap_or_default();

    let target = match fs.resolve(&args.path) {
        Some(t) => t,
        None => {
            return Err(RunResult {
                stderr: format!("{}\n", lang.t().err_cwd_denied),
                exit_code: 1,
                ..Default::default()
            })
        }
    };

    let st = fs.stat(&target, true);
    match st {
        None => return Err(early_exit(&i18n::err_not_found(lang, &args.path))),
        Some(st) if st.mode & 0o170000 != 0o040000 => {
            // 位置引数が通常ファイルなら単一ファイルレポート（--stdin と同じ経路）。
            // `dirlens -O src/main.py` のような使い方をエラーにしない
            if st.mode & 0o170000 == 0o100000 && args.stdin_files.is_none() {
                let parent = target
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| target.clone());
                let root_label = parent
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| parent.to_string_lossy().into_owned());
                let mut cfg = Cfg::from_args(args, parent, root_label, use_color)
                    .map_err(|msg| early_exit(&i18n::err_prefix(lang, &msg)))?;
                cfg.stdin_files = Some(vec![args.path.clone()]);
                // --stdin と同じ暗黙フラグ（トークン・アウトライン・TODO）
                cfg.show_tokens = true;
                cfg.show_outline = true;
                cfg.show_todo = true;
                cfg.has_extras = true;
                return Ok(cfg);
            }
            return Err(early_exit(&i18n::err_not_dir(lang, &args.path)));
        }
        _ => {}
    }

    let root_label = target
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string_lossy().into_owned());

    Cfg::from_args(args, target, root_label, use_color)
        .map_err(|msg| early_exit(&i18n::err_prefix(lang, &msg)))
}

/// ルート直下のディレクトリ一覧（並列プリフェッチ用・_prefetch_sizes の対象列挙）。
pub fn prefetch_targets<F: FsProvider>(sess: &Session<F>, cfg: &Cfg) -> Vec<std::path::PathBuf> {
    match sess.fs.scan_dir(&cfg.root) {
        Ok(entries) => entries
            .into_iter()
            .filter(|e| e.is_dir_nofollow)
            .map(|e| e.path)
            .collect(),
        Err(()) => Vec::new(),
    }
}

/// 解析＋レンダリング本体。
pub fn execute<F: FsProvider>(
    sess: &mut Session<F>,
    cfg: &mut Cfg,
    git: &dyn GitProvider,
    clip: &dyn ClipboardProvider,
) -> RunResult {
    let probe = crate::check::EnvProbe {
        git_available: git.available(),
        is_work_tree: git.is_work_tree(&cfg.root),
        clipboard: clip.available(),
    };

    // ── --check（能力レポート） ───────────────────────────────
    if cfg.check {
        let (stdout, exit_code) = crate::check::render_check(cfg, &probe, cfg.json);
        return RunResult {
            stdout,
            exit_code,
            ..Default::default()
        };
    }

    let active_pats: Arc<Vec<String>> = if cfg.use_gitignore {
        sess.load_gitignore(&cfg.root.clone())
    } else {
        Arc::new(Vec::new())
    };

    // gitignore 2層: Tier1（git check-ignore）を試し、失敗時は Tier3（内蔵マッチャ）へ縮退
    if cfg.use_gitignore {
        let mut tier = "builtin";
        if cfg.gitignore_prefer_git {
            if let Some(set) =
                crate::gitignore::build_git_ignored_set(sess, git, &cfg.root.clone())
            {
                sess.git_ignored = Some(set);
                tier = "git";
            }
        }
        cfg.gitignore_tier = Some(tier);
    }

    // ── git status / since の読み込み ────────────────────────
    // git の出力パスはリポジトリルート相対。スキャンルートがリポジトリの
    // サブディレクトリの場合に備え、スキャンルート相対へ変換して突き合わせる。
    let git_prefix: String = if cfg.show_status || cfg.since.is_some() {
        git.repo_prefix(&cfg.root).unwrap_or_default()
    } else {
        String::new()
    };
    if cfg.show_status || cfg.since.is_some() {
        if let Some(out) = git.status_output(&cfg.root) {
            cfg.status_map = parse_status_porcelain(&out)
                .into_iter()
                .filter_map(|(p, xy)| {
                    to_scan_relative(&p, &git_prefix)
                        .map(|s| (s.to_string(), xy))
                })
                .collect();
        }
    }
    if let Some(r) = cfg.since.clone() {
        let diff = git.diff_names(&cfg.root, &r);
        if diff.is_none() {
            return early_exit(&match cfg.lang {
                Lang::Ja => format!(
                    "エラー: --since {} を解決できません（git が無いか、ref が不正です）",
                    r
                ),
                Lang::En => format!(
                    "error: cannot resolve --since {} (git unavailable or bad ref)",
                    r
                ),
            });
        }
        let status = git.status_output(&cfg.root);
        let (set, marks, deleted) =
            build_since_set(diff.as_deref(), status.as_deref(), &git_prefix);
        cfg.since_set = set;
        cfg.since_status = marks;
        cfg.since_deleted = deleted;
    }

    // ── ツリー以外の出力モード（--compare / --dupes / --top） ──
    if let Some(other) = cfg.compare.clone() {
        let other_path = match sess.fs.resolve(&other) {
            Some(p) => p,
            None => return early_exit(&i18n::err_not_found(cfg.lang, &other)),
        };
        match sess.fs.stat(&other_path, true) {
            Some(st) if st.mode & 0o170000 == 0o040000 => {}
            Some(_) => return early_exit(&i18n::err_not_dir(cfg.lang, &other)),
            None => return early_exit(&i18n::err_not_found(cfg.lang, &other)),
        }
        // Tier1（git check-ignore）はルート A 基準のため、比較では内蔵マッチャに統一
        sess.git_ignored = None;
        let out = crate::modes::render_compare(sess, cfg, &active_pats, &other_path);
        return RunResult {
            stdout: out,
            ..Default::default()
        };
    }
    if cfg.dupes {
        return RunResult {
            stdout: crate::modes::render_dupes(sess, cfg, &active_pats),
            ..Default::default()
        };
    }
    if let Some(n) = cfg.top {
        return RunResult {
            stdout: crate::modes::render_top(sess, cfg, &active_pats, n),
            ..Default::default()
        };
    }

    if cfg.show_tests || cfg.show_entry || cfg.show_config || cfg.show_imports {
        let idx = build_project_index(sess, &cfg.root.clone(), cfg, &active_pats);
        cfg.untested_set = idx.untested;
        cfg.entry_set = idx.entry_set;
        cfg.config_set = idx.config_set;
        cfg.imports_map = idx.imports_map;
        cfg.imported_by_map = idx.imported_by_map;
        cfg.external_map = idx.external_map;
        cfg.cycles = idx.cycles;
    }
    if cfg.show_git || cfg.heat == Some(Heat::Churn) {
        // DIRLENS_COMPAT=python（suppress_notes）では Python 版と同じく
        // リポジトリルート相対のまま突き合わせる（DELTAS §14）
        let (map, counts) = load_git_log(git, &cfg.root, !cfg.suppress_notes);
        cfg.git_map = map;
        cfg.git_change_counts = counts;
    }

    // ── --estimate: 階層別の出力コスト見積もり ─────────────────
    // 「最低何トークン必要か / 次の階層は何トークンか / 解析注釈込みだと
    //  何トークンか」を数行で答える。--budget の値を決める材料に使う。
    if cfg.estimate {
        let measure = |s: &str| {
            crate::analysis::text_metrics::count_tokens(s, s.len(), None, false, cfg.tokens_bpe)
        };
        let orig_depth = cfg.max_depth;
        let mut rows: Vec<(String, i64)> = Vec::new();

        // ツリーだけ（解析注釈なし）の最小コスト
        if cfg.has_extras {
            let (st, sg, sk, sv, se, so, si, sc, ss) = (
                cfg.show_tokens, cfg.show_git, cfg.show_todo, cfg.show_tests,
                cfg.show_entry, cfg.show_outline, cfg.show_imports, cfg.show_config,
                cfg.show_status,
            );
            cfg.show_tokens = false;
            cfg.show_git = false;
            cfg.show_todo = false;
            cfg.show_tests = false;
            cfg.show_entry = false;
            cfg.show_outline = false;
            cfg.show_imports = false;
            cfg.show_config = false;
            cfg.show_status = false;
            cfg.has_extras = false;
            cfg.max_depth = Some(1);
            let t = render_for_estimate(sess, cfg, &active_pats, &probe);
            rows.push((
                i18n::tr(cfg.lang, "-L 1, tree only", "-L 1・ツリーのみ").to_string(),
                measure(&t),
            ));
            cfg.show_tokens = st;
            cfg.show_git = sg;
            cfg.show_todo = sk;
            cfg.show_tests = sv;
            cfg.show_entry = se;
            cfg.show_outline = so;
            cfg.show_imports = si;
            cfg.show_config = sc;
            cfg.show_status = ss;
            cfg.has_extras = true;
        }

        // 現在のフラグでの階層別コスト
        for d in [1i64, 2, 3] {
            if let Some(orig) = orig_depth {
                if d >= orig {
                    break;
                }
            }
            cfg.max_depth = Some(d);
            let t = render_for_estimate(sess, cfg, &active_pats, &probe);
            rows.push((format!("-L {}", d), measure(&t)));
        }
        cfg.max_depth = orig_depth;
        let t = render_for_estimate(sess, cfg, &active_pats, &probe);
        rows.push((
            match orig_depth {
                Some(d) => format!("-L {} ({})", d, i18n::tr(cfg.lang, "current", "現在")),
                None => i18n::tr(cfg.lang, "full depth", "全階層").to_string(),
            },
            measure(&t),
        ));

        // JSON 経路の見積もり時は、budget 指定時に返るテキスト形式の全階層
        // コストも併記する（テキストは JSON より大幅に軽いため、JSON の
        // 見積もりだけを見て不必要に depth を絞る判断を防ぐ）
        let mut text_full: Option<i64> = None;
        if cfg.json {
            cfg.json = false;
            let t = render_for_estimate(sess, cfg, &active_pats, &probe);
            cfg.json = true;
            let toks = measure(&t);
            text_full = Some(toks);
            rows.push((
                i18n::tr(
                    cfg.lang,
                    "full depth as text (what budget returns)",
                    "全階層・テキスト（budget 指定時の形式）",
                )
                .to_string(),
                toks,
            ));
        }

        let mut out = String::new();
        if cfg.json {
            out.push_str(i18n::tr(
                cfg.lang,
                "Estimated output tokens for the current flags, measured as JSON output (BPE o200k):\n",
                "現在のフラグでの出力トークン見積もり（JSON出力として測定・BPE o200k）:\n",
            ));
        } else {
            out.push_str(i18n::tr(
                cfg.lang,
                "Estimated output tokens for the current flags, measured as text output (BPE o200k):\n",
                "現在のフラグでの出力トークン見積もり（テキスト出力として測定・BPE o200k）:\n",
            ));
        }
        let label_w = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
        for (label, toks) in &rows {
            // ホスト上限（MCP 層が注入）を超える階層はその行で警告する。
            // 見積もり値が正確でも、上限超過なら呼び出し自体が失敗するため
            let cap_mark = match cfg.estimate_cap {
                Some(cap) if *toks > cap => {
                    i18n::tr(cfg.lang, "  ⚠ exceeds host cap", "  ⚠ ホスト上限超過")
                }
                _ => "",
            };
            out.push_str(&format!(
                "  {:<w$}  {}{}\n",
                label,
                crate::fmt::fmt_tokens(*toks),
                cap_mark,
                w = label_w
            ));
        }
        out.push_str(i18n::tr(
            cfg.lang,
            "Use --budget N to fit the output automatically.\n",
            "--budget N を付けると出力を自動で予算内に調整できます。\n",
        ));
        if let Some(cap) = cfg.estimate_cap {
            let any_over = rows.iter().any(|(_, t)| *t > cap);
            let line = if any_over {
                match cfg.lang {
                    Lang::Ja => format!(
                        "ホスト応答上限: ~{} トークン。⚠ の付いた階層は無指定で実行すると失敗します — 上限未満の --budget（例: {}）を指定してください。",
                        cap,
                        (cap - 5000).max(cap * 4 / 5)
                    ),
                    Lang::En => format!(
                        "Host response cap: ~{} tokens. Levels marked ⚠ will fail if run uncapped — pass a budget below the cap (e.g. {}).",
                        cap,
                        (cap - 5000).max(cap * 4 / 5)
                    ),
                }
            } else {
                match cfg.lang {
                    Lang::Ja => format!("ホスト応答上限: ~{} トークン — 全階層が上限内です。", cap),
                    Lang::En => format!(
                        "Host response cap: ~{} tokens — all levels fit within the cap.",
                        cap
                    ),
                }
            };
            out.push_str(&format!("{}\n", line));
            // JSON では上限超過でも、テキスト（budget 指定時の形式）なら
            // 全階層が収まる場合はその旨を示す — depth を絞るより情報が多い
            if let Some(tf) = text_full.filter(|tf| any_over && *tf <= cap) {
                let tf_h = crate::fmt::fmt_tokens(tf);
                out.push_str(&format!(
                    "{}\n",
                    match cfg.lang {
                        Lang::Ja => format!(
                            "ヒント: budget を指定すれば全階層が注釈付きテキスト（{}）で収まります — depth を絞る必要はありません。",
                            tf_h
                        ),
                        Lang::En => format!(
                            "Tip: with a budget set, the full annotated tree fits as text ({}) — no need to reduce depth.",
                            tf_h
                        ),
                    }
                ));
            }
        }
        return RunResult {
            stdout: out,
            ..Default::default()
        };
    }

    // ── ファイル単位レポート系モード ──────────────────────────
    if let Some(files) = cfg.stdin_files.clone() {
        let (text, json_val) = crate::report::render_stdin_report(sess, cfg, &files);
        let stdout = match json_val {
            Some(v) => {
                let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
                s.push('\n');
                s
            }
            None => text,
        };
        return RunResult {
            stdout,
            ..Default::default()
        };
    }
    if let Some(focus) = cfg.focus.clone() {
        let (text, json_val, ok) = crate::report::render_focus(sess, cfg, &focus);
        if !ok {
            return early_exit(&text);
        }
        let stdout = match json_val {
            Some(v) => {
                let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
                s.push('\n');
                s
            }
            None => text,
        };
        return RunResult {
            stdout,
            ..Default::default()
        };
    }
    if !cfg.pack.is_empty() {
        let files = cfg.pack.clone();
        let out = crate::report::render_pack(sess, cfg, &files);
        let mut result = RunResult {
            stdout: out,
            ..Default::default()
        };
        if cfg.copy {
            let ok = clip.copy(&strip_ansi(&result.stdout));
            let msg = if ok {
                c(cfg.lang.t().copy_ok, &[BOLD, GREEN], cfg.use_color)
            } else {
                c(cfg.lang.t().copy_fail, &[BOLD, DIM], cfg.use_color)
            };
            result.stderr = format!("{}\n", msg);
        }
        return result;
    }
    if cfg.export_mermaid {
        return RunResult {
            stdout: crate::report::render_mermaid(cfg),
            ..Default::default()
        };
    }
    if cfg.export_dot {
        return RunResult {
            stdout: crate::report::render_dot(cfg),
            ..Default::default()
        };
    }
    if cfg.export_csv {
        return RunResult {
            stdout: crate::report::render_csv(sess, cfg, &active_pats),
            ..Default::default()
        };
    }
    if let Some(r) = cfg.api_diff.clone() {
        return match crate::report::render_api_diff(sess, cfg, git, &active_pats, &r) {
            Ok(out) => RunResult {
                stdout: out,
                ..Default::default()
            },
            Err(msg) => early_exit(&msg),
        };
    }

    // ── JSON ─────────────────────────────────────────────────
    if cfg.json {
        return RunResult {
            stdout: render_json(sess, cfg, &active_pats, &probe),
            ..Default::default()
        };
    }

    // ── HTML ─────────────────────────────────────────────────
    if let Some(html_path) = cfg.html.clone() {
        let content = generate_html(sess, cfg, &active_pats);
        // Windows では Python の text モード書き込み（\n → \r\n 変換）に合わせる
        #[cfg(windows)]
        let content = content.replace('\n', "\r\n");
        let size = content.len() as u64;
        return RunResult {
            stdout: format!(
                "{}\n",
                i18n::html_generated(cfg.lang, &html_path, &fmt_size(size, false))
            ),
            html_file: Some((html_path, content)),
            ..Default::default()
        };
    }

    // ── テキスト出力 ─────────────────────────────────────────
    let (mut text, mut stats) = render_text_with_stats(sess, cfg, &active_pats);

    // --budget N: 出力トークンが予算内に収まるまで深さ→アウトラインの順で削る。
    // 自前の BPE でレンダリング結果そのものを測れるのが dirlens の強み。
    if let Some(budget) = cfg.budget {
        let measure = |s: &str| {
            crate::analysis::text_metrics::count_tokens(s, s.len(), None, false, cfg.tokens_bpe)
        };
        let mut used = measure(&text);
        if used > budget {
            let start_depth = cfg.max_depth.unwrap_or(i64::MAX);
            for d in [6i64, 5, 4, 3, 2, 1] {
                if d >= start_depth {
                    continue;
                }
                cfg.max_depth = Some(d);
                let (t, s) = render_text_with_stats(sess, cfg, &active_pats);
                text = t;
                stats = s;
                used = measure(&text);
                if used <= budget {
                    break;
                }
            }
            if used > budget && cfg.show_outline {
                cfg.show_outline = false;
                let (t, s) = render_text_with_stats(sess, cfg, &active_pats);
                text = t;
                stats = s;
                used = measure(&text);
            }
            // それでも超過するなら解析注釈（TODO・import・git・トークン）を落とし、
            // ツリーの骨格とサマリだけ残す
            if used > budget
                && (cfg.show_todo || cfg.show_imports || cfg.show_git || cfg.show_tokens)
            {
                cfg.show_todo = false;
                cfg.show_imports = false;
                cfg.show_git = false;
                cfg.show_tokens = false;
                cfg.has_extras = cfg.show_tests || cfg.show_entry || cfg.show_config;
                let (t, s) = render_text_with_stats(sess, cfg, &active_pats);
                text = t;
                stats = s;
                used = measure(&text);
            }
        }

        // 最終段: はしごを使い切っても超過するなら、ツリー行そのものを
        // 予算内に収まるまで末尾から間引く（サマリ部は残す）。
        // 「この階層を全て表示するには何トークン必要か」を案内する。
        let mut level_full_cost: Option<i64> = None;
        if used > budget {
            level_full_cost = Some(used);
            // ツリー部（先頭〜最初の空行）とサマリ部（空行以降）に分ける
            let lines: Vec<&str> = text.split('\n').collect();
            let split_at = lines
                .iter()
                .position(|l| l.is_empty())
                .unwrap_or(lines.len());
            let mut tree: Vec<String> =
                lines[..split_at].iter().map(|s| s.to_string()).collect();
            let tail: Vec<String> = lines[split_at..].iter().map(|s| s.to_string()).collect();
            let tail_str = tail.join("\n");
            let omitted_marker = |n: usize| match cfg.lang {
                Lang::Ja => format!("└── … 他 {} エントリ（--budget により省略）", n),
                Lang::En => format!("└── … {} more entries (omitted by --budget)", n),
            };
            let all_tree: Vec<String> = tree.clone();
            let mut omitted = 0usize;
            // ルート行（先頭1行）は必ず残す。1行ずつではなく残超過量に応じて間引く
            while tree.len() > 1 {
                let candidate = format!(
                    "{}\n{}\n{}",
                    tree.join("\n"),
                    omitted_marker(omitted.max(1)),
                    tail_str
                );
                used = measure(&candidate);
                if used <= budget {
                    break;
                }
                // 平均トークン/行から間引き行数を見積もる（最低1行）
                let over = (used - budget).max(1) as usize;
                let per_line = (used as usize / candidate.lines().count().max(1)).max(1);
                let drop = (over / per_line).clamp(1, tree.len() - 1);
                tree.truncate(tree.len() - drop);
                omitted += drop;
            }
            // 見積もりで削りすぎた分を、予算に収まる範囲で1行ずつ戻す
            let mut back_steps = 0;
            while omitted > 0 && back_steps < 64 {
                let candidate = format!(
                    "{}\n{}\n{}\n{}",
                    tree.join("\n"),
                    all_tree[tree.len()],
                    omitted_marker(omitted - 1),
                    tail_str
                );
                if measure(&candidate) > budget {
                    break;
                }
                tree.push(all_tree[tree.len()].clone());
                omitted -= 1;
                back_steps += 1;
            }
            if omitted > 0 {
                text = format!(
                    "{}\n{}\n{}",
                    tree.join("\n"),
                    omitted_marker(omitted),
                    tail_str
                );
                used = measure(&text);
            }
        }

        let depth_note = match cfg.max_depth {
            Some(d) => format!(", depth={}", d),
            None => String::new(),
        };
        let full_note = match (level_full_cost, cfg.lang) {
            (Some(f), Lang::Ja) => {
                format!("。この階層を全て表示するには ~{} tok 必要", f)
            }
            (Some(f), Lang::En) => {
                format!("; showing this level fully needs ~{} tok", f)
            }
            (None, _) => String::new(),
        };
        text.push_str(&format!(
            "{}\n",
            c(
                &match cfg.lang {
                    Lang::Ja => format!(
                        "  (--budget {} に調整: ~{} tok{}{})",
                        budget, used, depth_note, full_note
                    ),
                    Lang::En => format!(
                        "  (fitted to --budget {}: ~{} tok{}{})",
                        budget, used, depth_note, full_note
                    ),
                },
                &[DIM],
                cfg.use_color
            )
        ));
    }

    let mut result = RunResult {
        stdout: text,
        ..Default::default()
    };

    // 機密の可能性があるファイルの警告（--ai / -C でクリップボードへ送る時のみ）。
    // compat モード（Python 版とのバイト一致検証）では出さない。
    if cfg.copy && !cfg.suppress_notes && !stats.sensitive.is_empty() {
        let shown: Vec<&str> = stats.sensitive.iter().take(5).map(|s| s.as_str()).collect();
        let more = stats.sensitive.len().saturating_sub(shown.len());
        let list = if more > 0 {
            format!("{} (+{})", shown.join(", "), more)
        } else {
            shown.join(", ")
        };
        let warn = match cfg.lang {
            Lang::Ja => format!(
                "⚠ コピーした出力に機密の可能性があるファイル名が含まれています: {}",
                list
            ),
            Lang::En => format!(
                "⚠ copied output includes potentially sensitive files: {}",
                list
            ),
        };
        result
            .stderr
            .push_str(&format!("{}\n", c(&warn, &[BOLD, YELLOW], cfg.use_color)));
    }

    if cfg.copy {
        let ok = clip.copy(&strip_ansi(&result.stdout));
        let msg = if ok {
            c(cfg.lang.t().copy_ok, &[BOLD, GREEN], cfg.use_color)
        } else {
            c(cfg.lang.t().copy_fail, &[BOLD, DIM], cfg.use_color)
        };
        result.stderr.push_str(&format!("{}\n", msg));
    }
    result
}

/// prepare + execute の一括呼び出し（プリフェッチ無し・wasm 等の単純経路用）。
pub fn run<F: FsProvider>(
    mut args: Args,
    fs: &F,
    git: &dyn GitProvider,
    clip: &dyn ClipboardProvider,
    use_color_hint: bool,
) -> RunResult {
    args.merge_aliases();
    let mut cfg = match prepare(&args, fs, use_color_hint) {
        Ok(cfg) => cfg,
        Err(res) => return res,
    };
    let mut sess = Session::new(fs);
    execute(&mut sess, &mut cfg, git, clip)
}
