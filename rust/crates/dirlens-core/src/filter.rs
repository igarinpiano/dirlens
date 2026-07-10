//! 共通フィルタリング＋ソート（dirlens.py の _filter / count_entries /
//! _has_content / _sort_entries の等価移植）。表示可否の唯一のゲートキーパ。

use std::path::Path;
use std::sync::Arc;

use crate::cfg::Cfg;
use crate::fmt::splitext;
use crate::fnmatch::fnmatch;
use crate::gitignore::{extend_pats, is_ignored, relpath};
use crate::provider::{Entry, FsProvider};
use crate::pyc::py_casefold;
use crate::session::Session;

/// _filter 相当。None はアクセス拒否のシグナル。
pub fn filter_entries<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: &[String],
) -> Option<(Vec<Entry>, Vec<Entry>)> {
    let raw = sess.fs.scan_dir(path).ok()?;

    let mut entries: Vec<Entry> = raw
        .into_iter()
        .filter(|e| cfg.show_all || !e.name.starts_with('.'))
        .collect();

    // Tier1（事前計算済み集合）が有効なら active_pats が空でもフィルタする
    let engine_active =
        (cfg.use_gitignore && sess.git_ignored.is_some()) || !active_pats.is_empty();
    if engine_active {
        entries.retain(|e| !ignored_by_engine(sess, cfg, e, active_pats));
    }

    let mut dirs: Vec<Entry> = entries
        .iter()
        .filter(|e| e.is_dir_nofollow)
        .cloned()
        .collect();

    if cfg.follow_syms {
        let sym_dirs: Vec<Entry> = entries
            .iter()
            .filter(|e| e.is_symlink && !e.is_dir_nofollow && e.is_dir_follow)
            .cloned()
            .collect();
        dirs.extend(sym_dirs);
    }

    let mut files: Vec<Entry> = if cfg.dirs_only {
        Vec::new()
    } else {
        entries
            .iter()
            .filter(|e| {
                !e.is_dir_nofollow
                    && !(cfg.follow_syms && e.is_symlink && e.is_dir_follow)
            })
            .cloned()
            .collect()
    };

    if !cfg.excludes.is_empty() {
        dirs.retain(|d| !cfg.excludes.iter().any(|p| fnmatch(&d.name, p)));
        files.retain(|f| !cfg.excludes.iter().any(|p| fnmatch(&f.name, p)));
    }

    if !cfg.includes.is_empty() {
        files.retain(|f| cfg.includes.iter().any(|p| fnmatch(&f.name, p)));
    }

    if let Some(te) = &cfg.type_ext {
        files.retain(|f| splitext(&f.name).1.to_lowercase() == *te);
    }

    if cfg.min_size.is_some() || cfg.max_size.is_some() {
        files.retain(|f| match sess.fs.stat(&f.path, true) {
            None => true,
            Some(st) => {
                let sz = st.size as i64;
                if let Some(min) = cfg.min_size {
                    if sz < min {
                        return false;
                    }
                }
                if let Some(max) = cfg.max_size {
                    if sz > max {
                        return false;
                    }
                }
                true
            }
        });
    }

    Some((dirs, files))
}

/// gitignore 判定。Tier1（git check-ignore の事前計算集合）があればそれを、
/// なければ Tier3（内蔵マッチャ）を使う。
fn ignored_by_engine<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    e: &Entry,
    active_pats: &[String],
) -> bool {
    if let Some(git_set) = &sess.git_ignored {
        let rel = crate::gitignore::relpath_slash(&e.path, &cfg.root);
        return git_set.contains(&rel);
    }
    is_ignored(
        &e.name,
        &relpath(&e.path, &cfg.root),
        e.is_dir_nofollow,
        active_pats,
    )
}

/// count_entries 相当。(dirs, files, denied)
pub fn count_entries<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> (usize, usize, bool) {
    let pats = extend_pats(sess, active_pats, path, cfg);
    match filter_entries(sess, path, cfg, &pats) {
        None => (0, 0, true),
        Some((dirs, files)) => (dirs.len(), files.len(), false),
    }
}

/// _has_content 相当（--prune 用）。
pub fn has_content<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    depth: i64,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> bool {
    if let Some(md) = cfg.max_depth {
        if depth >= md {
            return false;
        }
    }
    let pats = extend_pats(sess, active_pats, path, cfg);
    match filter_entries(sess, path, cfg, &pats) {
        None => false,
        Some((dirs, files)) => {
            if !files.is_empty() {
                return true;
            }
            for d in &dirs {
                if has_content(sess, &d.path, depth + 1, cfg, &pats) {
                    return true;
                }
            }
            false
        }
    }
}

fn stat_f64<F: FsProvider>(sess: &Session<F>, e: &Entry, pick: fn(&crate::provider::StatInfo) -> f64) -> f64 {
    sess.fs.stat(&e.path, true).map(|st| pick(&st)).unwrap_or(0.0)
}

fn stat_size<F: FsProvider>(sess: &Session<F>, e: &Entry) -> u64 {
    sess.fs.stat(&e.path, true).map(|st| st.size).unwrap_or(0)
}

/// Python の list.sort(key=..., reverse=...) 相当の安定ソート。
/// reverse=True でも同キーの要素は元の順序を保つ（CPython と同じ）。
fn stable_sort_by_key<T, K: PartialOrd>(v: &mut Vec<T>, keys: Vec<K>, reverse: bool) {
    debug_assert_eq!(v.len(), keys.len());
    let mut pairs: Vec<(K, T)> = keys.into_iter().zip(std::mem::take(v)).collect();
    pairs.sort_by(|a, b| {
        let ord = a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal);
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    });
    v.extend(pairs.into_iter().map(|(_, t)| t));
}

/// _sort_entries 相当。
pub fn sort_entries<F: FsProvider>(
    sess: &Session<F>,
    dirs: &mut Vec<Entry>,
    files: &mut Vec<Entry>,
    cfg: &Cfg,
) {
    let rev = cfg.reverse;
    if cfg.sort_mtime {
        let dk: Vec<f64> = dirs.iter().map(|e| stat_f64(sess, e, |s| s.mtime)).collect();
        stable_sort_by_key(dirs, dk, !rev);
        let fk: Vec<f64> = files.iter().map(|e| stat_f64(sess, e, |s| s.mtime)).collect();
        stable_sort_by_key(files, fk, !rev);
    } else if cfg.sort_ctime {
        let dk: Vec<f64> = dirs.iter().map(|e| stat_f64(sess, e, |s| s.ctime)).collect();
        stable_sort_by_key(dirs, dk, !rev);
        let fk: Vec<f64> = files.iter().map(|e| stat_f64(sess, e, |s| s.ctime)).collect();
        stable_sort_by_key(files, fk, !rev);
    } else if cfg.by_size {
        let dk: Vec<u64> = dirs.iter().map(|e| sess.dir_size(&e.path).0).collect();
        stable_sort_by_key(dirs, dk, !rev);
        let fk: Vec<u64> = files.iter().map(|e| stat_size(sess, e)).collect();
        stable_sort_by_key(files, fk, !rev);
    } else {
        let dk: Vec<String> = dirs.iter().map(|e| py_casefold(&e.name)).collect();
        stable_sort_by_key(dirs, dk, rev);
        let fk: Vec<String> = files.iter().map(|e| py_casefold(&e.name)).collect();
        stable_sort_by_key(files, fk, rev);
    }
}
