//! JS/TS の AST 解析（oxc）。
//!
//! アウトラインは宣言ベース（トップレベル＋export）。import は
//! import/export 文を AST から、require()/動的 import() は正規表現で補完する
//! （実行時呼び出しはどこにでも書けるため）。

use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Expression, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::fmt::OutlineItem;

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

fn decl_items(decl: &Declaration, exported: bool, out: &mut Vec<OutlineItem>) {
    match decl {
        Declaration::ClassDeclaration(c) => {
            if let Some(id) = &c.id {
                out.push(("class".to_string(), id.name.to_string(), exported));
            }
        }
        Declaration::FunctionDeclaration(f) => {
            if let Some(id) = &f.id {
                out.push(("func".to_string(), id.name.to_string(), exported));
            }
        }
        Declaration::VariableDeclaration(v) => {
            for d in &v.declarations {
                if let Some(init) = &d.init {
                    if is_function_init(init) {
                        if let Some(name) = d.id.get_identifier_name() {
                            out.push(("func".to_string(), name.to_string(), exported));
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
    let mut out = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    out.push(("class".to_string(), id.name.to_string(), false));
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    out.push(("func".to_string(), id.name.to_string(), false));
                }
            }
            Statement::VariableDeclaration(v) => {
                for d in &v.declarations {
                    if let Some(init) = &d.init {
                        if is_function_init(init) {
                            if let Some(name) = d.id.get_identifier_name() {
                                out.push(("func".to_string(), name.to_string(), false));
                            }
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(e) => {
                if let Some(decl) = &e.declaration {
                    decl_items(decl, true, &mut out);
                }
            }
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    if let Some(id) = &c.id {
                        out.push(("class".to_string(), id.name.to_string(), true));
                    }
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        out.push(("func".to_string(), id.name.to_string(), true));
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
