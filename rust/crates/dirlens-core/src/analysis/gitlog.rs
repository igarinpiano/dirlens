//! git 連携（-H）。dirlens.py の load_git_log の解析部分の等価移植。
//! subprocess の実行は GitProvider（native 側）が担い、コアは stdout を解析する。

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;

use crate::analysis::gitstatus::to_scan_relative;
use crate::fmt::GitInfo;
use crate::provider::GitProvider;
use crate::pyc::py_strip;

pub const MAX_COMMITS: usize = 2000;

/// 戻り値: (file_map: 最終コミット情報, change_counts: 変更回数)
///
/// git log のパスはリポジトリルート相対のため、root がリポジトリの
/// サブディレクトリの場合はスキャンルート相対へ変換し、対象外のパスは落とす
/// （同名ファイルがルートにあると誤った履歴が付く・注釈が消える問題の修正）。
/// map_to_scan_root=false（DIRLENS_COMPAT=python）では Python 版と同じく
/// 変換しない（DELTAS §14）。
pub fn load_git_log(
    git: &dyn GitProvider,
    root: &Path,
    map_to_scan_root: bool,
) -> (HashMap<String, GitInfo>, IndexMap<String, u64>) {
    let stdout = match git.log_output(root, MAX_COMMITS) {
        Some(s) => s,
        None => return (HashMap::new(), IndexMap::new()),
    };
    let (map, counts) = parse_git_log(&stdout);
    if !map_to_scan_root {
        return (map, counts);
    }
    let prefix = git.repo_prefix(root).unwrap_or_default();
    remap_to_scan_root(map, counts, &prefix)
}

/// git log 由来のマップをスキャンルート相対へ変換する（prefix が空なら無変換）。
pub fn remap_to_scan_root(
    map: HashMap<String, GitInfo>,
    counts: IndexMap<String, u64>,
    prefix: &str,
) -> (HashMap<String, GitInfo>, IndexMap<String, u64>) {
    if prefix.is_empty() {
        return (map, counts);
    }
    let map = map
        .into_iter()
        .filter_map(|(p, v)| Some((to_scan_relative(&p, prefix)?.to_string(), v)))
        .collect();
    let counts = counts
        .into_iter()
        .filter_map(|(p, v)| Some((to_scan_relative(&p, prefix)?.to_string(), v)))
        .collect();
    (map, counts)
}

pub fn parse_git_log(stdout: &str) -> (HashMap<String, GitInfo>, IndexMap<String, u64>) {
    let mut file_map: HashMap<String, GitInfo> = HashMap::new();
    let mut change_counts: IndexMap<String, u64> = IndexMap::new();
    let mut current: Option<GitInfo> = None;
    for raw in stdout.split('\n') {
        let line = raw.trim_matches('\r');
        if let Some(body) = line.strip_prefix('\u{1}') {
            let body = body.strip_suffix('\u{3}').unwrap_or(body);
            let parts: Vec<&str> = body.splitn(4, '\u{2}').collect();
            current = if parts.len() == 4 {
                Some(GitInfo {
                    hash: parts[0].chars().take(7).collect(),
                    date: parts[1].to_string(),
                    author: parts[2].to_string(),
                    subject: parts[3].to_string(),
                })
            } else {
                None
            };
        } else if !py_strip(line).is_empty() {
            if let Some(cur) = &current {
                let fp = py_strip(line).replace('\\', "/");
                file_map.entry(fp.clone()).or_insert_with(|| cur.clone());
                *change_counts.entry(fp).or_insert(0) += 1;
            }
        }
    }
    (file_map, change_counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_fixture() -> (HashMap<String, GitInfo>, IndexMap<String, u64>) {
        // 新しい順: physq 更新 → ルート README のコミット
        let stdout = "\u{1}aaaa111\u{2}2 days ago\u{2}an\u{2}physq update\u{3}\n\
                      physq/README.md\n\
                      physq/src/cli.rs\n\
                      \n\
                      \u{1}bbbb222\u{2}10 days ago\u{2}an\u{2}root readme\u{3}\n\
                      README.md\n\
                      scripts/tool.py\n";
        parse_git_log(stdout)
    }

    #[test]
    fn remap_strips_prefix_and_drops_outside() {
        let (map, counts) = log_fixture();
        // 変換前: リポジトリルート相対のまま（ルートスキャン時の従来挙動）
        assert!(map.contains_key("physq/README.md") && map.contains_key("README.md"));

        let (map, counts) = remap_to_scan_root(map, counts, "physq/");
        // physq/README.md → README.md（physq のコミットが付く。ルートの
        // README.md のコミットが誤って付いていたのが修正前の症状）
        assert_eq!(map.get("README.md").unwrap().subject, "physq update");
        assert_eq!(map.get("src/cli.rs").unwrap().hash, "aaaa111");
        assert_eq!(map.len(), 2);
        assert!(!counts.contains_key("scripts/tool.py"));
        assert_eq!(counts.get("README.md"), Some(&1));
    }

    #[test]
    fn remap_noop_at_repo_root() {
        let (map, counts) = log_fixture();
        let (map2, counts2) = remap_to_scan_root(map.clone(), counts.clone(), "");
        assert_eq!(map2.len(), map.len());
        assert_eq!(counts2.len(), counts.len());
        assert_eq!(map2.get("README.md").unwrap().subject, "root readme");
    }
}
