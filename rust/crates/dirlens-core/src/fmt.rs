//! フォーマット関数群（dirlens.py の「フォーマット」セクションの等価移植）。

use crate::pyc::{char_len, fmt_prec, py_round, py_strip, py_trunc, rstrip_zeros, truncate_chars};

/// os.path.splitext 相当（名前部分のみを対象）。ext は '.' を含む。
/// ".env" のような先頭ドットは拡張子とみなさない（CPython 互換）。
pub fn splitext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        // 先頭からセパレータ位置まで全部 '.' なら拡張子なし（".env" 等）
        Some(sep) if sep > 0 && !name[..sep].chars().all(|c| c == '.') => {
            (&name[..sep], &name[sep..])
        }
        _ => (name, ""),
    }
}

pub fn fmt_size(n: u64, partial: bool) -> String {
    let sfx = if partial { "+" } else { "" };
    if n == 0 {
        return format!("0{} bytes", sfx);
    }
    for (unit, f) in [
        ("TB", 1u64 << 40),
        ("GB", 1u64 << 30),
        ("MB", 1u64 << 20),
        ("KB", 1u64 << 10),
    ] {
        if n >= f {
            let num = rstrip_zeros(&fmt_prec(n as f64 / f as f64, 2));
            return format!("{}{} {}", num, sfx, unit);
        }
    }
    let word = if n == 1 && !partial { "byte" } else { "bytes" };
    format!("{}{} {}", n, sfx, word)
}

pub fn fmt_count(nd: usize, nf: usize, denied: bool) -> String {
    let sfx = if denied { "+" } else { "" };
    let d_word = if nd == 1 && !denied { "dir" } else { "dirs" };
    let f_word = if nf == 1 && !denied { "file" } else { "files" };
    format!("{}{} {}, {}{} {}", nd, sfx, d_word, nf, sfx, f_word)
}

/// now / mtime は epoch 秒。Python 版は naive datetime の差分だが、
/// DST の無いタイムゾーン（テストでは TZ=UTC）では epoch 差分と一致する。
pub fn fmt_date(now: f64, mtime: f64) -> String {
    let sec = py_trunc(now - mtime);
    if sec < 60 {
        return "今".to_string();
    }
    if sec < 3600 {
        return format!("{}分前", sec / 60);
    }
    if sec < 86400 {
        return format!("{}時間前", sec / 3600);
    }
    let d = sec / 86400;
    if d == 1 {
        return "昨日".to_string();
    }
    if d < 7 {
        return format!("{}日前", d);
    }
    if d < 30 {
        return format!("{}週間前", d / 7);
    }
    if d < 365 {
        return format!("{}ヶ月前", d / 30);
    }
    format!("{}年前", d / 365)
}

pub fn fmt_bar(part: u64, total: u64, width: i64) -> String {
    let pct = if total != 0 {
        std::cmp::min(100, py_trunc(part as f64 * 100.0 / total as f64))
    } else {
        0
    };
    let filled = py_round(pct as f64 * width as f64 / 100.0);
    let empty = width - filled;
    format!(
        "[{}{}]{:4}%",
        "█".repeat(filled.max(0) as usize),
        "░".repeat(empty.max(0) as usize),
        pct
    )
}

/// parse_size: "50M" / "1G" / "500K" / 素の整数。エラー時は Python と同じ文言を返す。
pub fn parse_size(s: &str) -> Result<i64, String> {
    let s = py_strip(s);
    let upper = s.to_uppercase();
    for (sfx, mult) in [
        ("TB", 1i64 << 40),
        ("GB", 1i64 << 30),
        ("MB", 1i64 << 20),
        ("KB", 1i64 << 10),
        ("T", 1i64 << 40),
        ("G", 1i64 << 30),
        ("M", 1i64 << 20),
        ("K", 1i64 << 10),
    ] {
        if upper.ends_with(sfx) {
            let head = &s[..s.len() - sfx.len()];
            match head.trim().parse::<f64>() {
                Ok(v) => return Ok(py_trunc(v * mult as f64)),
                Err(_) => break, // Python 同様、int(s) の解釈へフォールバック
            }
        }
    }
    s.parse::<i64>()
        .map_err(|_| format!("無効なサイズ: '{}'（例: 50M, 1G, 500K）", s))
}

pub fn fmt_tokens(n: i64) -> String {
    if n >= 1000 {
        let s = rstrip_zeros(&fmt_prec(n as f64 / 1000.0, 1));
        format!("~{}K tok", s)
    } else {
        format!("~{} tok", n)
    }
}

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub hash: String,
    pub date: String,
    pub author: String,
    pub subject: String,
}

pub fn fmt_git(g: &GitInfo) -> String {
    let mut subj = py_strip(&g.subject).to_string();
    if char_len(&subj) > 30 {
        let (head, _) = truncate_chars(&subj, 30);
        subj = format!("{}…", head);
    }
    format!("\"{}\" ({})", subj, g.date)
}

/// アウトライン 1 項目: (kind, name, is_public)
pub type OutlineItem = (String, String, bool);

pub fn fmt_outline(outline: &[OutlineItem], limit: usize) -> Option<String> {
    if outline.is_empty() {
        return None;
    }
    let items: Vec<String> = outline
        .iter()
        .map(|(kind, name, _)| format!("{} {}", kind, name))
        .collect();
    let shown = &items[..items.len().min(limit)];
    let mut s = shown.join(", ");
    if items.len() > limit {
        s += &format!(", +{}", items.len() - limit);
    }
    Some(s)
}

/// CPython の stat.filemode() 相当。
pub fn filemode(mode: u32) -> String {
    let mut out = String::with_capacity(10);
    let ifmt = mode & 0o170000;
    out.push(match ifmt {
        0o120000 => 'l',
        0o140000 => 's',
        0o100000 => '-',
        0o060000 => 'b',
        0o040000 => 'd',
        0o020000 => 'c',
        0o010000 => 'p',
        _ => '?',
    });
    let triples = [
        (0o400, 0o200, 0o100, 0o4000, 's', 'S'), // user + setuid
        (0o040, 0o020, 0o010, 0o2000, 's', 'S'), // group + setgid
        (0o004, 0o002, 0o001, 0o1000, 't', 'T'), // other + sticky
    ];
    for (r, w, x, special, both, only_special) in triples {
        out.push(if mode & r != 0 { 'r' } else { '-' });
        out.push(if mode & w != 0 { 'w' } else { '-' });
        let has_x = mode & x != 0;
        let has_s = mode & special != 0;
        out.push(match (has_x, has_s) {
            (true, true) => both,
            (false, true) => only_special,
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(fmt_size(0, false), "0 bytes");
        assert_eq!(fmt_size(1, false), "1 byte");
        assert_eq!(fmt_size(1, true), "1+ bytes");
        assert_eq!(fmt_size(1024, false), "1 KB");
        assert_eq!(fmt_size(1536, false), "1.5 KB");
        assert_eq!(fmt_size(1587, false), "1.55 KB");
        assert_eq!(fmt_size(307200, false), "300 KB");
    }

    #[test]
    fn splitext_compat() {
        assert_eq!(splitext("foo.py"), ("foo", ".py"));
        assert_eq!(splitext(".env"), (".env", ""));
        assert_eq!(splitext(".env.local"), (".env", ".local"));
        assert_eq!(splitext("Makefile"), ("Makefile", ""));
        assert_eq!(splitext("a.tar.gz"), ("a.tar", ".gz"));
        assert_eq!(splitext("..."), ("...", ""));
        assert_eq!(splitext("..a.b"), ("..a", ".b"));
    }

    #[test]
    fn dates() {
        assert_eq!(fmt_date(1000.0, 990.0), "今");
        assert_eq!(fmt_date(90.0 * 60.0, 0.0), "1時間前");
        assert_eq!(fmt_date(100.0 * 86400.0, 0.0), "3ヶ月前");
        assert_eq!(fmt_date(30.0 * 3600.0, 0.0), "昨日");
    }

    #[test]
    fn parse_sizes() {
        assert_eq!(parse_size("1K"), Ok(1024));
        assert_eq!(parse_size("1.5M"), Ok(1572864));
        assert_eq!(parse_size("2000"), Ok(2000));
        assert!(parse_size("xyz").is_err());
    }
}
