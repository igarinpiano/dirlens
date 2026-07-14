//! I/O 抽象トレイト。
//!
//! コアは std::fs / std::process / std::thread を直接呼ばない。native では
//! dirlens-cli が std 実装を、wasm では dirlens-wasm がホストコールバック実装を提供する。

use std::path::{Path, PathBuf};

/// os.stat_result 相当（コアが必要とするフィールドのみ）。
/// mtime/ctime は CPython と同じ計算（sec + 1e-9 * nsec）の f64。
#[derive(Debug, Clone, Copy, Default)]
pub struct StatInfo {
    pub size: u64,
    pub mtime: f64,
    pub ctime: f64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

/// os.DirEntry 相当。型フラグは走査時に確定させる（d_type 相当）。
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir_nofollow: bool,
    pub is_file_nofollow: bool,
    pub is_symlink: bool,
    /// symlink の場合のみ follow した結果（それ以外は is_dir_nofollow と同じ）
    pub is_dir_follow: bool,
}

pub trait FsProvider {
    /// os.scandir 相当。列挙順は OS の返す順のまま（ソートしない）。
    /// 権限拒否等は Err(()) を返す（Python の OSError catch に対応）。
    fn scan_dir(&self, path: &Path) -> Result<Vec<Entry>, ()>;

    /// os.stat / os.lstat 相当。失敗時 None（呼び出し側でフォールバック）。
    fn stat(&self, path: &Path, follow: bool) -> Option<StatInfo>;

    /// ファイル先頭 limit バイトを読む（open+read 相当）。失敗時 None。
    fn read_prefix(&self, path: &Path, limit: usize) -> Option<Vec<u8>>;

    /// os.readlink 相当。失敗時 None。
    fn read_link(&self, path: &Path) -> Option<String>;

    /// os.path.realpath 相当（失敗しても入力を返す）。
    fn real_path(&self, path: &Path) -> PathBuf;

    /// Path.resolve() 相当（存在しなくても正規化した絶対パスを返す）。
    fn resolve(&self, path: &str) -> Option<PathBuf>;

    /// 現在時刻（epoch 秒）。wasm ではホストが供給する。
    fn now(&self) -> f64;

    /// pwd.getpwuid / grp.getgrgid 相当。未対応プラットフォームでは None。
    fn user_name(&self, uid: u32) -> Option<String>;
    fn group_name(&self, gid: u32) -> Option<String>;
}

pub trait GitProvider {
    /// `git -C root log -n max --name-only --date=relative --pretty=...` の
    /// stdout を返す。git が無い / リポジトリでない / タイムアウト時は None。
    fn log_output(&self, root: &Path, max_commits: usize) -> Option<String>;

    /// `git -C root check-ignore --stdin -z` に rel_paths を投入し、
    /// 無視されたパスの集合を返す。git 不在・非 work tree なら None（Tier3 へ縮退）。
    fn check_ignore(&self, root: &Path, rel_paths: &[String]) -> Option<Vec<String>>;

    /// git バイナリが使えるか（--check 用）。
    fn available(&self) -> bool {
        false
    }

    /// root が git work tree 内か（--check 用）。
    fn is_work_tree(&self, _root: &Path) -> bool {
        false
    }

    /// `git -C root status --porcelain -z` の stdout（--status / --since 用）。
    /// git 不在・非リポジトリなら None。
    fn status_output(&self, _root: &Path) -> Option<String> {
        None
    }

    /// `git -C root diff --name-status -z <ref>` の stdout（--since 用）。
    fn diff_names(&self, _root: &Path, _ref: &str) -> Option<String> {
        None
    }

    /// `git -C root show <ref>:<rel>` の stdout（--api-diff 用）。
    /// ref 時点にファイルが無い場合も None。
    fn show_file(&self, _root: &Path, _ref: &str, _rel: &str) -> Option<String> {
        None
    }
}

pub trait ClipboardProvider {
    /// クリップボードにコピーする。成功なら true。
    fn copy(&self, text: &str) -> bool;

    /// クリップボードツールが存在するか（--check 用）。
    fn available(&self) -> bool {
        false
    }
}

/// 解析結果の永続キャッシュ（トークン数など）。native CLI がファイル実装を供給し、
/// wasm では未使用。key はサイズ・mtime を含むため、無効化はキーの不一致で自然に起きる。
pub trait CacheProvider: Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn put(&self, key: &str, value: String);
}

/// GitProvider が存在しない環境（wasm 等）向けのダミー。
pub struct NoGit;
impl GitProvider for NoGit {
    fn log_output(&self, _root: &Path, _max: usize) -> Option<String> {
        None
    }
    fn check_ignore(&self, _root: &Path, _rel: &[String]) -> Option<Vec<String>> {
        None
    }
}

/// ClipboardProvider が存在しない環境向けのダミー。
pub struct NoClipboard;
impl ClipboardProvider for NoClipboard {
    fn copy(&self, _text: &str) -> bool {
        false
    }
}
