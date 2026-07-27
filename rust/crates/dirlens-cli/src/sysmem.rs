//! システムの物理メモリ量を検出し、5MB 固定だったトークン読み込み上限
//! （`TEXT_READ_LIMIT`）をマシンのスペックに応じて動的に引き上げる。
//!
//! メモリ量を検出できない環境（対応外OS・取得失敗）では None を返し、
//! 呼び出し側は今までどおり固定 5MB にフォールバックする（挙動を後退させない）。

use dirlens_core::analysis::text_metrics::TEXT_READ_LIMIT;

#[cfg(target_os = "macos")]
pub fn total_memory_bytes() -> Option<u64> {
    use std::ffi::CString;
    use std::mem;
    unsafe {
        let name = CString::new("hw.memsize").ok()?;
        let mut value: u64 = 0;
        let mut size = mem::size_of::<u64>();
        let ret = libc::sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        );
        if ret == 0 {
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
pub fn total_memory_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    // 単位は常に kB。"MemAvailable"（他プロセス・キャッシュ込みで実際に使える見込み量）
    // があればそちらを優先し、無ければ（古いカーネル）"MemTotal" にフォールバックする。
    let parse_kb = |label: &str| -> Option<u64> {
        text.lines()
            .find(|l| l.starts_with(label))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    parse_kb("MemAvailable:").or_else(|| parse_kb("MemTotal:"))
}

#[cfg(target_os = "windows")]
pub fn total_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            Some(status.ullTotalPhys)
        } else {
            None
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn total_memory_bytes() -> Option<u64> {
    None
}

/// システムメモリ量から、1ファイルあたりの本文読み込み上限（=これを超えると
/// BPE正確値ではなく比例概算になる境界）を決める。段階式なのは、際限のない
/// 連続式だと巨大ファイルが複数混在したときの合計メモリ使用量が読みにくくなる
/// ため（並列ウォーム時は複数ファイル分を同時にメモリへ保持する）。
/// 固定 5MB（TEXT_READ_LIMIT）を下限として維持する。
pub fn dynamic_text_read_limit(total_mem_bytes: u64) -> usize {
    const GB: u64 = 1_000_000_000;
    if total_mem_bytes < 8 * GB {
        TEXT_READ_LIMIT
    } else if total_mem_bytes < 16 * GB {
        15_000_000
    } else if total_mem_bytes < 32 * GB {
        30_000_000
    } else if total_mem_bytes < 64 * GB {
        50_000_000
    } else {
        80_000_000
    }
}

/// 実行時に使う text_read_limit を決める（main.rs から呼ぶ、判定ロジックだけを
/// 切り出した純粋関数。OS呼び出しを伴わないためユニットテストで全分岐を
/// 決定論的に検証できる）。優先順位:
///   1. `env_override`（DIRLENS_MAX_FILE_BYTES）が 1 以上の数値としてパースできればそれ
///      （数値でない・0 未満などパース不能な値は無視して 3. の既定に従う——
///      DIRLENS_MAX_WORKERS と同じ規約）
///   2. 上記が無く、互換モード（Python版とのバイト一致検証用）でもなく、
///      `detected_total_mem` が取得できていれば、そこから動的に算出した値
///   3. どちらにも該当しなければ `default`（呼び出し側が Cfg にあらかじめ入れている
///      TEXT_READ_LIMIT=5MB）——メモリ量の検出に失敗した（total_memory_bytes が
///      None を返した）場合もここに落ちる
pub fn resolve_text_read_limit(
    env_override: Option<&str>,
    compat_python: bool,
    detected_total_mem: Option<u64>,
    default: usize,
) -> usize {
    if let Some(v) = env_override {
        if let Ok(n) = v.trim().parse::<usize>() {
            if n >= 1 {
                return n;
            }
        }
    } else if !compat_python {
        if let Some(total) = detected_total_mem {
            return dynamic_text_read_limit(total);
        }
    }
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_are_monotonic_and_floor_is_the_old_constant() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(dynamic_text_read_limit(0), TEXT_READ_LIMIT);
        assert_eq!(dynamic_text_read_limit(4 * GB), TEXT_READ_LIMIT);
        assert_eq!(dynamic_text_read_limit(7 * GB), TEXT_READ_LIMIT);
        let mut prev = dynamic_text_read_limit(4 * GB);
        for gb in [8, 16, 32, 64, 128] {
            let cur = dynamic_text_read_limit(gb * GB);
            assert!(cur >= prev, "expected non-decreasing tiers at {}GB", gb);
            prev = cur;
        }
        assert_eq!(dynamic_text_read_limit(256 * GB), 80_000_000);
    }

    /// 本題: メモリ量の検出に失敗した（total_memory_bytes() が None を返す環境・
    /// 対応外OS・sysctl/GlobalMemoryStatusEx 失敗等）場合に、確実に既定値（5MB）
    /// へフォールバックすることの回帰テスト。
    #[test]
    fn falls_back_to_default_when_memory_detection_fails() {
        assert_eq!(
            resolve_text_read_limit(None, false, None, TEXT_READ_LIMIT),
            TEXT_READ_LIMIT
        );
        // 既定値は呼び出し側（Cfg）が持つ値をそのまま尊重する（TEXT_READ_LIMIT
        // 自体をハードコードしていないことの確認）。
        assert_eq!(resolve_text_read_limit(None, false, None, 999), 999);
    }

    #[test]
    fn uses_dynamic_value_when_memory_detected_and_not_compat() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(
            resolve_text_read_limit(None, false, Some(20 * GB), TEXT_READ_LIMIT),
            dynamic_text_read_limit(20 * GB)
        );
    }

    #[test]
    fn compat_python_ignores_detected_memory_even_when_available() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(
            resolve_text_read_limit(None, true, Some(64 * GB), TEXT_READ_LIMIT),
            TEXT_READ_LIMIT
        );
    }

    #[test]
    fn valid_env_override_wins_even_over_detected_memory() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(
            resolve_text_read_limit(Some("12345678"), false, Some(64 * GB), TEXT_READ_LIMIT),
            12345678
        );
    }

    #[test]
    fn malformed_env_override_is_ignored_like_dirlens_max_workers() {
        // DIRLENS_MAX_WORKERS と同じ規約: 数値でない・0 未満は無視して既定に従う。
        // メモリ検出結果があっても、override が指定されている以上そちらは見ない
        // （DIRLENS_MAX_WORKERS 同様、override 分岐に入ったら動的計算にはフォール
        // スルーしない）。
        const GB: u64 = 1_000_000_000;
        for bad in ["not-a-number", "0", "-5", ""] {
            assert_eq!(
                resolve_text_read_limit(Some(bad), false, Some(64 * GB), TEXT_READ_LIMIT),
                TEXT_READ_LIMIT,
                "bad override {:?} should fall back to default",
                bad
            );
        }
    }
}
