//! JSON レンダラ。dirlens.py の build_json_tree / project_summary の等価移植。
//! 出力は json.dumps(..., ensure_ascii=False, indent=2) と同一（キー順は挿入順）。

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::analysis::extras::{file_extras, reading_order_candidates};
use crate::cfg::Cfg;
use crate::filter::{filter_entries, has_content, sort_entries};
use crate::fmt::{fmt_size, splitext};
use crate::gitignore::{extend_pats, relpath, relpath_slash};
use crate::provider::{Entry, FsProvider};
use crate::session::Session;

#[derive(Debug, Default)]
pub struct JsonStats {
    pub tokens: i64,
    pub todo_total: u64,
    pub todo_samples: Vec<(String, usize, String, String)>,
}

fn build_json_tree<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    depth: i64,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    stats: &mut JsonStats,
) -> Value {
    let cur_pats = extend_pats(sess, active_pats, path, cfg);
    let filtered = filter_entries(sess, path, cfg, &cur_pats);
    let denied = filtered.is_none();
    let (mut dirs, mut files) = filtered.unwrap_or((Vec::new(), Vec::new()));
    if cfg.prune {
        dirs.retain(|d| has_content(sess, &d.path, depth + 1, cfg, &cur_pats));
    }
    sort_entries(sess, &mut dirs, &mut files, cfg);
    let (sz, sz_err) = sess.dir_size(path);

    let n_dirs = dirs.len();
    let n_files = files.len();

    let mut children: Vec<Value> = Vec::new();
    let within_depth = cfg.max_depth.map(|md| depth < md).unwrap_or(true);
    if within_depth {
        let combined: Vec<Entry> = if cfg.files_first {
            files.into_iter().chain(dirs).collect()
        } else {
            dirs.into_iter().chain(files).collect()
        };
        for entry in combined {
            if entry.is_dir_nofollow {
                children.push(build_json_tree(sess, &entry.path, depth + 1, cfg, &cur_pats, stats));
            } else {
                let f_sz = sess.fs.stat(&entry.path, true).map(|s| s.size).unwrap_or(0);
                let rel = relpath_slash(&entry.path, &cfg.root);
                let extras = if cfg.has_extras {
                    file_extras(sess, &entry, &rel, cfg)
                } else {
                    Default::default()
                };

                let mut obj = Map::new();
                obj.insert("name".into(), json!(entry.name));
                obj.insert("type".into(), json!("file"));
                obj.insert("size".into(), json!(f_sz));
                obj.insert("size_human".into(), json!(fmt_size(f_sz, false)));
                let (_, ext_raw) = splitext(&entry.name);
                obj.insert("ext".into(), json!(ext_raw.to_lowercase()));
                obj.insert("path".into(), json!(rel));

                if cfg.show_tokens {
                    obj.insert("tokens".into(), json!(extras.tokens));
                    obj.insert("lines".into(), json!(extras.lines));
                    if let Some(t) = extras.tokens {
                        stats.tokens += t;
                    }
                }
                if cfg.show_git {
                    let g = extras.git.as_ref().map(|g| {
                        let mut m = Map::new();
                        m.insert("hash".into(), json!(g.hash));
                        m.insert("date".into(), json!(g.date));
                        m.insert("author".into(), json!(g.author));
                        m.insert("subject".into(), json!(g.subject));
                        Value::Object(m)
                    });
                    obj.insert("git".into(), g.unwrap_or(Value::Null));
                }
                if cfg.show_todo {
                    let todos: Vec<Value> = extras
                        .todos
                        .iter()
                        .map(|(ln, k, s)| {
                            let mut m = Map::new();
                            m.insert("line".into(), json!(ln));
                            m.insert("kind".into(), json!(k));
                            m.insert("text".into(), json!(s));
                            Value::Object(m)
                        })
                        .collect();
                    stats.todo_total += extras.todos.len() as u64;
                    for item in extras.todos.iter().take(3) {
                        if stats.todo_samples.len() < 20 {
                            stats.todo_samples.push((
                                rel.clone(),
                                item.0,
                                item.1.clone(),
                                item.2.clone(),
                            ));
                        }
                    }
                    obj.insert("todos".into(), Value::Array(todos));
                }
                if cfg.show_tests {
                    obj.insert("has_test".into(), json!(!extras.no_test));
                }
                if cfg.show_entry {
                    obj.insert("is_entry".into(), json!(extras.is_entry));
                }
                if cfg.show_config {
                    obj.insert("is_config".into(), json!(extras.is_config));
                }
                if cfg.show_outline {
                    let v = match &extras.outline {
                        None => Value::Null,
                        Some(items) => Value::Array(
                            items
                                .iter()
                                .map(|(k, nm, p)| {
                                    let mut m = Map::new();
                                    m.insert("kind".into(), json!(k));
                                    m.insert("name".into(), json!(nm));
                                    m.insert("public".into(), json!(p));
                                    Value::Object(m)
                                })
                                .collect(),
                        ),
                    };
                    obj.insert("outline".into(), v);
                }
                if cfg.show_imports {
                    obj.insert("imports".into(), json!(extras.imports));
                    obj.insert("imported_by".into(), json!(extras.imported_by));
                    obj.insert("external_imports".into(), json!(extras.external_imports));
                }

                children.push(Value::Object(obj));
            }
        }
    }

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let path_str = if path == cfg.root.as_path() {
        ".".to_string()
    } else {
        relpath(path, &cfg.root)
    };

    let mut obj = Map::new();
    obj.insert("name".into(), json!(name));
    obj.insert("type".into(), json!("directory"));
    obj.insert("size".into(), json!(sz));
    obj.insert("size_human".into(), json!(fmt_size(sz, sz_err)));
    obj.insert("path".into(), json!(path_str));
    let mut ic = Map::new();
    ic.insert("dirs".into(), json!(n_dirs));
    ic.insert("files".into(), json!(n_files));
    ic.insert("permission_denied".into(), json!(denied));
    obj.insert("item_count".into(), Value::Object(ic));
    obj.insert("children".into(), Value::Array(children));
    Value::Object(obj)
}

/// `--json` 出力スキーマの版数。フィールド追加は後方互換、
/// 改名・削除・型変更時にインクリメントする（安定した公開契約・spec §8）。
pub const SCHEMA_VERSION: u32 = 1;

/// JSON 出力全体（project_summary を含む）を文字列で返す（末尾改行つき）。
pub fn render_json<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    probe: &crate::check::EnvProbe,
) -> String {
    let mut stats = JsonStats::default();
    let mut tree = build_json_tree(sess, &cfg.root, 0, cfg, active_pats, &mut stats);

    if cfg.has_extras {
        let most_depended: Value = if cfg.show_imports && !cfg.imported_by_map.is_empty() {
            let mut items: Vec<(&String, usize)> = cfg
                .imported_by_map
                .iter()
                .map(|(k, v)| (k, v.len()))
                .collect();
            items.sort_by(|a, b| b.1.cmp(&a.1));
            Value::Array(
                items
                    .into_iter()
                    .take(10)
                    .map(|(p, n)| {
                        let mut m = Map::new();
                        m.insert("path".into(), json!(p));
                        m.insert("used_by_count".into(), json!(n));
                        Value::Object(m)
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };

        let hotspots: Value = if cfg.show_git && !cfg.git_change_counts.is_empty() {
            let mut items: Vec<(&String, u64)> = cfg
                .git_change_counts
                .iter()
                .map(|(k, v)| (k, *v))
                .collect();
            items.sort_by(|a, b| b.1.cmp(&a.1));
            Value::Array(
                items
                    .into_iter()
                    .take(10)
                    .map(|(p, n)| {
                        let mut m = Map::new();
                        m.insert("path".into(), json!(p));
                        m.insert("change_count".into(), json!(n));
                        Value::Object(m)
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };

        let reading_order: Value = if cfg.show_entry
            && cfg.show_imports
            && (!cfg.entry_set.is_empty() || !cfg.imported_by_map.is_empty())
        {
            json!(reading_order_candidates(cfg, 5, 8))
        } else {
            Value::Null
        };

        let mut ps = Map::new();
        ps.insert(
            "estimated_tokens".into(),
            if cfg.show_tokens { json!(stats.tokens) } else { Value::Null },
        );
        ps.insert(
            "todo_count".into(),
            if cfg.show_todo { json!(stats.todo_total) } else { Value::Null },
        );
        ps.insert(
            "missing_tests_count".into(),
            if cfg.show_tests { json!(cfg.untested_set.len()) } else { Value::Null },
        );
        ps.insert(
            "entry_points_count".into(),
            if cfg.show_entry { json!(cfg.entry_set.len()) } else { Value::Null },
        );
        ps.insert(
            "config_files_count".into(),
            if cfg.show_config { json!(cfg.config_set.len()) } else { Value::Null },
        );
        ps.insert(
            "git_available".into(),
            if cfg.show_git { json!(!cfg.git_map.is_empty()) } else { Value::Null },
        );
        ps.insert("most_depended_on".into(), most_depended);
        ps.insert("hotspots".into(), hotspots);
        ps.insert(
            "circular_dependencies".into(),
            if cfg.show_imports && !cfg.cycles.is_empty() {
                json!(cfg.cycles)
            } else if cfg.show_imports {
                json!([] as [i32; 0])
            } else {
                Value::Null
            },
        );
        ps.insert("reading_order_candidates".into(), reading_order);
        if let Value::Object(map) = &mut tree {
            map.insert("project_summary".into(), Value::Object(ps));
        }
    }

    // schema_version（先頭キー）と --agent 用メタブロック。
    // DIRLENS_COMPAT=python（suppress_notes）では Python 版とのバイト一致のため出さない。
    tree = match tree {
        Value::Object(map) if !cfg.suppress_notes => {
            let mut wrapped = Map::new();
            wrapped.insert("schema_version".into(), json!(SCHEMA_VERSION));
            for (k, v) in map {
                wrapped.insert(k, v);
            }
            if cfg.agent {
                wrapped.insert(
                    "capabilities".into(),
                    crate::check::capabilities_json(cfg, probe),
                );
                let mut analysis = Map::new();
                analysis.insert(
                    "gitignore_tier".into(),
                    match cfg.gitignore_tier {
                        Some(t) => json!(t),
                        None => Value::Null,
                    },
                );
                analysis.insert(
                    "outline".into(),
                    json!(if cfg.enhanced_analysis {
                        "ast+regex-fallback"
                    } else {
                        "regex"
                    }),
                );
                analysis.insert(
                    "imports".into(),
                    json!(if cfg.enhanced_analysis {
                        "ast+manifest"
                    } else {
                        "regex"
                    }),
                );
                analysis.insert("tokens".into(), json!(crate::check::tokens_mode(cfg)));
                wrapped.insert("analysis".into(), Value::Object(analysis));
            }
            Value::Object(wrapped)
        }
        other => other,
    };

    let mut s = serde_json::to_string_pretty(&tree).unwrap_or_default();
    s.push('\n');
    s
}
