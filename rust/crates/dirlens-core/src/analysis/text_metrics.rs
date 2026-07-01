//! トークン数概算（-T）と行数カウント。dirlens.py の estimate_tokens /
//! count_lines / バイナリ判定の等価移植。

use crate::fmt::splitext;
use crate::pyc::py_round;

/// テキスト本文の最大読み込みバイト数（dirlens.py の _TEXT_READ_LIMIT）。
pub const TEXT_READ_LIMIT: usize = 5_000_000;

const BINARY_EXTS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp",
    ".mp3", ".mp4", ".mov", ".avi", ".wav", ".flac", ".ogg", ".webm", ".mkv",
    ".zip", ".tar", ".gz", ".rar", ".7z", ".bz2", ".xz",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".exe", ".dll", ".so", ".dylib", ".bin", ".o", ".a", ".class", ".jar",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".db", ".sqlite", ".sqlite3", ".pyc", ".pyo", ".whl",
];

pub fn is_probably_binary(name: &str) -> bool {
    let lower = name.to_lowercase();
    let (_, ext) = splitext(&lower);
    BINARY_EXTS.contains(&ext)
}

/// トークン数概算。英数字記号は約4文字/トークン、それ以外は約1.5文字/トークン。
/// 打ち切り時は実サイズとの比でスケール補正する。
pub fn estimate_tokens(text: &str, byte_len: usize, actual_size: Option<u64>, truncated: bool) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let mut ascii_chars: i64 = 0;
    let mut other_chars: i64 = 0;
    for ch in text.chars() {
        if (ch as u32) < 128 {
            ascii_chars += 1;
        } else {
            other_chars += 1;
        }
    }
    let mut tokens = ascii_chars as f64 / 4.0 + other_chars as f64 / 1.5;
    if truncated {
        if let Some(sz) = actual_size {
            if sz != 0 && byte_len > 0 {
                tokens *= sz as f64 / byte_len as f64;
            }
        }
    }
    std::cmp::max(1, py_round(tokens))
}

/// 行数カウント（打ち切り時はスケール補正）。
pub fn count_lines(text: &str, byte_len: usize, actual_size: Option<u64>, truncated: bool) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let mut n: i64 = text.matches('\n').count() as i64;
    if !text.ends_with('\n') {
        n += 1;
    }
    if truncated {
        if let Some(sz) = actual_size {
            if sz != 0 && byte_len > 0 {
                n = std::cmp::max(1, py_round(n as f64 * sz as f64 / byte_len as f64));
            }
        }
    }
    n
}
