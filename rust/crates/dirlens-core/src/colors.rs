//! ANSI カラー（dirlens.py のカラー設定と同一のコード）。

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const BLUE: &str = "\x1b[34m";
pub const CYAN: &str = "\x1b[36m";
pub const GREEN: &str = "\x1b[32m";
pub const MAGENTA: &str = "\x1b[35m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";

/// Python の `c(text, *codes)` 相当。
pub fn c(text: &str, codes: &[&str], use_color: bool) -> String {
    if use_color {
        format!("{}{}{}", codes.concat(), text, RESET)
    } else {
        text.to_string()
    }
}

/// Python の `strip_ansi`（`\033\[[0-9;]*[mK]` を除去）相当。
pub fn strip_ansi(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '[' {
            let mut j = i + 2;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == ';') {
                j += 1;
            }
            if j < chars.len() && (chars[j] == 'm' || chars[j] == 'K') {
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
