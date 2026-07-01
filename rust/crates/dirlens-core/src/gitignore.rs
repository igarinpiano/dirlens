//! .gitignore 内蔵マッチャ（Tier3）。dirlens.py の is_ignored / _extend_pats の等価移植。
//!
//! Tier1（git check-ignore による厳密判定）は session.git_ignored に事前計算した
//! 無視集合が入っている場合に使われる（run.rs で選択）。

use std::path::Path;
use std::sync::Arc;

use crate::cfg::Cfg;
use crate::fnmatch::fnmatch;
use crate::provider::FsProvider;
use crate::session::Session;

/// os.path.relpath 相当（共通接頭辞方式・OS セパレータで連結）。
pub fn relpath(path: &Path, start: &Path) -> String {
    let p: Vec<std::path::Component> = path.components().collect();
    let s: Vec<std::path::Component> = start.components().collect();
    let mut i = 0;
    while i < p.len() && i < s.len() && p[i] == s[i] {
        i += 1;
    }
    let mut parts: Vec<String> = std::iter::repeat("..".to_string())
        .take(s.len() - i)
        .collect();
    for comp in &p[i..] {
        parts.push(comp.as_os_str().to_string_lossy().into_owned());
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join(std::path::MAIN_SEPARATOR_STR)
    }
}

/// relpath を "/" 区切りに正規化したもの（Python の .replace("\\", "/") 相当）。
pub fn relpath_slash(path: &Path, start: &Path) -> String {
    relpath(path, start).replace('\\', "/")
}

/// パターンを順番に評価し、最後にマッチしたルールが勝つ（`!` 否定対応）。
pub fn is_ignored(name: &str, rel_path: &str, is_dir: bool, patterns: &[String]) -> bool {
    let rel = rel_path.replace('\\', "/");
    let mut result = false;
    for pat in patterns {
        let negated = pat.starts_with('!');
        let p = pat.trim_start_matches('!');
        let dir_only = p.ends_with('/');
        let p = p.trim_end_matches('/');
        if dir_only && !is_dir {
            continue;
        }
        let matched = if let Some(anchored) = p.strip_prefix('/') {
            let anchored = anchored.trim_start_matches('/');
            fnmatch(&rel, anchored)
        } else {
            fnmatch(name, p) || fnmatch(&rel, p) || fnmatch(&rel, &format!("*/{}", p))
        };
        if matched {
            result = !negated;
        }
    }
    result
}

/// Tier1: git check-ignore による無視集合の事前計算。
///
/// ルートから BFS でレベルごとに全エントリを `git check-ignore --stdin -z` へ
/// 一括投入し、無視された rel パス（"/" 区切り）の集合を作る。無視された
/// ディレクトリには降りない（その配下が問い合わせられることはないため）。
/// git が使えない・非 work tree・途中で失敗した場合は None（Tier3 へ縮退）。
pub fn build_git_ignored_set<F: FsProvider>(
    sess: &Session<F>,
    git: &dyn crate::provider::GitProvider,
    root: &Path,
) -> Option<std::collections::HashSet<String>> {
    let mut ignored: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut level_dirs: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    while !level_dirs.is_empty() {
        let mut rels: Vec<String> = Vec::new();
        let mut children: Vec<crate::provider::Entry> = Vec::new();
        for d in &level_dirs {
            if let Ok(entries) = sess.fs.scan_dir(d) {
                for e in entries {
                    rels.push(relpath_slash(&e.path, root));
                    children.push(e);
                }
            }
        }
        if rels.is_empty() {
            break;
        }
        let resp = git.check_ignore(root, &rels)?;
        ignored.extend(resp);
        level_dirs = children
            .into_iter()
            .filter(|e| e.is_dir_nofollow && !ignored.contains(&relpath_slash(&e.path, root)))
            .map(|e| e.path)
            .collect();
    }
    Some(ignored)
}

/// _extend_pats 相当: 下位ディレクトリの .gitignore をルート相対に書き換えて追加する。
pub fn extend_pats<F: FsProvider>(
    sess: &Session<F>,
    active: &Arc<Vec<String>>,
    path: &Path,
    cfg: &Cfg,
) -> Arc<Vec<String>> {
    if !cfg.use_gitignore {
        return active.clone();
    }
    if path == cfg.root.as_path() {
        return active.clone();
    }
    let local = sess.load_gitignore(path);
    if local.is_empty() {
        return active.clone();
    }
    let rel_dir = relpath_slash(path, &cfg.root);
    let mut out: Vec<String> = (**active).clone();
    for pat in local.iter() {
        let neg = pat.starts_with('!');
        let p = pat.trim_start_matches('!');
        if p.starts_with('/') {
            out.push(format!("{}/{}{}", if neg { "!" } else { "" }, rel_dir, p));
        } else {
            out.push(pat.clone());
        }
    }
    Arc::new(out)
}
