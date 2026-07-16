//! git status / diff の解析（--status / --since 用）。
//! subprocess の実行は GitProvider（native 側）が担い、コアは stdout を解析する。

use std::collections::{HashMap, HashSet};

/// git の出力パス（リポジトリルート相対）をスキャンルート相対へ変換する。
/// prefix は `git rev-parse --show-prefix` の値（末尾スラッシュ付き、
/// スキャンルートがリポジトリルートなら空文字）。prefix 配下に無いパス
/// （= スキャン対象外）は None。
pub fn to_scan_relative<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        Some(path)
    } else {
        path.strip_prefix(prefix)
    }
}

/// `git status --porcelain -z` の解析。rel path（"/" 区切り）→ XY コード。
/// リネーム（R/C）は「新パス→XY」を記録し、旧パスは無視する。
pub fn parse_status_porcelain(out: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty());
    while let Some(entry) = it.next() {
        if entry.len() < 4 {
            continue;
        }
        let xy = &entry[..2];
        let path = &entry[3..];
        map.insert(path.replace('\\', "/"), xy.to_string());
        // R/C は続くレコードが旧パス（-z 形式）。読み捨てる。
        if xy.starts_with('R') || xy.starts_with('C') {
            it.next();
        }
    }
    map
}

/// `git diff --name-status -z <ref>` の解析。
/// 戻り値: (変更ファイル → 状態文字, 削除されたファイル一覧)。
/// リネームは新パスを 'R' として記録する。
pub fn parse_diff_name_status(out: &str) -> (HashMap<String, char>, Vec<String>) {
    let mut changed: HashMap<String, char> = HashMap::new();
    let mut deleted: Vec<String> = Vec::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty());
    while let Some(status) = it.next() {
        let kind = status.chars().next().unwrap_or('?');
        match kind {
            'R' | 'C' => {
                let _old = it.next();
                if let Some(new) = it.next() {
                    changed.insert(new.replace('\\', "/"), 'R');
                }
            }
            'D' => {
                if let Some(p) = it.next() {
                    deleted.push(p.replace('\\', "/"));
                }
            }
            _ => {
                if let Some(p) = it.next() {
                    changed.insert(p.replace('\\', "/"), kind);
                }
            }
        }
    }
    deleted.sort();
    (changed, deleted)
}

/// --since 用: diff の変更分 + working tree の untracked/変更を統合した集合。
/// git のパスはリポジトリルート相対のため、prefix（`rev-parse --show-prefix`）で
/// スキャンルート相対へ変換し、スキャン対象外のパスは落とす。
pub fn build_since_set(
    diff_out: Option<&str>,
    status_out: Option<&str>,
    prefix: &str,
) -> (HashSet<String>, HashMap<String, char>, Vec<String>) {
    let (changed_repo, deleted_repo) = match diff_out {
        Some(o) => parse_diff_name_status(o),
        None => (HashMap::new(), Vec::new()),
    };
    let mut changed: HashMap<String, char> = changed_repo
        .into_iter()
        .filter_map(|(p, c)| Some((to_scan_relative(&p, prefix)?.to_string(), c)))
        .collect();
    let deleted: Vec<String> = deleted_repo
        .iter()
        .filter_map(|p| to_scan_relative(p, prefix).map(str::to_string))
        .collect();
    if let Some(o) = status_out {
        for (path, xy) in parse_status_porcelain(o) {
            let Some(path) = to_scan_relative(&path, prefix) else {
                continue;
            };
            let mark = if xy == "??" { 'A' } else { 'M' };
            changed.entry(path.to_string()).or_insert(mark);
        }
    }
    let set: HashSet<String> = changed.keys().cloned().collect();
    (set, changed, deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_basic() {
        let out = " M a.rs\0?? new.txt\0R  new_name.rs\0old_name.rs\0";
        let m = parse_status_porcelain(out);
        assert_eq!(m.get("a.rs").map(|s| s.as_str()), Some(" M"));
        assert_eq!(m.get("new.txt").map(|s| s.as_str()), Some("??"));
        assert_eq!(m.get("new_name.rs").map(|s| s.as_str()), Some("R "));
        assert!(!m.contains_key("old_name.rs"));
    }

    #[test]
    fn diff_name_status() {
        let out = "M\0src/a.rs\0A\0src/b.rs\0D\0gone.rs\0R100\0old.rs\0newpath.rs\0";
        let (changed, deleted) = parse_diff_name_status(out);
        assert_eq!(changed.get("src/a.rs"), Some(&'M'));
        assert_eq!(changed.get("src/b.rs"), Some(&'A'));
        assert_eq!(changed.get("newpath.rs"), Some(&'R'));
        assert_eq!(deleted, vec!["gone.rs"]);
    }

    #[test]
    fn since_merges_untracked() {
        let (set, marks, _) =
            build_since_set(Some("M\0a.rs\0"), Some("?? b.txt\0 M a.rs\0"), "");
        assert!(set.contains("a.rs") && set.contains("b.txt"));
        assert_eq!(marks.get("a.rs"), Some(&'M')); // diff 側が優先
        assert_eq!(marks.get("b.txt"), Some(&'A'));
    }

    #[test]
    fn scan_relative_prefix() {
        assert_eq!(to_scan_relative("physq/src/a.rs", "physq/"), Some("src/a.rs"));
        assert_eq!(to_scan_relative("README.md", "physq/"), None);
        assert_eq!(to_scan_relative("physq2/a.rs", "physq/"), None);
        assert_eq!(to_scan_relative("README.md", ""), Some("README.md"));
    }

    #[test]
    fn since_strips_repo_prefix() {
        // スキャンルートがリポジトリのサブディレクトリ physq/ の場合:
        // physq/ 配下はスキャンルート相対になり、外のパスは落ちる。
        let (set, marks, deleted) = build_since_set(
            Some("M\0physq/src/a.rs\0D\0physq/gone.rs\0M\0README.md\0"),
            Some("?? physq/new.txt\0?? scripts/x.py\0"),
            "physq/",
        );
        assert!(set.contains("src/a.rs") && set.contains("new.txt"));
        assert!(!set.contains("README.md") && !set.contains("physq/src/a.rs"));
        assert_eq!(marks.get("src/a.rs"), Some(&'M'));
        assert_eq!(marks.get("new.txt"), Some(&'A'));
        assert_eq!(deleted, vec!["gone.rs"]);
    }
}
