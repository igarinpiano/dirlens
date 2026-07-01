//! TODO/FIXME 抽出（-K）。dirlens.py の scan_todos の等価移植。

use std::sync::OnceLock;

use regex::Regex;

use crate::pyc::{char_len, py_strip, truncate_chars};

fn todo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(TODO|FIXME|HACK|XXX)\b").unwrap())
}

/// 戻り値: (行番号, 種別（大文字化）, スニペット)
pub fn scan_todos(text: &str) -> Vec<(usize, String, String)> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if let Some(m) = todo_re().captures(line) {
            let kind = m.get(1).unwrap().as_str().to_uppercase();
            let mut snippet = py_strip(line).to_string();
            if char_len(&snippet) > 80 {
                let (head, _) = truncate_chars(&snippet, 80);
                snippet = format!("{}…", head);
            }
            results.push((i + 1, kind, snippet));
        }
    }
    results
}
