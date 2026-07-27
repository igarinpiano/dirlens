//! 永続キャッシュ（CacheProvider の std 実装）。
//!
//! 置き場所: `$XDG_CACHE_HOME/dirlens/`（無ければ `~/.cache/dirlens/`）に
//! ルートパスのハッシュごとに 1 ファイル（JSON）。トークン計数（BPE）の
//! 再実行を省くのが目的で、キーに size / mtime を含むため明示的な無効化は不要。
//! `--no-cache` / `DIRLENS_CACHE=off` / compat モードでは使われない。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dirlens_core::provider::CacheProvider;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("dirlens"));
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".cache").join("dirlens"))
}

/// `--clear-cache`: 永続トークンキャッシュ（プロジェクトごとに `tokens-<hash>.json`
/// 1 ファイル）を全て削除する。ディレクトリが無ければ 0 件として成功扱い。
pub fn clear_all() -> std::io::Result<usize> {
    let Some(dir) = cache_dir() else {
        return Ok(0);
    };
    clear_dir(&dir)
}

/// `clear_all` の本体。指定ディレクトリ内の `tokens-*.json` だけを削除する
/// （キャッシュディレクトリ自体は dirlens 専用だが、将来ここへ別種のファイルが
/// 増えても壊さないよう命名パターンで絞る）。ディレクトリが無ければ 0 件。
/// 実ディレクトリ（$XDG_CACHE_HOME 等）に依存せずテストできるよう分離してある。
fn clear_dir(dir: &Path) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut removed = 0usize;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("tokens-") && name.ends_with(".json") {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// エントリ数の上限（超えたら古い集合ごと捨てて作り直す）。
const MAX_ENTRIES: usize = 200_000;

pub struct StdCache {
    path: Option<PathBuf>,
    state: Mutex<CacheState>,
}

struct CacheState {
    map: HashMap<String, String>,
    loaded: bool,
    dirty: bool,
}

impl StdCache {
    pub fn new(root: &Path) -> StdCache {
        let path = cache_dir().map(|d| {
            d.join(format!(
                "tokens-{:016x}.json",
                fnv1a64(root.to_string_lossy().as_bytes())
            ))
        });
        StdCache {
            path,
            state: Mutex::new(CacheState {
                map: HashMap::new(),
                loaded: false,
                dirty: false,
            }),
        }
    }

    fn ensure_loaded(&self, st: &mut CacheState) {
        if st.loaded {
            return;
        }
        st.loaded = true;
        let Some(p) = &self.path else { return };
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&text) {
                st.map = map;
            }
        }
    }

    /// 変更があればディスクへ書き戻す（main の最後に一度呼ぶ）。
    pub fn flush(&self) {
        let st = self.state.lock().unwrap();
        if !st.dirty {
            return;
        }
        let Some(p) = &self.path else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&st.map) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, p);
            }
        }
    }
}

impl CacheProvider for StdCache {
    fn get(&self, key: &str) -> Option<String> {
        let mut st = self.state.lock().unwrap();
        self.ensure_loaded(&mut st);
        st.map.get(key).cloned()
    }

    fn put(&self, key: &str, value: String) {
        let mut st = self.state.lock().unwrap();
        self.ensure_loaded(&mut st);
        if st.map.len() >= MAX_ENTRIES {
            st.map.clear();
        }
        st.map.insert(key.to_string(), value);
        st.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_dir_removes_only_token_cache_files() {
        let dir = std::env::temp_dir().join(format!(
            "dirlens_clear_cache_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tokens-abc123.json"), "{}").unwrap();
        std::fs::write(dir.join("tokens-def456.json"), "{}").unwrap();
        std::fs::write(dir.join("not-a-cache-file.txt"), "keep me").unwrap();

        let removed = clear_dir(&dir).unwrap();
        assert_eq!(removed, 2);
        assert!(!dir.join("tokens-abc123.json").exists());
        assert!(!dir.join("tokens-def456.json").exists());
        assert!(dir.join("not-a-cache-file.txt").exists());

        // 2回目は0件（既に消えている）で、存在しないファイルの再削除でエラーにならない。
        assert_eq!(clear_dir(&dir).unwrap(), 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clear_dir_missing_directory_is_zero_not_error() {
        let dir = std::env::temp_dir().join(format!(
            "dirlens_clear_cache_missing_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        assert!(!dir.exists());
        assert_eq!(clear_dir(&dir).unwrap(), 0);
    }
}
