//! CPython の fnmatch モジュール互換のグロブマッチャ。
//!
//! 重要な互換ポイント:
//! - `*` は `/` を含む任意の文字列にマッチする（gitignore の `*` とは異なる）。
//! - `?` は任意の 1 文字。
//! - `[seq]` は文字クラス（`[!seq]` で否定、先頭 `]` はリテラル、`a-z` は
//!   コードポイント範囲）。閉じ `]` が無い場合は `[` をリテラル扱い。
//! - fnmatch.fnmatch() は os.path.normcase を通す（Windows では小文字化、
//!   POSIX では恒等）。fnmatchcase 相当は `casefold=false` で呼ぶ。

#[derive(Debug, Clone)]
enum Tok {
    Star,
    Any,
    Lit(char),
    Class { neg: bool, ranges: Vec<(char, char)> },
}

fn parse(pat: &[char]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let n = pat.len();
    let mut i = 0;
    while i < n {
        let c = pat[i];
        match c {
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '?' => {
                toks.push(Tok::Any);
                i += 1;
            }
            '[' => {
                // CPython fnmatch.translate と同じ走査: '!' と先頭 ']' を特別扱いし、
                // 閉じ ']' が見つからなければ '[' をリテラルにする。
                let mut j = i + 1;
                if j < n && pat[j] == '!' {
                    j += 1;
                }
                if j < n && pat[j] == ']' {
                    j += 1;
                }
                while j < n && pat[j] != ']' {
                    j += 1;
                }
                if j >= n {
                    toks.push(Tok::Lit('['));
                    i += 1;
                } else {
                    let mut k = i + 1;
                    let neg = pat[k] == '!';
                    if neg {
                        k += 1;
                    }
                    let stuff: Vec<char> = pat[k..j].to_vec();
                    let mut ranges = Vec::new();
                    let m = stuff.len();
                    let mut p = 0;
                    while p < m {
                        if p + 2 < m && stuff[p + 1] == '-' {
                            ranges.push((stuff[p], stuff[p + 2]));
                            p += 3;
                        } else {
                            ranges.push((stuff[p], stuff[p]));
                            p += 1;
                        }
                    }
                    toks.push(Tok::Class { neg, ranges });
                    i = j + 1;
                }
            }
            _ => {
                toks.push(Tok::Lit(c));
                i += 1;
            }
        }
    }
    toks
}

fn class_match(c: char, neg: bool, ranges: &[(char, char)]) -> bool {
    let hit = ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi);
    hit != neg
}

fn match_toks(toks: &[Tok], name: &[char]) -> bool {
    // 古典的な「最後の * に戻る」バックトラッキング
    let (mut ti, mut ni) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ni < name.len() {
        if ti < toks.len() {
            match &toks[ti] {
                Tok::Star => {
                    star = Some((ti, ni));
                    ti += 1;
                    continue;
                }
                Tok::Any => {
                    ti += 1;
                    ni += 1;
                    continue;
                }
                Tok::Lit(l) => {
                    if *l == name[ni] {
                        ti += 1;
                        ni += 1;
                        continue;
                    }
                }
                Tok::Class { neg, ranges } => {
                    if class_match(name[ni], *neg, ranges) {
                        ti += 1;
                        ni += 1;
                        continue;
                    }
                }
            }
        }
        match star {
            Some((sti, sni)) => {
                ti = sti + 1;
                ni = sni + 1;
                star = Some((sti, sni + 1));
            }
            None => return false,
        }
    }
    while ti < toks.len() {
        if matches!(toks[ti], Tok::Star) {
            ti += 1;
        } else {
            return false;
        }
    }
    true
}

/// fnmatch.fnmatchcase 相当（大文字小文字をそのまま比較）。
pub fn fnmatch_case(name: &str, pat: &str) -> bool {
    let toks = parse(&pat.chars().collect::<Vec<_>>());
    match_toks(&toks, &name.chars().collect::<Vec<_>>())
}

/// fnmatch.fnmatch 相当。Windows では normcase（小文字化）を通す。
pub fn fnmatch(name: &str, pat: &str) -> bool {
    #[cfg(windows)]
    {
        fnmatch_case(&name.to_lowercase(), &pat.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        fnmatch_case(name, pat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert!(fnmatch_case("foo.md", "*.md"));
        assert!(!fnmatch_case("foo.mdx", "*.md"));
        assert!(fnmatch_case("a/b/c.md", "*.md")); // '*' は '/' を跨ぐ（CPython 互換）
        assert!(fnmatch_case("abc", "a?c"));
        assert!(fnmatch_case("a-c", "a[-x]c"));
        assert!(fnmatch_case("abc", "a[a-z]c"));
        assert!(!fnmatch_case("aBc", "a[a-z]c"));
        assert!(fnmatch_case("aBc", "a[!a-z]c"));
        assert!(fnmatch_case("a]c", "a[]]c"));
        assert!(fnmatch_case("a[c", "a[c")); // 閉じ ] 無し → リテラル
        assert!(fnmatch_case("[bracket].txt", "[bracket].txt") == false); // クラス扱い
        assert!(fnmatch_case("b.txt", "[bracket].txt"));
    }
}
