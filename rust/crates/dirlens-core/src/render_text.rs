//! テキスト（ツリー）レンダラ。dirlens.py の render() とテキスト出力部の等価移植。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;

use crate::analysis::extras::{file_extras, reading_order_candidates};
use crate::cfg::Cfg;
use crate::colors::{c, BLUE, BOLD, CYAN, DIM, GREEN, MAGENTA, RED};
use crate::emoji::get_emoji;
use crate::filter::{count_entries, filter_entries, has_content, sort_entries};
use crate::fmt::{
    filemode, fmt_bar, fmt_count, fmt_date, fmt_git, fmt_outline, fmt_size, fmt_tokens, splitext,
};
use crate::gitignore::{extend_pats, relpath_slash};
use crate::provider::{Entry, FsProvider};
use crate::session::Session;

const PIPE: &str = "│   ";
const FORK: &str = "├── ";
const LAST: &str = "└── ";
const BLANK: &str = "    ";

#[derive(Debug, Default)]
pub struct TextStats {
    pub files: u64,
    pub dirs: u64,
    pub extensions: IndexMap<String, u64>,
    pub tokens: i64,
    pub todo_total: u64,
    pub todo_samples: Vec<(String, usize, String, String)>,
}

fn fmt_size_i(n: i64) -> String {
    if n < 0 {
        format!("{} bytes", n)
    } else {
        fmt_size(n as u64, false)
    }
}

/// fmt_perm_info 相当。
fn perm_info<F: FsProvider>(sess: &Session<F>, entry: &Entry, cfg: &Cfg) -> String {
    if !cfg.show_perms && !cfg.show_user && !cfg.show_group {
        return String::new();
    }
    let st = match sess.fs.stat(&entry.path, false) {
        Some(st) => st,
        None => return String::new(),
    };
    let mut parts: Vec<String> = Vec::new();
    if cfg.show_perms {
        parts.push(filemode(st.mode));
    }
    if cfg.show_user {
        parts.push(
            sess.fs
                .user_name(st.uid)
                .unwrap_or_else(|| st.uid.to_string()),
        );
    }
    if cfg.show_group {
        parts.push(
            sess.fs
                .group_name(st.gid)
                .unwrap_or_else(|| st.gid.to_string()),
        );
    }
    if parts.is_empty() {
        String::new()
    } else {
        c(&format!("[{}] ", parts.join(" ")), &[DIM], cfg.use_color)
    }
}

fn esz<F: FsProvider>(sess: &Session<F>, e: &Entry) -> u64 {
    sess.fs.stat(&e.path, true).map(|s| s.size).unwrap_or(0)
}

fn emtime<F: FsProvider>(sess: &Session<F>, e: &Entry) -> f64 {
    sess.fs.stat(&e.path, true).map(|s| s.mtime).unwrap_or(0.0)
}

#[allow(clippy::too_many_arguments)]
fn render_node<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    prefix: &str,
    depth: i64,
    cfg: &Cfg,
    stats: &mut TextStats,
    active_pats: &Arc<Vec<String>>,
    seen: Option<&HashSet<PathBuf>>,
    out: &mut String,
) {
    if let Some(md) = cfg.max_depth {
        if depth >= md {
            return;
        }
    }

    let mut seen_owned: Option<HashSet<PathBuf>> = None;
    if cfg.follow_syms {
        let mut s = seen.cloned().unwrap_or_default();
        let real = sess.fs.real_path(path);
        if s.contains(&real) {
            out.push_str(&format!(
                "{}{}{}\n",
                prefix,
                LAST,
                c("[循環リンク]", &[DIM], cfg.use_color)
            ));
            return;
        }
        s.insert(real);
        seen_owned = Some(s);
    }
    let seen_ref = seen_owned.as_ref();

    let cur_pats = extend_pats(sess, active_pats, path, cfg);
    let (mut dirs, mut files) = match filter_entries(sess, path, cfg, &cur_pats) {
        Some(v) => v,
        None => {
            out.push_str(&format!(
                "{}{}{}\n",
                prefix,
                LAST,
                c("[アクセス拒否]", &[BOLD, RED], cfg.use_color)
            ));
            return;
        }
    };

    if cfg.prune {
        dirs.retain(|d| has_content(sess, &d.path, depth + 1, cfg, &cur_pats));
    }

    sort_entries(sess, &mut dirs, &mut files, cfg);
    let combined: Vec<Entry> = if cfg.files_first {
        files.into_iter().chain(dirs).collect()
    } else {
        dirs.into_iter().chain(files).collect()
    };
    let (cur_dir_size, _) = sess.dir_size(path);

    let n = combined.len();
    for (i, entry) in combined.iter().enumerate() {
        let is_last = i == n - 1;
        let branch = if is_last { LAST } else { FORK };
        let cont = if is_last { BLANK } else { PIPE };

        let sym_target = if entry.is_symlink {
            match sess.fs.read_link(&entry.path) {
                Some(t) => format!(" → {}", t),
                None => " →".to_string(),
            }
        } else {
            String::new()
        };

        let display = if cfg.full_path {
            format!("./{}", relpath_slash(&entry.path, &cfg.root))
        } else {
            entry.name.clone()
        };

        let perm_prefix = perm_info(sess, entry, cfg);

        let is_dir_entry = entry.is_dir_nofollow
            || (cfg.follow_syms && entry.is_symlink && entry.is_dir_follow);

        if is_dir_entry {
            let (sz, sz_err) = sess.dir_size(&entry.path);
            let (nd, nf, denied) = count_entries(sess, &entry.path, cfg, &cur_pats);
            stats.dirs += 1;

            let emoji = if cfg.show_emoji {
                format!("{} ", get_emoji(&entry.name, true))
            } else {
                String::new()
            };
            let mut parts = vec![fmt_count(nd, nf, denied), fmt_size(sz, sz_err)];
            if cfg.show_date {
                if let Some(st) = sess.fs.stat(&entry.path, false) {
                    parts.push(fmt_date(sess.fs.now(), st.mtime));
                }
            }
            let bar = if cfg.show_bar && cur_dir_size != 0 {
                format!(" {}", fmt_bar(sz, cur_dir_size, 10))
            } else {
                String::new()
            };

            let name = c(
                &format!("{}{}{}/", emoji, display, sym_target),
                &[BOLD, CYAN],
                cfg.use_color,
            );
            let meta = c(
                &format!("({}){}", parts.join(", "), bar),
                &[DIM],
                cfg.use_color,
            );
            out.push_str(&format!("{}{}{}{} {}\n", prefix, branch, perm_prefix, name, meta));
            render_node(
                sess,
                &entry.path,
                &format!("{}{}", prefix, cont),
                depth + 1,
                cfg,
                stats,
                &cur_pats,
                seen_ref,
                out,
            );
        } else {
            let sz = esz(sess, entry);
            stats.files += 1;
            let (_, ext_raw) = splitext(&entry.name);
            let ext = ext_raw.to_lowercase();
            let ext_key = if ext.is_empty() {
                "(no ext)".to_string()
            } else {
                ext
            };
            *stats.extensions.entry(ext_key).or_insert(0) += 1;

            let rel = relpath_slash(&entry.path, &cfg.root);
            let extras = if cfg.has_extras {
                file_extras(sess, entry, &rel, cfg)
            } else {
                Default::default()
            };

            let entry_mark = if extras.is_entry {
                if cfg.show_emoji {
                    "🎯 "
                } else {
                    "* "
                }
            } else {
                ""
            };
            let config_mark = if extras.is_config && entry_mark.is_empty() {
                "⚙ "
            } else {
                ""
            };

            // 注意: dirlens.py はここでファイル用の絵文字を計算するが、名前の表示には
            // 使っていない（entry_mark の 🎯 のみ）。ゴールデン一致のため同じ挙動にする。
            let mut parts = vec![fmt_size(sz, false)];
            if cfg.show_date {
                let mt = emtime(sess, entry);
                if mt != 0.0 {
                    parts.push(fmt_date(sess.fs.now(), mt));
                }
            }

            if cfg.show_tokens {
                if let Some(tok) = extras.tokens {
                    parts.push(fmt_tokens(tok));
                    stats.tokens += tok;
                    if let Some(lines) = extras.lines {
                        parts.push(format!("{} lines", lines));
                    }
                }
            }

            if cfg.show_git {
                if let Some(g) = &extras.git {
                    parts.push(fmt_git(g));
                }
            }

            if cfg.show_todo && !extras.todos.is_empty() {
                let n_todo = extras.todos.len();
                parts.push(format!("TODO×{}", n_todo));
                stats.todo_total += n_todo as u64;
                for item in extras.todos.iter().take(3) {
                    if stats.todo_samples.len() < 20 {
                        stats
                            .todo_samples
                            .push((rel.clone(), item.0, item.1.clone(), item.2.clone()));
                    }
                }
            }

            if cfg.show_tests && extras.no_test {
                parts.push("テスト無し".to_string());
            }

            if cfg.show_config && extras.is_config {
                parts.push("config".to_string());
            }

            if cfg.show_outline {
                if let Some(outline) = &extras.outline {
                    if !outline.is_empty() {
                        if let Some(ostr) = fmt_outline(outline, 5) {
                            parts.push(ostr);
                        }
                    }
                }
            }

            if cfg.show_imports {
                let imp_n = extras.imports.len();
                let used_n = extras.imported_by.len();
                if imp_n > 0 {
                    parts.push(format!("imports×{}", imp_n));
                }
                if used_n > 0 {
                    parts.push(format!("used-by×{}", used_n));
                }
            }

            let bar = if cfg.show_bar && cur_dir_size != 0 {
                format!(" {}", fmt_bar(sz, cur_dir_size, 10))
            } else {
                String::new()
            };

            let name = c(
                &format!("{}{}{}{}", entry_mark, config_mark, display, sym_target),
                &[if entry.is_symlink { MAGENTA } else { GREEN }],
                cfg.use_color,
            );
            let meta = c(
                &format!("({}){}", parts.join(", "), bar),
                &[DIM],
                cfg.use_color,
            );
            out.push_str(&format!("{}{}{}{} {}\n", prefix, branch, perm_prefix, name, meta));
        }
    }
}

/// テキスト出力全体（markdown フェンス・ルート行・ツリー・サマリ）。
pub fn render_text<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    let mut out = String::new();
    let color = cfg.use_color;

    if cfg.markdown {
        out.push_str("```\n");
    }

    let (root_sz, root_sz_err) = sess.dir_size(&cfg.root);
    let (root_nd, root_nf, root_denied) = count_entries(sess, &cfg.root, cfg, active_pats);

    let mut root_parts = vec![
        fmt_count(root_nd, root_nf, root_denied),
        fmt_size(root_sz, root_sz_err),
    ];
    if cfg.show_date {
        if let Some(st) = sess.fs.stat(&cfg.root, true) {
            root_parts.push(fmt_date(sess.fs.now(), st.mtime));
        }
    }

    let root_emoji = if cfg.show_emoji {
        format!("{} ", get_emoji(&cfg.root_label, true))
    } else {
        String::new()
    };
    out.push_str(&format!(
        "{} {}\n",
        c(
            &format!("{}{}/", root_emoji, cfg.root_label),
            &[BOLD, BLUE],
            color
        ),
        c(&format!("({})", root_parts.join(", ")), &[DIM], color)
    ));

    let mut stats = TextStats::default();
    render_node(sess, &cfg.root, "", 0, cfg, &mut stats, active_pats, None, &mut out);

    out.push('\n');
    let mut summary = format!("  合計  {} ディレクトリ", stats.dirs);
    if !cfg.dirs_only {
        summary += &format!(",  {} ファイル", stats.files);
    }
    if cfg.use_gitignore {
        summary += "  (.gitignore 適用済み)";
    }
    if let Some(te) = &cfg.type_ext {
        summary += &format!("  (フィルタ: {})", te);
    }
    if !cfg.excludes.is_empty() {
        summary += &format!("  (除外: {})", cfg.excludes.join(", "));
    }
    if !cfg.includes.is_empty() {
        summary += &format!("  (抽出: {})", cfg.includes.join(", "));
    }
    if let Some(ms) = cfg.min_size {
        if ms != 0 {
            summary += &format!("  (最小: {})", fmt_size_i(ms));
        }
    }
    if let Some(ms) = cfg.max_size {
        if ms != 0 {
            summary += &format!("  (最大: {})", fmt_size_i(ms));
        }
    }
    if cfg.prune {
        summary += "  (剪定済み)";
    }
    if cfg.dirs_only {
        summary += "  (ディレクトリのみ)";
    }
    out.push_str(&format!("{}\n", c(&summary, &[DIM], color)));

    if !cfg.dirs_only && !stats.extensions.is_empty() {
        let mut exts: Vec<(&String, &u64)> = stats.extensions.iter().collect();
        exts.sort_by(|a, b| b.1.cmp(a.1)); // 安定ソート: タイは出現順
        let line = exts
            .iter()
            .take(8)
            .map(|(e, n)| format!("{} ×{}", e, n))
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(&format!("{}\n", c(&format!("  {}", line), &[DIM], color)));
    }

    if cfg.show_tokens {
        out.push_str(&format!(
            "{}\n",
            c(
                &format!("  推定トークン数: {}", fmt_tokens(stats.tokens)),
                &[DIM],
                color
            )
        ));
    }

    if cfg.show_todo {
        if stats.todo_total > 0 {
            out.push_str(&format!(
                "{}\n",
                c(
                    &format!("  TODO/FIXME等: {}件", stats.todo_total),
                    &[DIM],
                    color
                )
            ));
            for (rel, ln, kind, snippet) in stats.todo_samples.iter().take(8) {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!("    {}:{} [{}] {}", rel, ln, kind, snippet),
                        &[DIM],
                        color
                    )
                ));
            }
            let shown = std::cmp::min(stats.todo_samples.len(), 8) as u64;
            if stats.todo_total > shown {
                let rest = stats.todo_total - shown;
                out.push_str(&format!(
                    "{}\n",
                    c(&format!("    …他 {} 件", rest), &[DIM], color)
                ));
            }
        } else {
            out.push_str(&format!("{}\n", c("  TODO/FIXME等: 0件", &[DIM], color)));
        }
    }

    if cfg.show_tests {
        out.push_str(&format!(
            "{}\n",
            c(
                &format!("  テスト未整備: {} ファイル", cfg.untested_set.len()),
                &[DIM],
                color
            )
        ));
    }

    if cfg.show_entry {
        out.push_str(&format!(
            "{}\n",
            c(
                &format!("  エントリーポイント候補: {} 件検出", cfg.entry_set.len()),
                &[DIM],
                color
            )
        ));
    }

    if cfg.show_config {
        out.push_str(&format!(
            "{}\n",
            c(
                &format!("  設定ファイル: {} 件検出", cfg.config_set.len()),
                &[DIM],
                color
            )
        ));
    }

    if cfg.show_imports && !cfg.imported_by_map.is_empty() {
        let mut items: Vec<(&String, usize)> = cfg
            .imported_by_map
            .iter()
            .map(|(k, v)| (k, v.len()))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        out.push_str(&format!(
            "{}\n",
            c(
                "  依存度が高いファイル（多くのファイルから参照されている）:",
                &[DIM],
                color
            )
        ));
        for (relpath, n) in items.into_iter().take(5) {
            out.push_str(&format!(
                "{}\n",
                c(&format!("    {}  (used by {})", relpath, n), &[DIM], color)
            ));
        }
    }

    if cfg.show_imports && !cfg.cycles.is_empty() {
        out.push_str(&format!(
            "{}\n",
            c(
                &format!("  循環依存: {} 件検出", cfg.cycles.len()),
                &[DIM],
                color
            )
        ));
        for cycle in cfg.cycles.iter().take(5) {
            out.push_str(&format!(
                "{}\n",
                c(&format!("    {}", cycle.join(" → ")), &[DIM], color)
            ));
        }
        if cfg.cycles.len() > 5 {
            out.push_str(&format!(
                "{}\n",
                c(
                    &format!("    …他 {} 件", cfg.cycles.len() - 5),
                    &[DIM],
                    color
                )
            ));
        }
    }

    if cfg.show_git && !cfg.git_change_counts.is_empty() {
        let mut items: Vec<(&String, u64)> = cfg
            .git_change_counts
            .iter()
            .map(|(k, v)| (k, *v))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        let top_hot: Vec<_> = items.into_iter().take(5).collect();
        if !top_hot.is_empty() && top_hot[0].1 > 1 {
            out.push_str(&format!(
                "{}\n",
                c("  変更頻度が高いファイル（直近の履歴内）:", &[DIM], color)
            ));
            for (relpath, n) in top_hot {
                out.push_str(&format!(
                    "{}\n",
                    c(&format!("    {}  ({} 回変更)", relpath, n), &[DIM], color)
                ));
            }
        }
    }

    if cfg.show_entry
        && cfg.show_imports
        && (!cfg.entry_set.is_empty() || !cfg.imported_by_map.is_empty())
    {
        let candidates = reading_order_candidates(cfg, 3, 5);
        if !candidates.is_empty() {
            out.push_str(&format!(
                "{}\n",
                c(
                    "  読み始めの候補（エントリーポイント→依存度の高い順）:",
                    &[DIM],
                    color
                )
            ));
            for (i, p) in candidates.iter().enumerate() {
                out.push_str(&format!(
                    "{}\n",
                    c(&format!("    {}. {}", i + 1, p), &[DIM], color)
                ));
            }
        }
    }

    if cfg.show_git && cfg.git_map.is_empty() {
        out.push_str(&format!(
            "{}\n",
            c(
                "  (gitリポジトリではないか、git未インストールのためコミット情報は取得できませんでした)",
                &[DIM],
                color
            )
        ));
    }

    if cfg.markdown {
        out.push_str("```\n");
    }

    out
}
