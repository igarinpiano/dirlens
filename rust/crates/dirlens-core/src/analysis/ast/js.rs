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
