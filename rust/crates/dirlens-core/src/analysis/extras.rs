//! ファイル単位の追加情報（dirlens.py の _file_extras の等価移植）。

use crate::analysis::outline::extract_outline;
use crate::analysis::text_metrics::{
    count_lines, count_tokens, is_probably_binary, TEXT_READ_LIMIT,
};
use crate::analysis::todo::scan_todos;
use crate::cfg::Cfg;
use crate::fmt::{splitext, GitInfo, OutlineItem};
use crate::provider::{Entry, FsProvider};
use crate::pyc::decode_utf8_ignore;
use crate::session::Session;

#[derive(Debug, Default)]
pub struct FileExtras {
    pub tokens: Option<i64>, // None = バイナリ/読めない（表示しない）
    pub lines: Option<i64>,
    pub git: Option<GitInfo>,
    pub todos: Vec<(usize, String, String)>,
    pub is_entry: bool,
    pub is_config: bool,
    pub no_test: bool,
    pub outline: Option<Vec<OutlineItem>>, // None = 対応外言語
    pub imports: Vec<String>,
    pub imported_by: Vec<String>,
    pub external_imports: Vec<String>,
}

pub fn file_extras<F: FsProvider>(
    sess: &Session<F>,
    entry: &Entry,
    rel: &str,
    cfg: &Cfg,
) -> FileExtras {
    let mut ex = FileExtras::default();
    let lower_name = entry.name.to_lowercase();
    let (_, ext_raw) = splitext(&lower_name);
    let ext = ext_raw.to_string();

    // 本文はここで一度だけ読み込んで共有する
    let need_text = cfg.show_tokens || cfg.show_todo || cfg.show_outline;
    let mut is_binary = is_probably_binary(&entry.name);
    let mut text = String::new();
    let mut byte_len: usize = 0;
    let mut truncated = false;
    if need_text && !is_binary {
        if let Some(mut data) = sess.fs.read_prefix(&entry.path, TEXT_READ_LIMIT + 1) {
            let head_len = data.len().min(8192);
            if data[..head_len].contains(&0u8) {
                is_binary = true;
            } else {
                truncated = data.len() > TEXT_READ_LIMIT;
                if truncated {
                    data.truncate(TEXT_READ_LIMIT);
                }
                byte_len = data.len();
                text = decode_utf8_ignore(&data);
            }
        }
    }

    if cfg.show_tokens {
        if is_binary {
            ex.tokens = None;
            ex.lines = None;
        } else {
            let sz = sess.fs.stat(&entry.path, true).map(|s| s.size);
            ex.tokens = Some(count_tokens(&text, byte_len, sz, truncated, cfg.tokens_bpe));
            ex.lines = Some(count_lines(&text, byte_len, sz, truncated));
        }
    }

    if cfg.show_git {
        ex.git = cfg.git_map.get(rel).cloned();
    }

    if cfg.show_todo {
        ex.todos = scan_todos(&text);
    }

    if cfg.show_entry {
        ex.is_entry = cfg.entry_set.contains(rel);
    }

    if cfg.show_config {
        ex.is_config = cfg.config_set.contains(rel);
    }

    if cfg.show_tests {
        ex.no_test = cfg.untested_set.contains(rel);
    }

    if cfg.show_outline {
        // 2段: AST（言語別最良パーサ）→ 失敗/未対応なら正規表現
        let mut outline = if cfg.enhanced_analysis {
            match crate::analysis::ast::ast_outline(&text, &ext) {
                Some(items) => Some(items),
                None => extract_outline(&text, &ext),
            }
        } else {
            extract_outline(&text, &ext)
        };
        if let Some(items) = &mut outline {
            if !items.is_empty() && cfg.public_only {
                items.retain(|item| item.public);
            }
        }
        ex.outline = outline;
    }

    if cfg.show_imports {
        ex.imports = cfg.imports_map.get(rel).cloned().unwrap_or_default();
        ex.imported_by = cfg.imported_by_map.get(rel).cloned().unwrap_or_default();
        ex.external_imports = cfg.external_map.get(rel).cloned().unwrap_or_default();
    }

    ex
}

/// 「読み始めの候補」（reading_order_candidates 相当）。
pub fn reading_order_candidates(cfg: &Cfg, top_n: usize, limit: usize) -> Vec<String> {
    let mut cand: Vec<String> = cfg.entry_set.iter().cloned().collect();
    if !cfg.imported_by_map.is_empty() {
        let mut items: Vec<(&String, usize)> = cfg
            .imported_by_map
            .iter()
            .map(|(k, v)| (k, v.len()))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1)); // 安定ソート: タイは挿入順を保つ
        for (p, _) in items.into_iter().take(top_n) {
            if !cand.contains(p) {
                cand.push(p.clone());
            }
        }
    }
    cand.truncate(limit);
    cand
}
