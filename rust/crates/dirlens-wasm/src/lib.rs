// dirlens – wasm バインディング
//
// Copyright 2026 Igarin
// Licensed under the Apache License, Version 2.0.
//
// ホスト（JS/Python）がファイルツリーを JSON マニフェストとして供給し、
// dirlens-core が解析する。実 FS・git・クリップボードは wasm では使えない
// （GitProvider は常に None 相当 → gitignore は Tier3、-H は無効）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dirlens_core::provider::{Entry, FsProvider, NoClipboard, NoGit, StatInfo};
use dirlens_core::{run, Args};
use serde::Deserialize;

/// ホストが供給するファイル1件分。
#[derive(Debug, Deserialize)]
pub struct ManifestFile {
    /// "/" 区切りの相対パス
    pub path: String,
    /// テキスト内容（省略時は空）
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub mtime: f64,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub now: f64,
}

/// メモリ内 FsProvider（wasm ホスト供給ツリー用）。
pub struct MemFs {
    files: BTreeMap<PathBuf, (Vec<u8>, f64)>,
    dirs: BTreeMap<PathBuf, f64>,
    now: f64,
}

const ROOT: &str = "/project";

impl MemFs {
    pub fn from_manifest(m: &Manifest) -> Self {
        let mut files = BTreeMap::new();
        let mut dirs: BTreeMap<PathBuf, f64> = BTreeMap::new();
        dirs.insert(PathBuf::from(ROOT), m.now);
        for f in &m.files {
            let full = Path::new(ROOT).join(&f.path);
            let mut anc = full.parent();
            while let Some(a) = anc {
                dirs.insert(a.to_path_buf(), m.now);
                if a == Path::new(ROOT) {
                    break;
                }
                anc = a.parent();
            }
            files.insert(full, (f.content.clone().into_bytes(), f.mtime));
        }
        MemFs {
            files,
            dirs,
            now: m.now,
        }
    }
}

impl FsProvider for MemFs {
    fn scan_dir(&self, path: &Path) -> Result<Vec<Entry>, ()> {
        if !self.dirs.contains_key(path) {
            return Err(());
        }
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for p in self.files.keys().chain(self.dirs.keys()) {
            if let Ok(rest) = p.strip_prefix(path) {
                if let Some(first) = rest.components().next() {
                    let name = first.as_os_str().to_string_lossy().into_owned();
                    if name.is_empty() || !seen.insert(name.clone()) {
                        continue;
                    }
                    let child = path.join(&name);
                    let is_dir = self.dirs.contains_key(&child);
                    out.push(Entry {
                        name,
                        path: child.clone(),
                        is_dir_nofollow: is_dir,
                        is_file_nofollow: !is_dir,
                        is_symlink: false,
                        is_dir_follow: is_dir,
                    });
                }
            }
        }
        Ok(out)
    }

    fn stat(&self, path: &Path, _follow: bool) -> Option<StatInfo> {
        if let Some((data, mtime)) = self.files.get(path) {
            return Some(StatInfo {
                size: data.len() as u64,
                mtime: *mtime,
                ctime: *mtime,
                mode: 0o100644,
                uid: 0,
                gid: 0,
            });
        }
        self.dirs.get(path).map(|mtime| StatInfo {
            size: 0,
            mtime: *mtime,
            ctime: *mtime,
            mode: 0o040755,
            uid: 0,
            gid: 0,
        })
    }

    fn read_prefix(&self, path: &Path, limit: usize) -> Option<Vec<u8>> {
        self.files
            .get(path)
            .map(|(data, _)| data[..data.len().min(limit)].to_vec())
    }

    fn read_link(&self, _path: &Path) -> Option<String> {
        None
    }

    fn real_path(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn resolve(&self, _path: &str) -> Option<PathBuf> {
        Some(PathBuf::from(ROOT))
    }

    fn now(&self) -> f64 {
        self.now
    }

    fn user_name(&self, _uid: u32) -> Option<String> {
        None
    }

    fn group_name(&self, _gid: u32) -> Option<String> {
        None
    }
}

/// マニフェスト（JSON）と引数リスト（JSON 配列）を受け取り、stdout 相当を返す。
pub fn run_with_manifest(manifest_json: &str, args_json: &str) -> Result<String, String> {
    let manifest: Manifest =
        serde_json::from_str(manifest_json).map_err(|e| format!("manifest parse error: {}", e))?;
    let argv: Vec<String> =
        serde_json::from_str(args_json).map_err(|e| format!("args parse error: {}", e))?;

    let mut args = Args {
        path: ROOT.to_string(),
        ..Default::default()
    };
    // wasm では最小限のフラグのみサポート（--json / --agent / 個別解析フラグ）
    for a in &argv {
        match a.as_str() {
            "--json" | "-J" => args.json = true,
            "--agent" => args.agent = true,
            "-T" | "--tokens" => args.tokens = true,
            "-K" | "--todo" => args.todo = true,
            "-V" | "--missing-tests" => args.tests = true,
            "-N" | "--entry" => args.entry = true,
            "-O" | "--outline" => args.outline = true,
            "-A" | "--api" => args.api = true,
            "-M" | "--imports" => args.imports = true,
            "-F" | "--config" => args.config = true,
            "-a" | "--all" => args.all = true,
            "--no-color" | "-n" => args.no_color = true,
            other => {
                if let Some(rest) = other.strip_prefix("-L") {
                    if let Ok(n) = rest.parse::<i64>() {
                        args.depth = Some(n);
                    }
                }
            }
        }
    }
    args.no_color = true;

    let fs = MemFs::from_manifest(&manifest);
    let res = run(args, &fs, &NoGit, &NoClipboard, false);
    if res.exit_code != 0 {
        return Err(res.stderr);
    }
    Ok(res.stdout)
}

#[cfg(target_arch = "wasm32")]
mod wasm_api {
    use wasm_bindgen::prelude::*;

    /// JS から呼ぶエントリポイント。
    #[wasm_bindgen]
    pub fn dirlens_run(manifest_json: &str, args_json: &str) -> Result<String, JsValue> {
        super::run_with_manifest(manifest_json, args_json).map_err(|e| JsValue::from_str(&e))
    }
}
