//! git 連携（-H）。dirlens.py の load_git_log の解析部分の等価移植。
//! subprocess の実行は GitProvider（native 側）が担い、コアは stdout を解析する。

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;

use crate::fmt::GitInfo;
use crate::provider::GitProvider;
use crate::pyc::py_strip;

pub const MAX_COMMITS: usize = 2000;

/// 戻り値: (file_map: 最終コミット情報, change_counts: 変更回数)
pub fn load_git_log(
    git: &dyn GitProvider,
    root: &Path,
) -> (HashMap<String, GitInfo>, IndexMap<String, u64>) {
    let stdout = match git.log_output(root, MAX_COMMITS) {
        Some(s) => s,
        None => return (HashMap::new(), IndexMap::new()),
    };
    parse_git_log(&stdout)
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
