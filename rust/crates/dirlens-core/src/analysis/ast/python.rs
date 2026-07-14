//! Python の AST 解析（rustpython-parser）。
//! CPython の `ast` モジュール相当の忠実度で outline / import を抽出する。

use rustpython_parser::ast::{Constant, Expr, Mod, Ranged, Stmt};
use rustpython_parser::{parse, Mode};

use crate::fmt::OutlineItem;

fn parse_module(text: &str) -> Option<Vec<Stmt>> {
    match parse(text, Mode::Module, "<dirlens>") {
        Ok(Mod::Module(m)) => Some(m.body),
        _ => None,
    }
}

/// 各行の開始バイトオフセット（行番号変換用・1行目 = index 0）。
fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

/// バイトオフセット → 1-indexed 行番号。
fn line_of(starts: &[usize], byte: usize) -> u32 {
    (starts.partition_point(|&s| s <= byte)) as u32
}

/// 本文先頭の docstring の1行目を返す。
fn docstring_head(body: &[Stmt]) -> Option<String> {
    if let Some(Stmt::Expr(e)) = body.first() {
        if let Expr::Constant(c) = e.value.as_ref() {
            if let Constant::Str(s) = &c.value {
                let head = s.lines().next().unwrap_or("").trim();
                if !head.is_empty() {
                    return Some(head.to_string());
                }
            }
        }
    }
    None
}

/// ソース順（再帰）で class / def を抽出する。
/// 公開判定は正規表現版と同じ「名前が _ 始まりでない」。
pub fn outline(text: &str) -> Option<Vec<OutlineItem>> {
    let body = parse_module(text)?;
    let starts = line_starts(text);
    let mut out = Vec::new();
    collect_outline(&body, &starts, &mut out);
    Some(out)
}

fn collect_outline(stmts: &[Stmt], starts: &[usize], out: &mut Vec<OutlineItem>) {
    for stmt in stmts {
        let span = {
            let r = stmt.range();
            Some((
                line_of(starts, r.start().to_usize()),
                line_of(starts, r.end().to_usize().saturating_sub(1).max(r.start().to_usize())),
            ))
        };
        match stmt {
            Stmt::ClassDef(c) => {
                let name = c.name.to_string();
                let public = !name.starts_with('_');
                let mut item = OutlineItem::new("class", name, public);
                item.doc = docstring_head(&c.body);
                item.span = span;
                out.push(item);
                collect_outline(&c.body, starts, out);
            }
            Stmt::FunctionDef(f) => {
                let name = f.name.to_string();
                let public = !name.starts_with('_');
                let mut item = OutlineItem::new("def", name, public);
                item.doc = docstring_head(&f.body);
                item.span = span;
                out.push(item);
                collect_outline(&f.body, starts, out);
            }
            Stmt::AsyncFunctionDef(f) => {
                let name = f.name.to_string();
                let public = !name.starts_with('_');
                let mut item = OutlineItem::new("def", name, public);
                item.doc = docstring_head(&f.body);
                item.span = span;
                out.push(item);
                collect_outline(&f.body, starts, out);
            }
            _ => {
                for child in child_bodies(stmt) {
                    collect_outline(child, starts, out);
                }
            }
        }
    }
}

/// import 抽出（CPython の ast.walk と同じ BFS 順）。
pub fn imports(text: &str) -> Option<Vec<(String, u32, Option<Vec<String>>)>> {
    let body = parse_module(text)?;
    let mut out = Vec::new();
    // BFS: レベルごとに文を処理する
    let mut queue: std::collections::VecDeque<&Stmt> = body.iter().collect();
    while let Some(stmt) = queue.pop_front() {
        match stmt {
            Stmt::Import(imp) => {
                for alias in &imp.names {
                    out.push((alias.name.to_string(), 0, None));
                }
            }
            Stmt::ImportFrom(imp) => {
                let module = imp
                    .module
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_default();
                let level = imp.level.map(|l| l.to_u32()).unwrap_or(0);
                let names: Vec<String> =
                    imp.names.iter().map(|a| a.name.to_string()).collect();
                out.push((module, level, Some(names)));
            }
            _ => {}
        }
        for child in child_bodies(stmt) {
            for s in child {
                queue.push_back(s);
            }
        }
    }
    Some(out)
}

/// 文が持つネストした本文（body 相当）を列挙する。
fn child_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::FunctionDef(s) => vec![&s.body],
        Stmt::AsyncFunctionDef(s) => vec![&s.body],
        Stmt::ClassDef(s) => vec![&s.body],
        Stmt::For(s) => vec![&s.body, &s.orelse],
        Stmt::AsyncFor(s) => vec![&s.body, &s.orelse],
        Stmt::While(s) => vec![&s.body, &s.orelse],
        Stmt::If(s) => vec![&s.body, &s.orelse],
        Stmt::With(s) => vec![&s.body],
        Stmt::AsyncWith(s) => vec![&s.body],
        Stmt::Try(s) => {
            let mut v: Vec<&[Stmt]> = vec![&s.body];
            for h in &s.handlers {
                let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                v.push(&h.body);
            }
            v.push(&s.orelse);
            v.push(&s.finalbody);
            v
        }
        Stmt::TryStar(s) => {
            let mut v: Vec<&[Stmt]> = vec![&s.body];
            for h in &s.handlers {
                let rustpython_parser::ast::ExceptHandler::ExceptHandler(h) = h;
                v.push(&h.body);
            }
            v.push(&s.orelse);
            v.push(&s.finalbody);
            v
        }
        Stmt::Match(s) => s.cases.iter().map(|c| c.body.as_slice()).collect(),
        _ => Vec::new(),
    }
}
