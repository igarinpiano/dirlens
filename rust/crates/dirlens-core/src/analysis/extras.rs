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
    /// tokens が 5MB 打ち切りの比例概算か（BPE 正確値ではない）。JSON 出力用
    pub tokens_estimated: bool,
    pub lines: Option<i64>,
    pub git: Option<GitInfo>,
    pub todos: Vec<(usize, String, String)>,
    pub is_entry: bool,
    pub is_config: bool,
    pub no_test: bool,
    pub outline: Option<Vec<OutlineItem>>, // None = 対応外言語
    /// アウトラインの取得方式。"ast" = 言語別 AST パーサ、"regex" = 正規表現縮退
    /// （構文エラー・AST 無効時。取得漏れがありうる）。outline が None なら None
    pub outline_method: Option<&'static str>,
    pub imports: Vec<String>,
    pub imported_by: Vec<String>,
    pub external_imports: Vec<String>,
}

/// ファイル本文の読み込みを要する重い解析結果（tokens / lines / todos / outline）。
/// ファイル内容と cfg だけに依存する純粋な計算なので、事前に並列で計算して
/// キャッシュしておける（cfg.git_map 等の共有状態に依存する軽い項目は含めない）。
#[derive(Debug, Default, Clone)]
pub struct HeavyExtras {
    pub tokens: Option<i64>,
    pub tokens_estimated: bool,
    pub lines: Option<i64>,
    pub todos: Vec<(usize, String, String)>,
    pub outline: Option<Vec<OutlineItem>>,
    pub outline_method: Option<&'static str>,
}

/// 本文読込を伴う重い解析（tokens / lines / todos / outline）を計算する。
/// I/O と CPU（BPE トークナイズ・AST パース）が集中するため、native では
/// これを全ファイル分だけ事前に並列実行して `Session` にキャッシュする。
/// 出力は入力（ファイル内容 + cfg）だけで決まるので、直列実行と完全に一致する。
pub fn compute_heavy_extras<F: FsProvider>(
    sess: &Session<F>,
    entry: &Entry,
    rel: &str,
    cfg: &Cfg,
) -> HeavyExtras {
    let mut ex = HeavyExtras::default();
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
            // 5MB 打ち切り時は比例概算（キャッシュヒットでも読み込みは
            // 行われるため truncated はこの時点で確定している）
            ex.tokens_estimated = truncated;
            let st = sess.fs.stat(&entry.path, true);
            let sz = st.map(|s| s.size);
            // 永続キャッシュ: BPE 計数はコストが高いため (rel, size, mtime, 方式) を
            // キーに再利用する。ファイル変更でキーが変わり自然に無効化される。
            let key = st.map(|s| {
                format!(
                    "tok:{}:{}:{}:{}",
                    rel,
                    s.size,
                    (s.mtime * 1e9) as i128,
                    if cfg.tokens_bpe { "bpe" } else { "chr" }
                )
            });
            let cached = key
                .as_deref()
                .and_then(|k| sess.cache.and_then(|c| c.get(k)))
                .and_then(|v| {
                    let (a, b) = v.split_once(',')?;
                    Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?))
                });
            match cached {
                Some((tok, lines)) => {
                    ex.tokens = Some(tok);
                    ex.lines = Some(lines);
                }
                None => {
                    let tok = count_tokens(&text, byte_len, sz, truncated, cfg.tokens_bpe);
                    let lines = count_lines(&text, byte_len, sz, truncated);
                    ex.tokens = Some(tok);
                    ex.lines = Some(lines);
                    if let (Some(k), Some(c)) = (key, sess.cache) {
                        c.put(&k, format!("{},{}", tok, lines));
                    }
                }
            }
        }
    }

    if cfg.show_todo {
        ex.todos = scan_todos(&text);
    }

    if cfg.show_outline {
        // 2段: AST（言語別最良パーサ）→ 失敗/未対応なら正規表現。
        // どちらの層で取得したかを outline_method に残す（regex 縮退は
        // 取得漏れがありうるため、JSON 消費者が機械的に判別できるように）
        let mut outline = if cfg.enhanced_analysis {
            match crate::analysis::ast::ast_outline(&text, &ext) {
                Some(items) => {
                    ex.outline_method = Some("ast");
                    Some(items)
                }
                None => extract_outline(&text, &ext),
            }
        } else {
            extract_outline(&text, &ext)
        };
        if outline.is_some() && ex.outline_method.is_none() {
            ex.outline_method = Some("regex");
        }
        if let Some(items) = &mut outline {
            if !items.is_empty() && cfg.public_only {
                items.retain(|item| item.public);
            }
        }
        ex.outline = outline;
    }

    ex
}

pub fn file_extras<F: FsProvider>(
    sess: &Session<F>,
    entry: &Entry,
    rel: &str,
    cfg: &Cfg,
) -> FileExtras {
    // 重い項目は事前並列計算のキャッシュがあれば再利用し、無ければその場で計算する
    // （キャッシュはあくまで高速化のためのウォーマーで、ミスしても結果は同一）。
    let heavy = sess.heavy_extras(entry, rel, cfg);

    let mut ex = FileExtras {
        tokens: heavy.tokens,
        tokens_estimated: heavy.tokens_estimated,
        lines: heavy.lines,
        todos: heavy.todos,
        outline: heavy.outline,
        outline_method: heavy.outline_method,
        ..Default::default()
    };

    if cfg.show_git {
        ex.git = cfg.git_map.get(rel).cloned();
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
