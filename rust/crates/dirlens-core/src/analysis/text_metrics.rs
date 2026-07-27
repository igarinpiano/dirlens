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

/// BPE トークナイザ（Tier1・o200k_base）。feature 無効時は None。
#[cfg(feature = "tokens-bpe")]
fn bpe_encoder() -> Option<&'static tiktoken_rs::CoreBPE> {
    use std::sync::OnceLock;
    static ENC: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();
    ENC.get_or_init(|| tiktoken_rs::o200k_base().ok()).as_ref()
}

/// tiktoken-rs（vendor_tiktoken.rs）は内部で使う fancy-regex の分岐バックトラック
/// スタックが自前の上限（1,000,000 分岐）を超えたときに返す `Err` を `.unwrap()`
/// しており、そのまま panic になる。OS のスタックオーバーフローではなく
/// fancy-regex 自身が検知して安全に `Err` を返している値なので、catch_unwind で
/// 捕まえれば通常の panic として回収できる（実測: 空白の無い2MB超の1トークンで
/// 確実に再現する — 改行の無い巨大な1行や連続したbase64/バイナリ様データ等）。
/// 捕まえた場合は Tier2 ヒューリスティックへ縮退する。デフォルトの panic hook が
/// 出す "thread panicked at ..." はこの既知ケースでは煩わしいだけなのでこの1件に
/// 限って抑制する（他の panic は通常どおり出力する）。
#[cfg(feature = "tokens-bpe")]
fn bpe_encode_len(enc: &tiktoken_rs::CoreBPE, text: &str) -> Option<usize> {
    use std::cell::Cell;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Once;

    thread_local! {
        static SUPPRESS: Cell<bool> = const { Cell::new(false) };
    }
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !SUPPRESS.with(Cell::get) {
                default_hook(info);
            }
        }));
    });

    SUPPRESS.with(|c| c.set(true));
    let result = panic::catch_unwind(AssertUnwindSafe(|| enc.encode_ordinary(text)));
    SUPPRESS.with(|c| c.set(false));
    result.ok().map(|v| v.len())
}

/// このビルドで BPE 計数が使えるか（--check / capabilities 用）。
pub fn bpe_available() -> bool {
    #[cfg(feature = "tokens-bpe")]
    {
        return bpe_encoder().is_some();
    }
    #[allow(unreachable_code)]
    false
}

/// トークン計数の 2 層エントリポイント。
/// Tier1: BPE（o200k_base）による正確値（打ち切り時はスケール補正で概算に戻る）。
/// Tier2: 文字数ヒューリスティック（Python 版と同一式）へ縮退。
pub fn count_tokens(
    text: &str,
    byte_len: usize,
    actual_size: Option<u64>,
    truncated: bool,
    prefer_bpe: bool,
) -> i64 {
    if text.is_empty() {
        return 0;
    }
    #[cfg(feature = "tokens-bpe")]
    if prefer_bpe {
        if let Some(enc) = bpe_encoder() {
            if let Some(n) = bpe_encode_len(enc, text) {
                let mut tokens = n as f64;
                if truncated {
                    if let Some(sz) = actual_size {
                        if sz != 0 && byte_len > 0 {
                            tokens *= sz as f64 / byte_len as f64;
                        }
                    }
                }
                return std::cmp::max(1, py_round(tokens));
            }
            // fancy-regex の内部バックトラック上限超過（→ 上の bpe_encode_len 参照）。
            // この1ファイルだけ Tier2 ヒューリスティックへ縮退し、処理は継続する。
        }
    }
    let _ = prefer_bpe;
    estimate_tokens(text, byte_len, actual_size, truncated)
}

/// トークン数概算（Tier2）。英数字記号は約4文字/トークン、それ以外は約1.5文字/トークン。
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

#[cfg(all(test, feature = "tokens-bpe"))]
mod bpe_tests {
    use super::count_tokens;

    #[test]
    fn bpe_exact_counts() {
        // o200k_base: "hello world" は 2 トークン
        assert_eq!(count_tokens("hello world", 11, None, false, true), 2);
        // 空文字は 0
        assert_eq!(count_tokens("", 0, None, false, true), 0);
        // prefer_bpe=false はヒューリスティック（11 ASCII 文字 / 4 → round(2.75) = 3）
        assert_eq!(count_tokens("hello world", 11, None, false, false), 3);
    }

    #[test]
    fn bpe_truncation_scales() {
        // 打ち切り時は実サイズとの比で補正される
        let full = count_tokens("abcd ".repeat(100).as_str(), 500, Some(1000), true, true);
        let half = count_tokens("abcd ".repeat(100).as_str(), 500, Some(500), false, true);
        assert!(full >= half * 2 - 1);
    }

    /// 空白の無い巨大な1トークン（実測: 2MB超で確実に再現）は fancy-regex の
    /// 内部バックトラック上限を超え、素の tiktoken-rs では panic する
    /// （vendor_tiktoken.rs:230 の unwrap）。catch_unwind で回収し Tier2 へ
    /// 縮退することを確認する（= プロセス全体がクラッシュしないことの回帰テスト）。
    #[test]
    fn bpe_pathological_input_falls_back_instead_of_crashing() {
        let text = "!".repeat(2_000_000);
        let byte_len = text.len();
        let tokens = count_tokens(&text, byte_len, None, false, true);
        assert!(tokens > 0);
    }
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
