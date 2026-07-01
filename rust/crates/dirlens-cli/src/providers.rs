//! FsProvider / GitProvider / ClipboardProvider の std 実装（native）。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};

use dirlens_core::provider::{ClipboardProvider, Entry, FsProvider, GitProvider, StatInfo};

pub struct StdFs;

fn systime_to_f64(t: std::time::SystemTime) -> f64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    }
}

impl FsProvider for StdFs {
    fn scan_dir(&self, path: &Path) -> Result<Vec<Entry>, ()> {
        let rd = std::fs::read_dir(path).map_err(|_| ())?;
        let mut out = Vec::new();
        for ent in rd {
            let ent = ent.map_err(|_| ())?;
            let p = ent.path();
            let name = ent.file_name().to_string_lossy().into_owned();
            let (is_dir_nf, is_file_nf, is_sym) = match ent.file_type() {
                Ok(ft) => (ft.is_dir(), ft.is_file(), ft.is_symlink()),
                Err(_) => (false, false, false),
            };
            let is_dir_follow = if is_sym {
                std::fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false)
            } else {
                is_dir_nf
            };
            out.push(Entry {
                name,
                path: p,
                is_dir_nofollow: is_dir_nf,
                is_file_nofollow: is_file_nf,
                is_symlink: is_sym,
                is_dir_follow,
            });
        }
        Ok(out)
    }

    #[cfg(unix)]
    fn stat(&self, path: &Path, follow: bool) -> Option<StatInfo> {
        use std::os::unix::fs::MetadataExt;
        let md = if follow {
            std::fs::metadata(path)
        } else {
            std::fs::symlink_metadata(path)
        }
        .ok()?;
        Some(StatInfo {
            size: md.size(),
            // CPython の st_mtime と同じ計算（sec + 1e-9 * nsec）
            mtime: md.mtime() as f64 + 1e-9 * md.mtime_nsec() as f64,
            ctime: md.ctime() as f64 + 1e-9 * md.ctime_nsec() as f64,
            mode: md.mode(),
            uid: md.uid(),
            gid: md.gid(),
        })
    }

    #[cfg(windows)]
    fn stat(&self, path: &Path, follow: bool) -> Option<StatInfo> {
        use std::os::windows::fs::MetadataExt;
        let md = if follow {
            std::fs::metadata(path)
        } else {
            std::fs::symlink_metadata(path)
        }
        .ok()?;
        // CPython の attributes_to_mode 相当の簡易版
        let attrs = md.file_attributes();
        let readonly = attrs & 0x1 != 0;
        let is_link = md.file_type().is_symlink();
        let mut mode: u32 = if is_link {
            0o120000
        } else if md.is_dir() {
            0o040000 | 0o111
        } else {
            0o100000
        };
        mode |= if readonly { 0o444 } else { 0o666 };
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if matches!(ext.as_deref(), Some("bat" | "cmd" | "exe" | "com")) {
            mode |= 0o111;
        }
        Some(StatInfo {
            size: md.file_size(),
            mtime: md.modified().map(systime_to_f64).unwrap_or(0.0),
            // Windows の st_ctime は作成時刻（CPython 互換）
            ctime: md.created().map(systime_to_f64).unwrap_or(0.0),
            mode,
            uid: 0,
            gid: 0,
        })
    }

    fn read_prefix(&self, path: &Path, limit: usize) -> Option<Vec<u8>> {
        let f = std::fs::File::open(path).ok()?;
        let mut buf = Vec::new();
        let mut handle = f.take(limit as u64);
        handle.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    fn read_link(&self, path: &Path) -> Option<String> {
        std::fs::read_link(path)
            .ok()
            .map(|t| t.to_string_lossy().into_owned())
    }

    fn real_path(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        if let Ok(c) = std::fs::canonicalize(p) {
            return Some(c);
        }
        // 存在しないパス: Python の Path.resolve(strict=False) 同様、
        // 絶対化＋字句的正規化のみ行う（cwd が取れない場合は None）。
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(p)
        };
        let mut out = PathBuf::new();
        for comp in abs.components() {
            match comp {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
        Some(out)
    }

    fn now(&self) -> f64 {
        systime_to_f64(std::time::SystemTime::now())
    }

    #[cfg(unix)]
    fn user_name(&self, uid: u32) -> Option<String> {
        unsafe {
            let mut pwd: libc::passwd = std::mem::zeroed();
            let mut buf = [0i8; 4096];
            let mut result: *mut libc::passwd = std::ptr::null_mut();
            let rc = libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr() as *mut _, buf.len(), &mut result);
            if rc == 0 && !result.is_null() {
                let cstr = std::ffi::CStr::from_ptr(pwd.pw_name);
                return Some(cstr.to_string_lossy().into_owned());
            }
            None
        }
    }

    #[cfg(windows)]
    fn user_name(&self, _uid: u32) -> Option<String> {
        None
    }

    #[cfg(unix)]
    fn group_name(&self, gid: u32) -> Option<String> {
        unsafe {
            let mut grp: libc::group = std::mem::zeroed();
            let mut buf = [0i8; 4096];
            let mut result: *mut libc::group = std::ptr::null_mut();
            let rc = libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr() as *mut _, buf.len(), &mut result);
            if rc == 0 && !result.is_null() {
                let cstr = std::ffi::CStr::from_ptr(grp.gr_name);
                return Some(cstr.to_string_lossy().into_owned());
            }
            None
        }
    }

    #[cfg(windows)]
    fn group_name(&self, _gid: u32) -> Option<String> {
        None
    }
}

// ─── git ─────────────────────────────────────────────────────

pub struct StdGit;

const GIT_TIMEOUT: Duration = Duration::from_secs(8);

/// タイムアウトつきでコマンドを実行し stdout を返す（成功時のみ）。
/// パイプ詰まりを避けるため stdout は別スレッドで読む。
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    };
    let buf = reader.join().ok()?;
    if !status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// PATH からコマンドを探す（--check の存在確認用）。
pub fn has_cmd(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let p = dir.join(name);
        #[cfg(windows)]
        let p_exe = dir.join(format!("{}.exe", name));
        #[cfg(windows)]
        return p.is_file() || p_exe.is_file();
        #[cfg(not(windows))]
        p.is_file()
    })
}

impl GitProvider for StdGit {
    fn log_output(&self, root: &Path, max_commits: usize) -> Option<String> {
        let mut cmd = Command::new("git");
        cmd.args([
            "-C",
            &root.to_string_lossy(),
            "log",
            "-n",
            &max_commits.to_string(),
            "--name-only",
            "--date=relative",
            "--pretty=format:\u{1}%H\u{2}%ad\u{2}%an\u{2}%s\u{3}",
        ]);
        run_with_timeout(cmd, GIT_TIMEOUT)
    }

    fn check_ignore(&self, root: &Path, rel_paths: &[String]) -> Option<Vec<String>> {
        if rel_paths.is_empty() {
            return Some(Vec::new());
        }
        let mut cmd = Command::new("git");
        cmd.args(["-C", &root.to_string_lossy(), "check-ignore", "--stdin", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().ok()?;
        let mut stdin = child.stdin.take()?;
        let input: Vec<u8> = rel_paths
            .iter()
            .flat_map(|p| p.as_bytes().iter().copied().chain(std::iter::once(0u8)))
            .collect();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(&input);
            // drop で stdin を閉じる
        });
        let output = child.wait_with_output().ok()?;
        let _ = writer.join();
        // check-ignore: 0=無視あり, 1=無視なし, 128=エラー（非リポジトリ等）
        match output.status.code() {
            Some(0) | Some(1) => {}
            _ => return None,
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Some(
            text.split('\0')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
        )
    }

    fn available(&self) -> bool {
        has_cmd("git")
    }

    fn is_work_tree(&self, root: &Path) -> bool {
        Command::new("git")
            .args(["-C", &root.to_string_lossy(), "rev-parse", "--is-inside-work-tree"])
            .stderr(Stdio::null())
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false)
    }
}

// ─── クリップボード ───────────────────────────────────────────

pub struct StdClipboard;

fn pipe_to(cmd: &[&str], input: &[u8]) -> Result<bool, std::io::ErrorKind> {
    let mut c = Command::new(cmd[0]);
    c.args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return Err(e.kind()),
    };
    {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
        }
    }
    match child.wait() {
        Ok(status) => Ok(status.success()),
        Err(_) => Ok(false),
    }
}

impl ClipboardProvider for StdClipboard {
    fn copy(&self, text: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            matches!(pipe_to(&["pbcopy"], text.as_bytes()), Ok(true))
        }
        #[cfg(windows)]
        {
            // Python 版と同じく UTF-16 で clip へ渡す
            let mut bytes = Vec::with_capacity(text.len() * 2 + 2);
            bytes.extend_from_slice(&[0xFF, 0xFE]); // BOM (Python の encode("utf-16") 相当)
            for u in text.encode_utf16() {
                bytes.extend_from_slice(&u.to_le_bytes());
            }
            matches!(pipe_to(&["clip"], &bytes), Ok(true))
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            for cmd in [
                &["wl-copy"][..],
                &["xclip", "-selection", "clipboard"][..],
                &["xsel", "--clipboard", "--input"][..],
            ] {
                match pipe_to(cmd, text.as_bytes()) {
                    Ok(ok) => return ok,
                    Err(std::io::ErrorKind::NotFound) => continue,
                    Err(_) => return false,
                }
            }
            false
        }
    }

    fn available(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            has_cmd("pbcopy")
        }
        #[cfg(windows)]
        {
            has_cmd("clip")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            has_cmd("wl-copy") || has_cmd("xclip") || has_cmd("xsel")
        }
    }
}
