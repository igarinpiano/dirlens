//! JS/TS の AST 解析（oxc）。
//!
//! アウトラインは宣言ベース（トップレベル＋export）。import は
//! import/export 文を AST から、require()/動的 import() は正規表現で補完する
//! （実行時呼び出しはどこにでも書けるため）。

use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Expression, Statement};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};

use crate::fmt::OutlineItem;

/// 各行の開始バイトオフセット（行番号変換用）。
fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

fn line_of(starts: &[usize], byte: usize) -> u32 {
    starts.partition_point(|&s| s <= byte) as u32
}

fn item(
    kind: &str,
    name: String,
    public: bool,
    span: oxc_span::Span,
    starts: &[usize],
) -> OutlineItem {
    let mut it = OutlineItem::new(kind, name, public);
    it.span = Some((
        line_of(starts, span.start as usize),
        line_of(starts, (span.end as usize).saturating_sub(1).max(span.start as usize)),
    ));
    it
}

fn source_type(ext: &str) -> SourceType {
    match ext {
        ".ts" => SourceType::ts(),
        ".tsx" => SourceType::tsx(),
        ".jsx" => SourceType::jsx(),
        ".mjs" => SourceType::mjs(),
        ".cjs" => SourceType::cjs(),
        _ => SourceType::default().with_module(true),
    }
}

fn parse_program<'a>(
    alloc: &'a Allocator,
    text: &'a str,
    ext: &str,
) -> Option<oxc_ast::ast::Program<'a>> {
    let ret = Parser::new(alloc, text, source_type(ext)).parse();
    if ret.panicked || !ret.diagnostics.is_empty() {
        return None;
    }
    Some(ret.program)
}

/// 変数宣言の初期化子が関数（アロー/関数式）かどうか。
fn is_function_init(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
    )
}

fn decl_items(decl: &Declaration, exported: bool, starts: &[usize], out: &mut Vec<OutlineItem>) {
    match decl {
        Declaration::ClassDeclaration(c) => {
            if let Some(id) = &c.id {
                out.push(item("class", id.name.to_string(), exported, c.span, starts));
            }
        }
        Declaration::FunctionDeclaration(f) => {
            if let Some(id) = &f.id {
                out.push(item("func", id.name.to_string(), exported, f.span, starts));
            }
        }
        Declaration::VariableDeclaration(v) => {
            for d in &v.declarations {
                if let Some(init) = &d.init {
                    if is_function_init(init) {
                        if let Some(name) = d.id.get_identifier_name() {
                            out.push(item("func", name.to_string(), exported, init.span(), starts));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn outline(text: &str, ext: &str) -> Option<Vec<OutlineItem>> {
    let alloc = Allocator::default();
    let program = parse_program(&alloc, text, ext)?;
    let starts = line_starts(text);
    let mut out = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    out.push(item("class", id.name.to_string(), false, c.span, &starts));
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    out.push(item("func", id.name.to_string(), false, f.span, &starts));
                }
            }
            Statement::VariableDeclaration(v) => {
                for d in &v.declarations {
                    if let Some(init) = &d.init {
                        if is_function_init(init) {
                            if let Some(name) = d.id.get_identifier_name() {
                                out.push(item(
                                    "func",
                                    name.to_string(),
                                    false,
                                    init.span(),
                                    &starts,
                                ));
                            }
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(e) => {
                if let Some(decl) = &e.declaration {
                    decl_items(decl, true, &starts, &mut out);
                }
            }
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        out.push(item("class", id.name.to_string(), true, c.span, &starts));
                    }
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        out.push(item("func", id.name.to_string(), true, f.span, &starts));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    Some(out)
}

/// `type` 属性の値が JS として解析すべきものか（無指定・JS MIME・module のみ）。
/// JSON・importmap・テンプレート等の非 JS ブロックは除外する。
fn script_type_is_js(attrs_lower: &str) -> bool {
    static TYPE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = TYPE_RE.get_or_init(|| {
        regex::Regex::new(r#"\btype\s*=\s*["']?\s*([^"'\s>]*)"#).unwrap()
    });
    match re.captures(attrs_lower) {
        None => true,
        Some(m) => matches!(
            m.get(1).map(|g| g.as_str()).unwrap_or(""),
            "" | "text/javascript"
                | "application/javascript"
                | "text/ecmascript"
                | "application/ecmascript"
                | "module"
        ),
    }
}

/// HTML 内のインライン `<script>` ブロック（src 属性なし・JS タイプのみ）を
/// 抽出し、各ブロックを JS としてアウトラインする。行番号は HTML ファイル
/// 全体の行に合わせてオフセットする。AST パースに失敗したブロックは
/// 正規表現抽出（行範囲なし）へ縮退する。
pub fn html_outline(text: &str) -> Option<Vec<OutlineItem>> {
    static SRC_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let src_re = SRC_RE.get_or_init(|| regex::Regex::new(r"\bsrc\s*=").unwrap());

    // ASCII 小文字化はバイト長を変えないため、lower 上のオフセットを
    // そのまま原文 text のスライスに使える
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<OutlineItem> = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = lower[pos..].find("<script") {
        let open = pos + open_rel;
        let Some(tag_end_rel) = lower[open..].find('>') else { break };
        let tag_end = open + tag_end_rel;
        let attrs = &lower[open + "<script".len()..tag_end];
        let self_closing = attrs.trim_end().ends_with('/');
        let body_start = tag_end + 1;
        let Some(close_rel) = lower[body_start..].find("</script") else { break };
        let close = body_start + close_rel;

        if !self_closing && !src_re.is_match(attrs) && script_type_is_js(attrs) {
            let content = &text[body_start..close];
            let line_offset = text[..body_start].bytes().filter(|&b| b == b'\n').count() as u32;
            let items = outline(content, ".js")
                .or_else(|| crate::analysis::outline::extract_outline(content, ".js"));
            if let Some(items) = items {
                for mut it in items {
                    if let Some((a, b)) = it.span {
                        it.span = Some((a + line_offset, b + line_offset));
                    }
                    out.push(it);
                }
            }
        }
        pos = close + "</script".len();
    }
    Some(out)
}

pub fn imports(text: &str, ext: &str) -> Option<Vec<String>> {
    let alloc = Allocator::default();
    let program = parse_program(&alloc, text, ext)?;
    let mut found = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(i) => {
                found.push(i.source.value.to_string());
            }
            Statement::ExportNamedDeclaration(e) => {
                if let Some(src) = &e.source {
                    found.push(src.value.to_string());
                }
            }
            Statement::ExportAllDeclaration(e) => {
                found.push(e.source.value.to_string());
            }
            _ => {}
        }
    }
    // require() / 動的 import() は正規表現で補完（第2段と同じパターン）
    for pat in super::super::index::js_call_patterns() {
        for m in pat.captures_iter(text) {
            found.push(m.get(1).unwrap().as_str().to_string());
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::html_outline;

    #[test]
    fn html_inline_scripts_with_line_offsets() {
        let html = "<!doctype html>\n<html>\n<head>\n<script>\nfunction alpha() {\n  return 1;\n}\nconst beta = () => 2;\n</script>\n</head>\n<body>\n<script type=\"module\">\nfunction gamma() {}\n</script>\n</body>\n</html>\n";
        let items = html_outline(html).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
        // alpha は HTML 全体では 5〜7 行目
        assert_eq!(items[0].span, Some((5, 7)));
        assert_eq!(items[2].span, Some((13, 13)));
    }

    #[test]
    fn html_skips_external_and_non_js_scripts() {
        let html = concat!(
            "<script src=\"app.js\"></script>\n",
            "<script type=\"application/json\">{\"function\": \"not js\"}</script>\n",
            "<SCRIPT>function upper() {}</SCRIPT>\n",
        );
        let items = html_outline(html).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["upper"]);
    }

    #[test]
    fn html_broken_script_falls_back_to_regex() {
        // 構文エラーのブロックは AST で拾えないが、正規表現縮退で関数名は残す
        let html = "<script>\nfunction ok() {}\nfunction broken( {\n</script>\n";
        let items = html_outline(html).unwrap();
        assert!(items.iter().any(|i| i.name == "ok"));
    }

    #[test]
    fn html_without_scripts_is_empty_not_none() {
        assert_eq!(html_outline("<p>hello</p>").unwrap().len(), 0);
    }
}
