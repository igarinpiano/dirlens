//! 実行オーケストレーション（dirlens.py の main() の後半相当）。
//!
//! I/O（stdout/stderr への書き出し・HTML ファイルの書き込み）は呼び出し側
//! （CLI / wasm ホスト）が行う。コアは文字列と終了コードを返すだけ。

use std::sync::Arc;

use crate::analysis::gitlog::load_git_log;
use crate::analysis::index::build_project_index;
use crate::args::Args;
use crate::cfg::Cfg;
use crate::colors::{c, strip_ansi, BOLD, DIM, GREEN};
use crate::fmt::fmt_size;
use crate::provider::{ClipboardProvider, FsProvider, GitProvider};
use crate::render_html::generate_html;
use crate::render_json::render_json;
use crate::render_text::render_text;
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

    let target = match fs.resolve(&args.path) {
        Some(t) => t,
        None => {
            return Err(RunResult {
                stderr: "エラー: 現在のディレクトリへのアクセス権限がありません。\n\
                         絶対パスを明示的に指定してください（例: dirlens /path/to/project）。\n"
                    .to_string(),
                exit_code: 1,
                ..Default::default()
            })
        }
    };

    let st = fs.stat(&target, true);
    match st {
        None => {
            return Err(early_exit(&format!(
                "エラー: '{}' が見つかりません",
                args.path
            )))
        }
        Some(st) if st.mode & 0o170000 != 0o040000 => {
            return Err(early_exit(&format!(
                "エラー: '{}' はディレクトリではありません",
                args.path
            )))
        }
        _ => {}
    }

    let root_label = target
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string_lossy().into_owned());

    Cfg::from_args(args, target, root_label, use_color)
        .map_err(|msg| early_exit(&format!("エラー: {}", msg)))
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
    if cfg.show_git {
        let (map, counts) = load_git_log(git, &cfg.root);
        cfg.git_map = map;
        cfg.git_change_counts = counts;
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
        let size = content.len() as u64;
        return RunResult {
            stdout: format!(
                "✓ {} を生成しました ({})\n",
                html_path,
                fmt_size(size, false)
            ),
            html_file: Some((html_path, content)),
            ..Default::default()
        };
    }

    // ── テキスト出力 ─────────────────────────────────────────
    let text = render_text(sess, cfg, &active_pats);
    let mut result = RunResult {
        stdout: text,
        ..Default::default()
    };
    if cfg.copy {
        let ok = clip.copy(&strip_ansi(&result.stdout));
        let msg = if ok {
            c("✓ クリップボードにコピーしました", &[BOLD, GREEN], cfg.use_color)
        } else {
            c(
                "✗ コピー失敗 (pbcopy / xclip / wl-copy が必要)",
                &[BOLD, DIM],
                cfg.use_color,
            )
        };
        result.stderr = format!("{}\n", msg);
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
