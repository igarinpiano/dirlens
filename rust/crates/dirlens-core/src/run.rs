//! 実行オーケストレーション（dirlens.py の main() の後半相当）。
//!
//! I/O（stdout/stderr への書き出し・HTML ファイルの書き込み）は呼び出し側
//! （CLI / wasm ホスト）が行う。コアは文字列と終了コードを返すだけ。

use std::sync::Arc;

use crate::analysis::gitlog::load_git_log;
use crate::analysis::gitstatus::{build_since_set, parse_status_porcelain};
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
            return Err(early_exit(&i18n::err_not_dir(lang, &args.path)))
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
    if cfg.show_status || cfg.since.is_some() {
        if let Some(out) = git.status_output(&cfg.root) {
            cfg.status_map = parse_status_porcelain(&out);
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
        let (set, marks, deleted) = build_since_set(diff.as_deref(), status.as_deref());
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
        let (map, counts) = load_git_log(git, &cfg.root);
        cfg.git_map = map;
        cfg.git_change_counts = counts;
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
        let depth_note = match cfg.max_depth {
            Some(d) => format!(", depth={}", d),
            None => String::new(),
        };
        text.push_str(&format!(
            "{}\n",
            c(
                &match cfg.lang {
                    Lang::Ja => format!(
                        "  (--budget {} に調整: ~{} tok{})",
                        budget, used, depth_note
                    ),
                    Lang::En => format!(
                        "  (fitted to --budget {}: ~{} tok{})",
                        budget, used, depth_note
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
