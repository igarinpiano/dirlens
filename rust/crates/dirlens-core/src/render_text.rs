//! テキスト（ツリー）レンダラ。dirlens.py の render() とテキスト出力部の等価移植。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;

use crate::analysis::extras::{file_extras, reading_order_candidates};
use crate::cfg::{Cfg, Heat};
use crate::colors::{c, fg256, BLUE, BOLD, CYAN, DIM, GREEN, MAGENTA, RED, YELLOW};
use crate::emoji::get_emoji;
use crate::filter::{count_entries, filter_entries, has_content, sort_entries};
use crate::fmt::{
    filemode, fmt_bar, fmt_count, fmt_date, fmt_git, fmt_outline, fmt_size, fmt_tokens,
    sanitize_ctrl, splitext,
};
use crate::gitignore::{extend_pats, relpath_slash};
use crate::i18n;
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
    /// 機密の可能性があるファイル（--ai / -C の警告用）
    pub sensitive: Vec<String>,
    /// 拡張子 → (ファイル数, 行数, トークン数)。-T 時のみ集計。
    pub lang_stats: IndexMap<String, (u64, i64, i64)>,
    /// 長大関数の候補（行数, "rel:name"）。-O 時のみ集計。
    pub long_funcs: Vec<(u32, String)>,
}

/// AI チャットへ貼り付ける際に警告すべき「機密の可能性が高い」ファイル名か。
pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == ".env" || (lower.starts_with(".env.") && !lower.contains("example") && !lower.contains("sample")) {
        return true;
    }
    let (_, ext) = splitext(&lower);
    if matches!(ext, ".pem" | ".key" | ".p12" | ".pfx" | ".keystore" | ".jks") {
        return true;
    }
    if lower.starts_with("id_rsa") || lower.starts_with("id_ed25519") || lower.starts_with("id_ecdsa") {
        return true;
    }
    matches!(
        lower.as_str(),
        "credentials.json" | "service-account.json" | ".npmrc" | ".pypirc" | ".netrc" | ".htpasswd"
    ) || lower.contains("secret")
}

/// --heat のグラデーション色（256色コード）。熱いほど赤い。
fn heat_color(cfg: &Cfg, now: f64, mtime: f64, size: u64, rel: &str) -> Option<String> {
    let heat = cfg.heat?;
    if !cfg.use_color {
        return None;
    }
    let code: u8 = match heat {
        Heat::Age => {
            let age = (now - mtime).max(0.0);
            if age < 3600.0 { 196 }
            else if age < 86_400.0 { 202 }
            else if age < 604_800.0 { 208 }
            else if age < 2_592_000.0 { 214 }
            else if age < 15_552_000.0 { 220 }
            else { 245 }
        }
        Heat::Size => {
            if size >= 100 * 1024 * 1024 { 196 }
            else if size >= 10 * 1024 * 1024 { 202 }
            else if size >= 1024 * 1024 { 208 }
            else if size >= 100 * 1024 { 214 }
            else if size >= 10 * 1024 { 220 }
            else { 245 }
        }
        Heat::Churn => {
            let n = cfg.git_change_counts.get(rel).copied().unwrap_or(0);
            if n >= 20 { 196 }
            else if n >= 10 { 202 }
            else if n >= 5 { 208 }
            else if n >= 3 { 214 }
            else if n >= 2 { 220 }
            else { 245 }
        }
    };
    Some(fg256(code))
}

/// --status マークの色（porcelain XY コード別）。
fn status_color(xy: &str) -> &'static str {
    if xy == "??" {
        RED
    } else if xy.starts_with('A') || xy.ends_with('A') {
        GREEN
    } else if xy.starts_with('R') || xy.starts_with('C') {
        MAGENTA
    } else {
        YELLOW
    }
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
                c(cfg.lang.t().cyclic_link, &[DIM], cfg.use_color)
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
                c(cfg.lang.t().access_denied, &[BOLD, RED], cfg.use_color)
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

        // 名前・リンク先は攻撃者制御になりうる（clone したリポジトリ等）。
        // 制御文字を無害化してエスケープ注入・行偽装を防ぐ。
        let sym_target = if entry.is_symlink {
            match sess.fs.read_link(&entry.path) {
                Some(t) => format!(" → {}", sanitize_ctrl(&t)),
                None => " →".to_string(),
            }
        } else {
            String::new()
        };

        let display = if cfg.full_path {
            sanitize_ctrl(&format!("./{}", relpath_slash(&entry.path, &cfg.root)))
        } else {
            sanitize_ctrl(&entry.name)
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
                    parts.push(fmt_date(sess.fs.now(), st.mtime, cfg.lang));
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
            *stats.extensions.entry(ext_key.clone()).or_insert(0) += 1;

            let rel = relpath_slash(&entry.path, &cfg.root);
            let extras = if cfg.has_extras {
                file_extras(sess, entry, &rel, cfg)
            } else {
                Default::default()
            };

            if is_sensitive_name(&entry.name) && stats.sensitive.len() < 20 {
                stats.sensitive.push(rel.clone());
            }

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
                    parts.push(fmt_date(sess.fs.now(), mt, cfg.lang));
                }
            }

            if cfg.show_tokens {
                if let Some(tok) = extras.tokens {
                    parts.push(fmt_tokens(tok));
                    stats.tokens += tok;
                    if let Some(lines) = extras.lines {
                        parts.push(format!("{} lines", lines));
                    }
                    let agg = stats
                        .lang_stats
                        .entry(ext_key.clone())
                        .or_insert((0, 0, 0));
                    agg.0 += 1;
                    agg.1 += extras.lines.unwrap_or(0);
                    agg.2 += tok;
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
                parts.push(cfg.lang.t().no_test.to_string());
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
                        // 長大関数の収集（--agent サマリ / JSON 用）
                        for it in outline {
                            if let Some((a, b)) = it.span {
                                if it.kind != "class" && it.kind != "struct" && b > a {
                                    stats.long_funcs.push((b - a + 1, format!("{}:{}", rel, it.name)));
                                }
                            }
                        }
                        if stats.long_funcs.len() > 512 {
                            stats.long_funcs.sort_by(|x, y| y.0.cmp(&x.0));
                            stats.long_funcs.truncate(16);
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

            // --status / --since のマーク（[M] / [??] / [A] 等）
            let mut vcs_mark = String::new();
            if cfg.show_status {
                if let Some(xy) = cfg.status_map.get(&rel) {
                    let xy_disp = xy.trim();
                    vcs_mark = format!(
                        "{} ",
                        c(&format!("[{}]", xy_disp), &[BOLD, status_color(xy)], cfg.use_color)
                    );
                }
            } else if cfg.since.is_some() {
                if let Some(ch) = cfg.since_status.get(&rel) {
                    let col = match ch {
                        'A' => GREEN,
                        'R' => MAGENTA,
                        _ => YELLOW,
                    };
                    vcs_mark = format!(
                        "{} ",
                        c(&format!("[{}]", ch), &[BOLD, col], cfg.use_color)
                    );
                }
            }

            // --heat: ファイル名の色をグラデーションで上書き
            let heat_code =
                heat_color(cfg, sess.fs.now(), emtime(sess, entry), sz, &rel);
            let name_color: &str = match &heat_code {
                Some(code) => code.as_str(),
                None => {
                    if entry.is_symlink {
                        MAGENTA
                    } else {
                        GREEN
                    }
                }
            };

            let name = c(
                &format!("{}{}{}{}", entry_mark, config_mark, display, sym_target),
                &[name_color],
                cfg.use_color,
            );
            let meta = c(
                &format!("({}){}", parts.join(", "), bar),
                &[DIM],
                cfg.use_color,
            );
            out.push_str(&format!(
                "{}{}{}{}{} {}\n",
                prefix, branch, perm_prefix, vcs_mark, name, meta
            ));
        }
    }
}

/// テキスト出力全体（markdown フェンス・ルート行・ツリー・サマリ）。
pub fn render_text<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    render_text_with_stats(sess, cfg, active_pats).0
}

/// render_text の本体。統計（機密ファイル検出など）も返す。
pub fn render_text_with_stats<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> (String, TextStats) {
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
            root_parts.push(fmt_date(sess.fs.now(), st.mtime, cfg.lang));
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
            &format!("{}{}/", root_emoji, sanitize_ctrl(&cfg.root_label)),
            &[BOLD, BLUE],
            color
        ),
        c(&format!("({})", root_parts.join(", ")), &[DIM], color)
    ));

    let mut stats = TextStats::default();
    render_node(sess, &cfg.root, "", 0, cfg, &mut stats, active_pats, None, &mut out);

    out.push('\n');
    let lang = cfg.lang;
    let t = lang.t();
    let mut summary = i18n::summary_total_dirs(lang, stats.dirs);
    if !cfg.dirs_only {
        summary += &i18n::summary_files(lang, stats.files);
    }
    if cfg.use_gitignore {
        summary += &format!("  {}", t.gitignore_applied);
    }
    if let Some(te) = &cfg.type_ext {
        summary += &i18n::filter_note(lang, te);
    }
    if !cfg.excludes.is_empty() {
        summary += &i18n::exclude_note(lang, &cfg.excludes.join(", "));
    }
    if !cfg.includes.is_empty() {
        summary += &i18n::include_note(lang, &cfg.includes.join(", "));
    }
    if let Some(ms) = cfg.min_size {
        if ms != 0 {
            summary += &i18n::min_note(lang, &fmt_size_i(ms));
        }
    }
    if let Some(ms) = cfg.max_size {
        if ms != 0 {
            summary += &i18n::max_note(lang, &fmt_size_i(ms));
        }
    }
    if cfg.prune {
        summary += &format!("  {}", t.pruned);
    }
    if cfg.dirs_only {
        summary += &format!("  {}", t.dirs_only);
    }
    out.push_str(&format!("{}\n", c(&summary, &[DIM], color)));

    if !cfg.dirs_only && !stats.extensions.is_empty() {
        let mut exts: Vec<(&String, &u64)> = stats.extensions.iter().collect();
        exts.sort_by(|a, b| b.1.cmp(a.1)); // 安定ソート: タイは出現順
        let line = exts
            .iter()
            .take(8)
            .map(|(e, n)| format!("{} ×{}", sanitize_ctrl(e), n))
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(&format!("{}\n", c(&format!("  {}", line), &[DIM], color)));
    }

    if cfg.show_tokens {
        out.push_str(&format!(
            "{}\n",
            c(
                &i18n::estimated_tokens(lang, &fmt_tokens(stats.tokens)),
                &[DIM],
                color
            )
        ));
        // 言語別統計（compat モードでは出さない・Python 版に無い機能）
        if !cfg.suppress_notes && stats.lang_stats.len() > 1 {
            let mut items: Vec<(&String, &(u64, i64, i64))> =
                stats.lang_stats.iter().collect();
            items.sort_by(|a, b| b.1 .2.cmp(&a.1 .2));
            out.push_str(&format!(
                "{}\n",
                c(
                    i18n::tr(lang, "  Tokens by file type:", "  拡張子別トークン:"),
                    &[DIM],
                    color
                )
            ));
            for (ext, (files, lines, tokens)) in items.into_iter().take(6) {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!(
                            "    {} ×{}  {}  {} lines",
                            sanitize_ctrl(ext),
                            files,
                            fmt_tokens(*tokens),
                            lines
                        ),
                        &[DIM],
                        color
                    )
                ));
            }
        }
    }

    // --since: 削除されたファイル（ツリーには現れないため一覧で出す）
    if let Some(since_ref) = &cfg.since {
        out.push_str(&format!(
            "{}\n",
            c(
                &match lang {
                    i18n::Lang::Ja => format!("  {} 以降の変更のみ表示", since_ref),
                    i18n::Lang::En => format!("  showing only changes since {}", since_ref),
                },
                &[DIM],
                color
            )
        ));
        if !cfg.since_deleted.is_empty() {
            out.push_str(&format!(
                "{}\n",
                c(
                    &match lang {
                        i18n::Lang::Ja =>
                            format!("  削除されたファイル: {} 件", cfg.since_deleted.len()),
                        i18n::Lang::En =>
                            format!("  deleted files: {}", cfg.since_deleted.len()),
                    },
                    &[DIM],
                    color
                )
            ));
            for d in cfg.since_deleted.iter().take(10) {
                out.push_str(&format!(
                    "{}\n",
                    c(&format!("    - {}", sanitize_ctrl(d)), &[DIM], color)
                ));
            }
            if cfg.since_deleted.len() > 10 {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &i18n::more_items(lang, (cfg.since_deleted.len() - 10) as u64),
                        &[DIM],
                        color
                    )
                ));
            }
        }
    }

    if cfg.show_todo {
        if stats.todo_total > 0 {
            out.push_str(&format!(
                "{}\n",
                c(&i18n::todo_count(lang, stats.todo_total), &[DIM], color)
            ));
            for (rel, ln, kind, snippet) in stats.todo_samples.iter().take(8) {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!(
                            "    {}:{} [{}] {}",
                            sanitize_ctrl(rel),
                            ln,
                            kind,
                            sanitize_ctrl(snippet)
                        ),
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
                    c(&i18n::more_items(lang, rest), &[DIM], color)
                ));
            }
        } else {
            out.push_str(&format!(
                "{}\n",
                c(&i18n::todo_count(lang, 0), &[DIM], color)
            ));
        }
    }

    if cfg.show_tests {
        out.push_str(&format!(
            "{}\n",
            c(
                &i18n::missing_tests(lang, cfg.untested_set.len()),
                &[DIM],
                color
            )
        ));
    }

    if cfg.show_entry {
        out.push_str(&format!(
            "{}\n",
            c(&i18n::entry_points(lang, cfg.entry_set.len()), &[DIM], color)
        ));
    }

    if cfg.show_config {
        out.push_str(&format!(
            "{}\n",
            c(&i18n::config_files(lang, cfg.config_set.len()), &[DIM], color)
        ));
    }

    if cfg.show_imports && !cfg.imported_by_map.is_empty() {
        let mut items: Vec<(&String, usize)> = cfg
            .imported_by_map
            .iter()
            .map(|(k, v)| (k, v.len()))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        out.push_str(&format!("{}\n", c(t.most_depended, &[DIM], color)));
        for (relpath, n) in items.into_iter().take(5) {
            out.push_str(&format!(
                "{}\n",
                c(
                    &format!("    {}  (used by {})", sanitize_ctrl(relpath), n),
                    &[DIM],
                    color
                )
            ));
        }
    }

    if cfg.show_imports && !cfg.cycles.is_empty() {
        out.push_str(&format!(
            "{}\n",
            c(&i18n::cycles_found(lang, cfg.cycles.len()), &[DIM], color)
        ));
        for cycle in cfg.cycles.iter().take(5) {
            out.push_str(&format!(
                "{}\n",
                c(
                    &format!("    {}", sanitize_ctrl(&cycle.join(" → "))),
                    &[DIM],
                    color
                )
            ));
        }
        if cfg.cycles.len() > 5 {
            out.push_str(&format!(
                "{}\n",
                c(
                    &i18n::more_items(lang, (cfg.cycles.len() - 5) as u64),
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
            out.push_str(&format!("{}\n", c(t.hotspots, &[DIM], color)));
            for (relpath, n) in top_hot {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!(
                            "    {}  {}",
                            sanitize_ctrl(relpath),
                            i18n::change_count(lang, n)
                        ),
                        &[DIM],
                        color
                    )
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
            out.push_str(&format!("{}\n", c(t.reading_order, &[DIM], color)));
            for (i, p) in candidates.iter().enumerate() {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!("    {}. {}", i + 1, sanitize_ctrl(p)),
                        &[DIM],
                        color
                    )
                ));
            }
        }
    }

    // 長大関数トップ5（compat モードでは出さない・Python 版に無い機能）
    if cfg.show_outline && !cfg.suppress_notes && !stats.long_funcs.is_empty() {
        let mut lf = stats.long_funcs.clone();
        lf.sort_by(|x, y| y.0.cmp(&x.0).then_with(|| x.1.cmp(&y.1)));
        let top: Vec<&(u32, String)> = lf.iter().take(5).filter(|(n, _)| *n >= 50).collect();
        if !top.is_empty() {
            out.push_str(&format!(
                "{}\n",
                c(
                    i18n::tr(lang, "  Longest functions (50+ lines):", "  長大な関数（50行以上）:"),
                    &[DIM],
                    color
                )
            ));
            for (n, name) in top {
                out.push_str(&format!(
                    "{}\n",
                    c(
                        &format!("    {} lines  {}", n, sanitize_ctrl(name)),
                        &[DIM],
                        color
                    )
                ));
            }
        }
    }

    if cfg.show_git && cfg.git_map.is_empty() {
        out.push_str(&format!("{}\n", c(t.no_git_info, &[DIM], color)));
    }

    // --agent の末尾に短い精度注記（spec 機能5）。互換モードでは出さない。
    if cfg.agent && !cfg.suppress_notes {
        out.push_str(&format!(
            "{}\n",
            c(&crate::check::agent_note(cfg), &[DIM], color)
        ));
    }

    if cfg.markdown {
        out.push_str("```\n");
    }

    (out, stats)
}
