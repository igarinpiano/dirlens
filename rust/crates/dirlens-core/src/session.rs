//! 走査セッション。dirlens.py のモジュールグローバルキャッシュ
//! （_sz_cache / _gi_cache）に相当する状態を保持する。
//!
//! Mutex を使うのは native 側で dir サイズの並列プリフェッチ（CLI がスレッドから
//! dir_size を呼ぶ）を可能にするため。wasm では単線で使われ、競合しない。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::provider::FsProvider;

pub struct Session<'a, F: FsProvider> {
    pub fs: &'a F,
    sz_cache: Mutex<HashMap<PathBuf, (u64, bool)>>,
    gi_cache: Mutex<HashMap<PathBuf, Arc<Vec<String>>>>,
    /// Tier1（git check-ignore）で得た無視パス集合（rel path, "/" 区切り）。
    /// None なら Tier3（内蔵マッチャ）に縮退する。
    pub git_ignored: Option<std::collections::HashSet<String>>,
}

impl<'a, F: FsProvider> Session<'a, F> {
    pub fn new(fs: &'a F) -> Self {
        Session {
            fs,
            sz_cache: Mutex::new(HashMap::new()),
            gi_cache: Mutex::new(HashMap::new()),
            git_ignored: None,
        }
    }

    /// dir_size 相当。(合計サイズ, 読めない箇所があったか) を返す（メモ化つき）。
    /// symlink はサイズに算入しない。file でも dir でもないエントリも同様。
    pub fn dir_size(&self, path: &Path) -> (u64, bool) {
        if let Some(v) = self.sz_cache.lock().unwrap().get(path) {
            return *v;
        }
        let mut total: u64 = 0;
        let mut has_errors = false;
        match self.fs.scan_dir(path) {
            Err(()) => has_errors = true,
            Ok(entries) => {
                for e in entries {
                    if e.is_file_nofollow {
                        match self.fs.stat(&e.path, false) {
                            Some(st) => total += st.size,
                            None => has_errors = true,
                        }
                    } else if e.is_dir_nofollow {
                        let (sub, err) = self.dir_size(&e.path);
                        total += sub;
                        if err {
                            has_errors = true;
                        }
                    }
                }
            }
        }
        let result = (total, has_errors);
        self.sz_cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), result);
        result
    }

    /// load_gitignore 相当（ディレクトリ単位でキャッシュ）。
    pub fn load_gitignore(&self, dir: &Path) -> Arc<Vec<String>> {
        if let Some(v) = self.gi_cache.lock().unwrap().get(dir) {
            return v.clone();
        }
        let mut pats = Vec::new();
        let p = dir.join(".gitignore");
        // 巨大ファイルによる OOM を防ぐため上限つきで読む（正常な .gitignore は数 KB）
        if let Some(data) = self
            .fs
            .read_prefix(&p, crate::analysis::text_metrics::TEXT_READ_LIMIT)
        {
            let text = crate::pyc::decode_utf8_ignore(&data);
            for line in text.split('\n') {
                let line = crate::pyc::py_strip(line);
                if !line.is_empty() && !line.starts_with('#') {
                    pats.push(line.to_string());
                }
            }
        }
        let arc = Arc::new(pats);
        self.gi_cache
            .lock()
            .unwrap()
            .insert(dir.to_path_buf(), arc.clone());
        arc
    }
}
