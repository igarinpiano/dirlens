//! CPython 互換の細部ヘルパ。
//!
//! ゴールデンテスト（dirlens.py とのバイト一致）のため、丸め・文字列処理の
//! 細かい挙動を CPython と揃える必要がある箇所をここに集約する。

/// CPython の `round(x)`（銀行家丸め・half-to-even）相当。
pub fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// CPython の `int(x)`（ゼロ方向切り捨て）相当。
pub fn py_trunc(x: f64) -> i64 {
    x.trunc() as i64
}

/// CPython の `f"{x:.2f}"` 等に相当（Rust の {:.prec$} も正確丸めで一致する）。
pub fn fmt_prec(x: f64, prec: usize) -> String {
    format!("{:.*}", prec, x)
}

/// Python の `s.rstrip('0').rstrip('.')` 相当。
pub fn rstrip_zeros(s: &str) -> String {
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

/// Python の str.strip()（Unicode 空白除去）相当。
/// Python は Unicode White_Space に加えて \x1c-\x1f も空白として扱う。
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

/// Python の `bytes.decode("utf-8", errors="ignore")` 相当。
/// 不正なバイトは（置換文字ではなく）黙って捨てる。
pub fn decode_utf8_ignore(mut data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    loop {
        match std::str::from_utf8(data) {
            Ok(s) => {
                out.push_str(s);
                return out;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(unsafe { std::str::from_utf8_unchecked(&data[..valid]) });
                match e.error_len() {
                    Some(len) => data = &data[valid + len..],
                    None => return out, // 末尾の不完全なシーケンスは捨てる
                }
            }
        }
    }
}

/// Python の str.casefold() 近似。
/// ASCII とほとんどの文字は to_lowercase() と一致する。casefold 固有の差
/// （ß→ss 等の完全ケースフォールド）のうち代表的なものだけ対応する。
/// ソートキーとしての利用が目的（ファイル名の大半は ASCII/CJK でどちらも影響なし）。
pub fn py_casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' | 'ẞ' => out.push_str("ss"),
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬀ' => out.push_str("ff"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'µ' => out.push('μ'),
            _ => {
                if c.is_ascii() {
                    out.push(c.to_ascii_lowercase());
                } else {
                    for lc in c.to_lowercase() {
                        out.push(lc);
                    }
                }
            }
        }
    }
    out
}

/// 文字列をコードポイント数で切り詰める（Python の s[:n] 相当）。
pub fn truncate_chars(s: &str, n: usize) -> (String, bool) {
    let mut it = s.char_indices();
    match it.nth(n) {
        Some((idx, _)) => (s[..idx].to_string(), true),
        None => (s.to_string(), false),
    }
}

/// コードポイント数（Python の len(str) 相当）。
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_even() {
        assert_eq!(py_round(2.5), 2);
        assert_eq!(py_round(3.5), 4);
        assert_eq!(py_round(-2.5), -2);
        assert_eq!(py_round(0.5), 0);
    }

    #[test]
    fn fmt_prec_matches_python() {
        // Python: f"{1.25:.1f}" == "1.2", f"{0.5:.0f}" == "0"
        assert_eq!(fmt_prec(1.25, 1), "1.2");
        assert_eq!(fmt_prec(0.5, 0), "0");
        assert_eq!(fmt_prec(1536.0 / 1024.0, 2), "1.50");
    }

    #[test]
    fn decode_ignore() {
        assert_eq!(decode_utf8_ignore(b"caf\xc3\xa9 \xff\xfe end"), "café  end");
        assert_eq!(decode_utf8_ignore(b"abc\xe3\x81"), "abc"); // 末尾の不完全シーケンス
    }
}
