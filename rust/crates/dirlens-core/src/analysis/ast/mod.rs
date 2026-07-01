//! AST 第1段（言語別最良パーサ）の共通アダプタ層。
//!
//! 方式は「言語別最良パーサ → 正規表現」の 2 段:
//! - 各関数は `None` を返すことで「このビルドでは未対応 / パース失敗」を表し、
//!   呼び出し側（extras / index）が正規表現抽出（第2段）へ縮退する。
//! - 純 Rust パーサ（rustpython / oxc / syn）は wasm でも動く。
//!   Go / C の tree-sitter は C 依存のため feature gate（wasm では無効）。

#[cfg(feature = "ast-js")]
pub mod js;
#[cfg(feature = "ast-python")]
pub mod python;
#[cfg(feature = "ast-rust")]
pub mod rustlang;

#[cfg(any(feature = "lang-go", feature = "lang-c"))]
pub mod treesitter;

use crate::fmt::OutlineItem;

/// このビルドで利用できる解析方式（--check / capabilities 出力用）。
#[derive(Debug, Clone, Copy)]
pub struct AstCapabilities {
    pub python: bool,
    pub js_ts: bool,
    pub rust: bool,
    pub go: bool,
    pub c: bool,
}

pub const CAPABILITIES: AstCapabilities = AstCapabilities {
    python: cfg!(feature = "ast-python"),
    js_ts: cfg!(feature = "ast-js"),
    rust: cfg!(feature = "ast-rust"),
    go: cfg!(feature = "lang-go"),
    c: cfg!(feature = "lang-c"),
};

/// AST によるアウトライン抽出。None → 正規表現へ縮退。
pub fn ast_outline(text: &str, ext: &str) -> Option<Vec<OutlineItem>> {
    match ext {
        #[cfg(feature = "ast-python")]
        ".py" => python::outline(text),
        #[cfg(feature = "ast-js")]
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" => js::outline(text, ext),
        #[cfg(feature = "ast-rust")]
        ".rs" => rustlang::outline(text),
        #[cfg(feature = "lang-go")]
        ".go" => treesitter::outline_go(text),
        #[cfg(feature = "lang-c")]
        ".c" | ".h" => treesitter::outline_c(text),
        _ => None,
    }
}

/// AST による Python import 抽出（ast.walk と同じ BFS 順）。None → 行ベースへ縮退。
pub fn ast_imports_py(text: &str) -> Option<Vec<(String, u32, Option<Vec<String>>)>> {
    #[cfg(feature = "ast-python")]
    {
        return python::imports(text);
    }
    #[allow(unreachable_code)]
    {
        let _ = text;
        None
    }
}

/// AST による JS/TS import 抽出。None → 正規表現へ縮退。
pub fn ast_imports_js(text: &str, ext: &str) -> Option<Vec<String>> {
    #[cfg(feature = "ast-js")]
    {
        return js::imports(text, ext);
    }
    #[allow(unreachable_code)]
    {
        let _ = (text, ext);
        None
    }
}

/// AST による Rust use/mod 抽出。None → 正規表現へ縮退。
/// uses は use ツリーを展開した "a::b::c" 形式（グループは複数エントリに展開）。
pub fn ast_imports_rs(text: &str) -> Option<(Vec<String>, Vec<String>)> {
    #[cfg(feature = "ast-rust")]
    {
        return rustlang::imports(text);
    }
    #[allow(unreachable_code)]
    {
        let _ = text;
        None
    }
}

/// AST による Go import 抽出。None → 正規表現へ縮退。
pub fn ast_imports_go(text: &str) -> Option<Vec<String>> {
    #[cfg(feature = "lang-go")]
    {
        return treesitter::imports_go(text);
    }
    #[allow(unreachable_code)]
    {
        let _ = text;
        None
    }
}
