//! シンボルアウトライン（-O / -A）。
//!
//! 現段階は dirlens.py の正規表現ベース抽出（extract_outline）の等価移植。
//! 言語別 AST パーサ（ruff/oxc/syn/tree-sitter）による第1段は後続ステージで
//! この上に重ね、失敗時にこの正規表現抽出へ縮退する。

use std::sync::OnceLock;

use regex::Regex;

use crate::fmt::OutlineItem;

struct Pat {
    re: Regex,
    kind: &'static str,
    name_group: usize,
}

fn pats(ext: &str) -> Option<&'static [Pat]> {
    static PY: OnceLock<Vec<Pat>> = OnceLock::new();
    static JS: OnceLock<Vec<Pat>> = OnceLock::new();
    static GO: OnceLock<Vec<Pat>> = OnceLock::new();
    static RS: OnceLock<Vec<Pat>> = OnceLock::new();
    let p = |re: &str, kind: &'static str, name_group: usize| Pat {
        re: Regex::new(re).unwrap(),
        kind,
        name_group,
    };
    match ext {
        ".py" => Some(PY.get_or_init(|| {
            vec![
                p(r"^(\s*)class\s+(\w+)", "class", 2),
                p(r"^(\s*)(?:async\s+)?def\s+(\w+)\s*\(", "def", 2),
            ]
        })),
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" => Some(JS.get_or_init(|| {
            vec![
                p(r"^\s*(?:export\s+)?(?:default\s+)?class\s+(\w+)", "class", 1),
                p(
                    r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s+(\w+)\s*\(",
                    "func",
                    1,
                ),
                p(
                    r"^\s*export\s+(?:default\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(",
                    "func",
                    1,
                ),
                p(
                    r"^\s*(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(.*\)\s*=>",
                    "func",
                    1,
                ),
            ]
        })),
        ".go" => Some(GO.get_or_init(|| {
            vec![
                p(r"^func\s+(?:\([^)]*\)\s+)?(\w+)\s*\(", "func", 1),
                p(r"^type\s+(\w+)\s+struct", "struct", 1),
                p(r"^type\s+(\w+)\s+interface", "interface", 1),
            ]
        })),
        ".rs" => Some(RS.get_or_init(|| {
            vec![
                p(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)", "fn", 1),
                p(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)", "struct", 1),
                p(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)", "enum", 1),
                p(r"^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)", "trait", 1),
            ]
        })),
        _ => None,
    }
}

fn export_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bexport\b").unwrap())
}

fn pub_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bpub\b").unwrap())
}

/// 言語ごとの公開API判定（best-effort、dirlens.py の _is_public_symbol 相当）。
pub fn is_public_symbol(ext: &str, line: &str, name: &str) -> bool {
    match ext {
        ".py" => !name.starts_with('_'),
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" => export_re().is_match(line),
        ".go" => name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false),
        ".rs" => pub_re().is_match(line),
        _ => true, // 不明な場合は除外しない（保守的に倒す）
    }
}

const LIMIT_LINES: usize = 4000;

/// 対応言語の関数・クラス名を正規表現で簡易抽出する。
/// 対応外の拡張子は None（「対応していない」ことを空リストと区別する）。
pub fn extract_outline(text: &str, ext: &str) -> Option<Vec<OutlineItem>> {
    let patterns = pats(ext)?;
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for line in text.split('\n').take(LIMIT_LINES) {
        for pat in patterns {
            // Python は re.match（行頭アンカー）。パターン自体が ^ 始まりなので同義。
            if let Some(m) = pat.re.captures(line) {
                let name = m.get(pat.name_group).map(|g| g.as_str()).unwrap_or("");
                out.push(OutlineItem::new(
                    pat.kind,
                    name.to_string(),
                    is_public_symbol(ext, line, name),
                ));
                break;
            }
        }
    }
    Some(out)
}
