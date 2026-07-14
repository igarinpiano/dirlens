//! ファイル単位レポート系の出力モード
//! （--stdin / --focus / --pack / --mermaid / --dot / --csv / --api-diff）。

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::analysis::extras::file_extras;
use crate::analysis::gitstatus::parse_diff_name_status;
use crate::analysis::outline::extract_outline;
use crate::analysis::text_metrics::TEXT_READ_LIMIT;
use crate::cfg::Cfg;
use crate::filter::filter_entries;
use crate::fmt::{fmt_size, fmt_tokens, sanitize_ctrl, splitext, OutlineItem};
use crate::gitignore::{extend_pats, relpath_slash};
use crate::i18n::{tr, Lang};
use crate::provider::{Entry, FsProvider, GitProvider};
use crate::pyc::decode_utf8_ignore;
use crate::session::Session;

/// フィルタ適用済みの全ファイル Entry を rel path つきで収集する。
pub fn collect_entries<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    out: &mut Vec<(String, Entry)>,
) {
    let cur_pats = extend_pats(sess, active_pats, path, cfg);
    let Some((dirs, files)) = filter_entries(sess, path, cfg, &cur_pats) else {
        return;
    };
    for f in files {
        out.push((relpath_slash(&f.path, &cfg.root), f));
    }
    for d in dirs {
        collect_entries(sess, &d.path, cfg, &cur_pats, out);
    }
}

fn entry_for_path<F: FsProvider>(fs: &F, path: &Path) -> Option<Entry> {
    let st = fs.stat(path, true)?;
    if st.mode & 0o170000 == 0o040000 {
        return None; // ディレクトリは対象外
    }
    Some(Entry {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_path_buf(),
        is_dir_nofollow: false,
        is_file_nofollow: true,
        is_symlink: false,
        is_dir_follow: false,
    })
}

/// 入力パスをプロジェクト rel（"/" 区切り）に正規化する。root 外はそのまま返す。
fn normalize_rel(cfg: &Cfg, resolved: &Path, given: &str) -> String {
    let rel = relpath_slash(resolved, &cfg.root);
    if rel.starts_with("..") {
        given.replace('\\', "/")
    } else {
        rel
    }
}

// ─── --stdin ─────────────────────────────────────────────────

pub fn render_stdin_report<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    files: &[String],
) -> (String, Option<Value>) {
    let lang = cfg.lang;
    let mut out = String::new();
    let mut json_files: Vec<Value> = Vec::new();
    let mut total_tokens: i64 = 0;

    for given in files {
        let Some(resolved) = sess.fs.resolve(given) else {
            out.push_str(&format!(
                "{}\n",
                crate::i18n::err_not_found(lang, given)
            ));
            continue;
        };
        let Some(entry) = entry_for_path(sess.fs, &resolved) else {
            out.push_str(&format!(
                "{}\n",
                crate::i18n::err_not_found(lang, given)
            ));
            continue;
        };
        let rel = normalize_rel(cfg, &resolved, given);
        let ex = file_extras(sess, &entry, &rel, cfg);
        let sz = sess.fs.stat(&entry.path, true).map(|s| s.size).unwrap_or(0);

        if cfg.json {
            let mut o = Map::new();
            o.insert("path".into(), json!(rel));
            o.insert("size".into(), json!(sz));
            o.insert("tokens".into(), json!(ex.tokens));
            o.insert("lines".into(), json!(ex.lines));
            o.insert(
                "outline".into(),
                match &ex.outline {
                    None => Value::Null,
                    Some(items) => Value::Array(
                        items
                            .iter()
                            .map(|it| {
                                let mut m = Map::new();
                                m.insert("kind".into(), json!(it.kind));
                                m.insert("name".into(), json!(it.name));
                                m.insert("public".into(), json!(it.public));
                                if let Some(doc) = &it.doc {
                                    m.insert("doc".into(), json!(doc));
                                }
                                if let Some((a, b)) = it.span {
                                    m.insert("lines".into(), json!([a, b]));
                                }
                                Value::Object(m)
                            })
                            .collect(),
                    ),
                },
            );
            o.insert(
                "todos".into(),
                Value::Array(
                    ex.todos
                        .iter()
                        .map(|(ln, k, s)| json!({"line": ln, "kind": k, "text": s}))
                        .collect(),
                ),
            );
            if cfg.show_git {
                o.insert(
                    "git".into(),
                    ex.git
                        .as_ref()
                        .map(|g| json!({"hash": g.hash, "date": g.date, "subject": g.subject}))
                        .unwrap_or(Value::Null),
                );
            }
            json_files.push(Value::Object(o));
        } else {
            let mut meta = vec![fmt_size(sz, false)];
            if let Some(t) = ex.tokens {
                total_tokens += t;
                meta.push(fmt_tokens(t));
            }
            if let Some(l) = ex.lines {
                meta.push(format!("{} lines", l));
            }
            out.push_str(&format!(
                "{}  ({})\n",
                sanitize_ctrl(&rel),
                meta.join(", ")
            ));
            if let Some(items) = &ex.outline {
                if !items.is_empty() {
                    let names: Vec<String> = items
                        .iter()
                        .take(12)
                        .map(|it| format!("{} {}", it.kind, it.name))
                        .collect();
                    let extra = if items.len() > 12 {
                        format!(", +{}", items.len() - 12)
                    } else {
                        String::new()
                    };
                    out.push_str(&format!(
                        "  outline: {}{}\n",
                        sanitize_ctrl(&names.join(", ")),
                        extra
                    ));
                }
            }
            for (ln, kind, text) in ex.todos.iter().take(5) {
                out.push_str(&format!(
                    "  {}:{} [{}] {}\n",
                    tr(lang, "todo", "todo"),
                    ln,
                    kind,
                    sanitize_ctrl(text)
                ));
            }
            if let Some(g) = &ex.git {
                out.push_str(&format!("  git: {}\n", crate::fmt::fmt_git(g)));
            }
        }
    }

    if cfg.json {
        let v = json!({
            "schema_version": crate::render_json::SCHEMA_VERSION,
            "files": json_files,
        });
        (String::new(), Some(v))
    } else {
        if total_tokens > 0 {
            out.push_str(&format!(
                "\n{}: {}\n",
                tr(lang, "total", "合計"),
                fmt_tokens(total_tokens)
            ));
        }
        (out, None)
    }
}

// ─── --focus ─────────────────────────────────────────────────

/// graph 上の BFS（推移閉包・出発点は含まない）。戻り値は (距離順の到達ノード)。
fn transitive(graph_get: impl Fn(&str) -> Vec<String>, start: &str) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = Vec::new();
    let mut q: VecDeque<String> = VecDeque::new();
    q.push_back(start.to_string());
    while let Some(cur) = q.pop_front() {
        for nxt in graph_get(&cur) {
            if nxt != start && seen.insert(nxt.clone()) {
                order.push(nxt.clone());
                q.push_back(nxt);
            }
        }
    }
    order
}

pub fn render_focus<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    focus: &str,
) -> (String, Option<Value>, bool) {
    let lang = cfg.lang;
    let in_graph = |r: &str| {
        cfg.imports_map.contains_key(r) || cfg.imported_by_map.contains_key(r)
    };
    // 入力の正規化: ①プロジェクト相対（dirlens の出力そのまま）②CWD 相対/絶対 の順に試す
    let as_given = focus.trim_start_matches("./").replace('\\', "/");
    let rel = if in_graph(&as_given) {
        as_given
    } else {
        let resolved = sess
            .fs
            .resolve(focus)
            .map(|p| normalize_rel(cfg, &p, focus))
            .unwrap_or_else(|| as_given.clone());
        if in_graph(&resolved) {
            resolved
        } else {
            // suffix 一致でサジェスト（タイポ・ディレクトリ違いの救済）
            let suffix = format!("/{}", as_given.rsplit('/').next().unwrap_or(&as_given));
            let mut hints: Vec<&String> = cfg
                .imports_map
                .keys()
                .chain(cfg.imported_by_map.keys())
                .filter(|k| k.ends_with(&suffix) || **k == as_given)
                .collect();
            hints.sort();
            hints.dedup();
            let hint = if hints.is_empty() {
                String::new()
            } else {
                let list: Vec<&str> = hints.iter().take(3).map(|s| s.as_str()).collect();
                match lang {
                    Lang::Ja => format!("。もしかして: {}", list.join(", ")),
                    Lang::En => format!("; did you mean: {}", list.join(", ")),
                }
            };
            let msg = match lang {
                Lang::Ja => format!(
                    "エラー: '{}' は import グラフに存在しません（対応言語のソースで、プロジェクト内のパスを指定してください）{}",
                    as_given, hint
                ),
                Lang::En => format!(
                    "error: '{}' is not in the import graph (specify a project-relative source file in a supported language){}",
                    as_given, hint
                ),
            };
            return (msg, None, false);
        }
    };

    let deps_direct = cfg.imports_map.get(&rel).cloned().unwrap_or_default();
    let dependents_direct = cfg.imported_by_map.get(&rel).cloned().unwrap_or_default();
    let deps_all = transitive(
        |n| cfg.imports_map.get(n).cloned().unwrap_or_default(),
        &rel,
    );
    let dependents_all = transitive(
        |n| cfg.imported_by_map.get(n).cloned().unwrap_or_default(),
        &rel,
    );
    let cycles_here: Vec<&Vec<String>> = cfg
        .cycles
        .iter()
        .filter(|c| c.contains(&rel))
        .collect();

    if cfg.json {
        let v = json!({
            "schema_version": crate::render_json::SCHEMA_VERSION,
            "focus": rel,
            "depends_on": {"direct": deps_direct, "transitive": deps_all},
            "depended_on_by": {"direct": dependents_direct, "transitive": dependents_all},
            "external_imports": cfg.external_map.get(&rel).cloned().unwrap_or_default(),
            "cycles": cycles_here,
        });
        return (String::new(), Some(v), true);
    }

    let mut out = String::new();
    out.push_str(&format!(
        "{}: {}\n\n",
        tr(lang, "Impact analysis", "影響範囲"),
        sanitize_ctrl(&rel)
    ));
    let list = |out: &mut String, title: String, direct: &[String], all: &[String]| {
        out.push_str(&format!("{}\n", title));
        if all.is_empty() {
            out.push_str(tr(lang, "  (none)\n", "  (なし)\n"));
        }
        const CAP: usize = 30;
        let direct_set: BTreeSet<&String> = direct.iter().collect();
        for p in all.iter().take(CAP) {
            let mark = if direct_set.contains(p) { "→" } else { "⇢" };
            out.push_str(&format!("  {} {}\n", mark, sanitize_ctrl(p)));
        }
        if all.len() > CAP {
            out.push_str(&format!(
                "{}\n",
                crate::i18n::more_items(lang, (all.len() - CAP) as u64)
            ));
        }
        out.push('\n');
    };
    list(
        &mut out,
        match lang {
            Lang::Ja => format!(
                "依存先 — このファイルが import している（直接 {} / 推移 {}）:",
                deps_direct.len(),
                deps_all.len()
            ),
            Lang::En => format!(
                "Depends on — files this imports (direct {} / transitive {}):",
                deps_direct.len(),
                deps_all.len()
            ),
        },
        &deps_direct,
        &deps_all,
    );
    list(
        &mut out,
        match lang {
            Lang::Ja => format!(
                "依存元 — このファイルを変更すると影響しうる（直接 {} / 推移 {}）:",
                dependents_direct.len(),
                dependents_all.len()
            ),
            Lang::En => format!(
                "Depended on by — files potentially affected by changes here (direct {} / transitive {}):",
                dependents_direct.len(),
                dependents_all.len()
            ),
        },
        &dependents_direct,
        &dependents_all,
    );
    let externals = cfg.external_map.get(&rel).cloned().unwrap_or_default();
    if !externals.is_empty() {
        out.push_str(&format!(
            "{}: {}\n",
            tr(lang, "External imports", "外部 import"),
            sanitize_ctrl(&externals.join(", "))
        ));
    }
    if !cycles_here.is_empty() {
        out.push_str(&format!(
            "{}:\n",
            tr(lang, "Cycles involving this file", "このファイルを含む循環依存")
        ));
        for cyc in cycles_here.iter().take(3) {
            out.push_str(&format!("  {}\n", sanitize_ctrl(&cyc.join(" → "))));
        }
    }
    (out, None, true)
}

// ─── --pack ──────────────────────────────────────────────────

fn fence_for(content: &str) -> String {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    "`".repeat((max_run + 1).max(3))
}

fn lang_hint(ext: &str) -> &'static str {
    match ext {
        ".py" => "python",
        ".rs" => "rust",
        ".js" | ".mjs" | ".cjs" => "javascript",
        ".jsx" => "jsx",
        ".ts" => "typescript",
        ".tsx" => "tsx",
        ".go" => "go",
        ".c" | ".h" => "c",
        ".json" => "json",
        ".toml" => "toml",
        ".yml" | ".yaml" => "yaml",
        ".sh" => "bash",
        ".md" => "markdown",
        ".html" => "html",
        ".css" => "css",
        _ => "",
    }
}

pub fn render_pack<F: FsProvider>(sess: &Session<F>, cfg: &Cfg, files: &[String]) -> String {
    let lang = cfg.lang;
    let mut sections = String::new();
    let mut total_tokens: i64 = 0;
    let mut count = 0usize;

    for given in files {
        let Some(resolved) = sess.fs.resolve(given) else {
            sections.push_str(&format!(
                "{}\n\n",
                crate::i18n::err_not_found(lang, given)
            ));
            continue;
        };
        if entry_for_path(sess.fs, &resolved).is_none() {
            sections.push_str(&format!(
                "{}\n\n",
                crate::i18n::err_not_found(lang, given)
            ));
            continue;
        }
        let rel = normalize_rel(cfg, &resolved, given);
        let data = sess
            .fs
            .read_prefix(&resolved, TEXT_READ_LIMIT + 1)
            .unwrap_or_default();
        let truncated = data.len() > TEXT_READ_LIMIT;
        let mut data = data;
        if truncated {
            data.truncate(TEXT_READ_LIMIT);
        }
        let text = decode_utf8_ignore(&data);
        let sz = sess.fs.stat(&resolved, true).map(|s| s.size).unwrap_or(0);
        let tokens = crate::analysis::text_metrics::count_tokens(
            &text,
            data.len(),
            Some(sz),
            truncated,
            cfg.tokens_bpe,
        );
        total_tokens += tokens;
        count += 1;

        let lower_name = rel.rsplit('/').next().unwrap_or(&rel).to_lowercase();
        let (_, ext_raw) = splitext(&lower_name);
        let fence = fence_for(&text);
        sections.push_str(&format!(
            "## {} ({}, {})\n\n{}{}\n{}{}\n\n",
            rel,
            fmt_size(sz, false),
            fmt_tokens(tokens),
            fence,
            lang_hint(&ext_raw),
            text.trim_end_matches('\n').to_string() + "\n",
            fence
        ));
        if truncated {
            sections.push_str(tr(
                lang,
                "(truncated to 5MB)\n\n",
                "(5MB で打ち切り)\n\n",
            ));
        }
    }

    let header = match lang {
        Lang::Ja => format!(
            "# dirlens pack — {} ファイル, {}\n\n",
            count,
            fmt_tokens(total_tokens)
        ),
        Lang::En => format!(
            "# dirlens pack — {} file{}, {}\n\n",
            count,
            if count == 1 { "" } else { "s" },
            fmt_tokens(total_tokens)
        ),
    };
    format!("{}{}", header, sections)
}

// ─── --mermaid / --dot / --csv ───────────────────────────────

pub fn render_mermaid(cfg: &Cfg) -> String {
    // Mermaid の node id は英数のみが安全なため、パス→連番 id の2パスで組む
    let mut index: HashMap<&String, usize> = HashMap::new();
    let mut ordered: Vec<&String> = Vec::new();
    for (from, tos) in &cfg.imports_map {
        if !index.contains_key(from) {
            index.insert(from, ordered.len());
            ordered.push(from);
        }
        for to in tos {
            if !index.contains_key(to) {
                index.insert(to, ordered.len());
                ordered.push(to);
            }
        }
    }
    let mut out = String::from("graph LR\n");
    for (i, p) in ordered.iter().enumerate() {
        out.push_str(&format!(
            "    n{}[\"{}\"]\n",
            i,
            p.replace('"', "&quot;")
        ));
    }
    for (from, tos) in &cfg.imports_map {
        let fi = index[from];
        for to in tos {
            out.push_str(&format!("    n{} --> n{}\n", fi, index[to]));
        }
    }
    out
}

pub fn render_dot(cfg: &Cfg) -> String {
    let mut out = String::from("digraph imports {\n    rankdir=LR;\n    node [shape=box, fontsize=10];\n");
    for (from, tos) in &cfg.imports_map {
        for to in tos {
            out.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                from.replace('"', "\\\""),
                to.replace('"', "\\\"")
            ));
        }
    }
    out.push_str("}\n");
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn render_csv<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    let mut entries = Vec::new();
    collect_entries(sess, &cfg.root, cfg, active_pats, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::from(
        "path,size,ext,mtime_epoch,tokens,lines,todo_count,is_entry,is_config,has_test,imports,imported_by\n",
    );
    for (rel, entry) in &entries {
        let st = sess.fs.stat(&entry.path, true);
        let sz = st.map(|s| s.size).unwrap_or(0);
        let mtime = st.map(|s| s.mtime).unwrap_or(0.0);
        let lower_name = entry.name.to_lowercase();
        let (_, ext) = splitext(&lower_name);
        let ex = if cfg.has_extras {
            file_extras(sess, entry, rel, cfg)
        } else {
            Default::default()
        };
        let opt_i64 = |v: Option<i64>| v.map(|x| x.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(rel),
            sz,
            csv_escape(&ext),
            mtime as i64,
            opt_i64(ex.tokens),
            opt_i64(ex.lines),
            if cfg.show_todo { ex.todos.len().to_string() } else { String::new() },
            if cfg.show_entry { ex.is_entry.to_string() } else { String::new() },
            if cfg.show_config { ex.is_config.to_string() } else { String::new() },
            if cfg.show_tests { (!ex.no_test).to_string() } else { String::new() },
            if cfg.show_imports { ex.imports.len().to_string() } else { String::new() },
            if cfg.show_imports { ex.imported_by.len().to_string() } else { String::new() },
        ));
    }
    out
}

// ─── --api-diff ──────────────────────────────────────────────

fn public_outline(text: &str, ext: &str, enhanced: bool) -> Option<Vec<OutlineItem>> {
    let outline = if enhanced {
        match crate::analysis::ast::ast_outline(text, ext) {
            Some(items) => Some(items),
            None => extract_outline(text, ext),
        }
    } else {
        extract_outline(text, ext)
    };
    outline.map(|items| items.into_iter().filter(|i| i.public).collect())
}

pub fn render_api_diff<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    git: &dyn GitProvider,
    active_pats: &Arc<Vec<String>>,
    ref_: &str,
) -> Result<String, String> {
    let lang = cfg.lang;
    // 変更ファイルだけを対象にする（git diff --name-status）
    let Some(diff_out) = git.diff_names(&cfg.root, ref_) else {
        return Err(match lang {
            Lang::Ja => format!(
                "エラー: --api-diff {} を解決できません（git が無いか、ref が不正です）",
                ref_
            ),
            Lang::En => format!(
                "error: cannot resolve --api-diff {} (git unavailable or bad ref)",
                ref_
            ),
        });
    };
    let (changed, deleted) = parse_diff_name_status(&diff_out);

    let mut entries = Vec::new();
    collect_entries(sess, &cfg.root, cfg, active_pats, &mut entries);
    let current: HashMap<&String, &Entry> = entries.iter().map(|(r, e)| (r, e)).collect();

    let supported = |rel: &str| {
        let (_, ext) = splitext(rel.rsplit('/').next().unwrap_or(rel));
        matches!(
            ext.to_lowercase().as_str(),
            ".py" | ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" | ".rs" | ".go" | ".c"
        )
    };

    struct FileDiff {
        rel: String,
        added: Vec<String>,
        removed: Vec<String>,
    }
    let mut diffs: Vec<FileDiff> = Vec::new();

    let outline_names = |items: Option<Vec<OutlineItem>>| -> BTreeSet<String> {
        items
            .unwrap_or_default()
            .into_iter()
            .map(|it| format!("{} {}", it.kind, it.name))
            .collect()
    };

    let mut targets: Vec<String> = changed.keys().filter(|r| supported(r)).cloned().collect();
    targets.sort();
    for rel in targets {
        let lower_name = rel.rsplit('/').next().unwrap_or(&rel).to_lowercase();
        let (_, ext) = splitext(&lower_name);
        let old_text = git.show_file(&cfg.root, ref_, &rel).unwrap_or_default();
        let old_syms = outline_names(public_outline(&old_text, &ext, cfg.enhanced_analysis));
        let new_syms = match current.get(&rel) {
            Some(e) => {
                let data = sess.fs.read_prefix(&e.path, TEXT_READ_LIMIT).unwrap_or_default();
                let text = decode_utf8_ignore(&data);
                outline_names(public_outline(&text, &ext, cfg.enhanced_analysis))
            }
            None => BTreeSet::new(),
        };
        let added: Vec<String> = new_syms.difference(&old_syms).cloned().collect();
        let removed: Vec<String> = old_syms.difference(&new_syms).cloned().collect();
        if !added.is_empty() || !removed.is_empty() {
            diffs.push(FileDiff { rel, added, removed });
        }
    }
    // ref 以降に削除されたファイル: 全公開シンボルが除去扱い
    for rel in &deleted {
        if !supported(rel) {
            continue;
        }
        let lower_name = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
        let (_, ext) = splitext(&lower_name);
        if let Some(old_text) = git.show_file(&cfg.root, ref_, rel) {
            let old_syms = outline_names(public_outline(&old_text, &ext, cfg.enhanced_analysis));
            if !old_syms.is_empty() {
                diffs.push(FileDiff {
                    rel: rel.clone(),
                    added: Vec::new(),
                    removed: old_syms.into_iter().collect(),
                });
            }
        }
    }

    let total_added: usize = diffs.iter().map(|d| d.added.len()).sum();
    let total_removed: usize = diffs.iter().map(|d| d.removed.len()).sum();

    let mut out = String::new();
    match lang {
        Lang::Ja => out.push_str(&format!(
            "公開APIの差分 ({} 以降): +{} / -{} シンボル\n\n",
            ref_, total_added, total_removed
        )),
        Lang::En => out.push_str(&format!(
            "Public API diff (since {}): +{} / -{} symbols\n\n",
            ref_, total_added, total_removed
        )),
    }
    if diffs.is_empty() {
        out.push_str(tr(
            lang,
            "No public API changes detected.\n",
            "公開APIの変更は検出されませんでした。\n",
        ));
        return Ok(out);
    }
    for d in &diffs {
        out.push_str(&format!("{}\n", sanitize_ctrl(&d.rel)));
        for a in &d.added {
            out.push_str(&format!("  + {}\n", sanitize_ctrl(a)));
        }
        for r in &d.removed {
            out.push_str(&format!("  - {}\n", sanitize_ctrl(r)));
        }
        out.push('\n');
    }
    if total_removed > 0 {
        out.push_str(tr(
            lang,
            "note: removed symbols may indicate breaking changes (rename/signature changes appear as remove+add)\n",
            "注: シンボルの除去は破壊的変更の可能性があります（改名やシグネチャ変更は除去+追加として現れます）\n",
        ));
    }
    Ok(out)
}
