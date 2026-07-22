//! 走査セッション。dirlens.py のモジュールグローバルキャッシュ
//! （_sz_cache / _gi_cache）に相当する状態を保持する。
//!
//! Mutex を使うのは native 側で dir サイズの並列プリフェッチ（CLI がスレッドから
//! dir_size を呼ぶ）を可能にするため。wasm では単線で使われ、競合しない。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::analysis::extras::{compute_heavy_extras, HeavyExtras};
use crate::cfg::Cfg;
use crate::provider::{Entry, FsProvider};

pub struct Session<'a, F: FsProvider> {
    pub fs: &'a F,
    sz_cache: Mutex<HashMap<PathBuf, (u64, bool)>>,
    gi_cache: Mutex<HashMap<PathBuf, Arc<Vec<String>>>>,
    /// 重い解析結果（tokens / lines / todos / outline）の事前計算キャッシュ。
    /// native では走査前に全ファイル分を並列計算して埋める。読み出しはあくまで
    /// 高速化用のウォーマーで、未登録ならその場で直列計算するため結果は同一。
    heavy_cache: Mutex<HashMap<PathBuf, Arc<HeavyExtras>>>,
    /// Tier1（git check-ignore）で得た無視パス集合（rel path, "/" 区切り）。
    /// None なら Tier3（内蔵マッチャ）に縮退する。
    pub git_ignored: Option<std::collections::HashSet<String>>,
    /// 永続キャッシュ（native CLI が供給。無ければ None）。
    pub cache: Option<&'a dyn crate::provider::CacheProvider>,
}

impl<'a, F: FsProvider> Session<'a, F> {
    pub fn new(fs: &'a F) -> Self {
        Session {
            fs,
            sz_cache: Mutex::new(HashMap::new()),
            gi_cache: Mutex::new(HashMap::new()),
            heavy_cache: Mutex::new(HashMap::new()),
            git_ignored: None,
            cache: None,
        }
    }

    /// file_extras の重い項目を返す。事前並列計算のキャッシュがあれば再利用し、
    /// 無ければその場で計算する。
    pub fn heavy_extras(&self, entry: &Entry, rel: &str, cfg: &Cfg) -> HeavyExtras {
        if let Some(v) = self.heavy_cache.lock().unwrap().get(&entry.path) {
            return (**v).clone();
        }
        compute_heavy_extras(self, entry, rel, cfg)
    }

    /// 事前計算した重い項目をキャッシュへ登録する（並列ウォーマから呼ぶ）。
    pub fn insert_heavy(&self, path: PathBuf, heavy: HeavyExtras) {
        self.heavy_cache
            .lock()
            .unwrap()
            .insert(path, Arc::new(heavy));
    }

    /// ワーカー 1 本分の結果をまとめて登録する（ロック取得を per-item ではなく
    /// per-worker にして、高コア時のロック競合を避ける）。
    pub fn insert_heavy_many(&self, items: Vec<(PathBuf, HeavyExtras)>) {
        if items.is_empty() {
            return;
        }
        let mut cache = self.heavy_cache.lock().unwrap();
        for (path, heavy) in items {
            cache.insert(path, Arc::new(heavy));
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
