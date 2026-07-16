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

/// 公開判定用のスコープ文脈。
/// - Module: モジュール直下（if / try 等の制御ブロック内も含む）
/// - Class(bool): クラス本体（bool = そのクラス自身が公開 API か）
/// - Function: 関数本体（この中の def / class はローカル定義であり公開 API ではない）
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Module,
    Class(bool),
    Function,
}

/// 文脈を踏まえた公開判定。名前の `_` 始まりに加えて、
/// 関数内のローカル定義は常に非公開、非公開クラスのメンバも非公開とする
/// （モジュール外から `module.name` で到達できるものだけを公開 API と数える）。
fn is_public(name: &str, scope: Scope) -> bool {
    match scope {
        Scope::Function => false,
        Scope::Class(class_public) => class_public && !name.starts_with('_'),
        Scope::Module => !name.starts_with('_'),
    }
}

/// ソース順（再帰）で class / def を抽出する。
/// 公開判定はスコープ文脈つき（関数内ローカル定義・非公開クラスのメンバは
/// 非公開）。v1.2.8 以前は正規表現版と同じ「名前が _ 始まりでない」だけで
/// 判定しており、関数内の def まで公開 API として過剰報告されていた。
pub fn outline(text: &str) -> Option<Vec<OutlineItem>> {
    let body = parse_module(text)?;
    let starts = line_starts(text);
    let mut out = Vec::new();
    collect_outline(&body, &starts, Scope::Module, None, &mut out);
    Some(out)
}

fn collect_outline(
    stmts: &[Stmt],
    starts: &[usize],
    scope: Scope,
    parent: Option<&str>,
    out: &mut Vec<OutlineItem>,
) {
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
                let public = is_public(&name, scope);
                let mut item = OutlineItem::new("class", name.clone(), public);
                item.doc = docstring_head(&c.body);
                item.span = span;
                item.parent = parent.map(str::to_string);
                out.push(item);
                // 関数内のクラスはローカル定義のままメンバも非公開
                let inner = if scope == Scope::Function {
                    Scope::Function
                } else {
                    Scope::Class(public)
                };
                collect_outline(&c.body, starts, inner, Some(&name), out);
            }
            Stmt::FunctionDef(f) => {
                let name = f.name.to_string();
                let public = is_public(&name, scope);
                let mut item = OutlineItem::new("def", name.clone(), public);
                item.doc = docstring_head(&f.body);
                item.span = span;
                item.parent = parent.map(str::to_string);
                out.push(item);
                collect_outline(&f.body, starts, Scope::Function, Some(&name), out);
            }
            Stmt::AsyncFunctionDef(f) => {
                let name = f.name.to_string();
                let public = is_public(&name, scope);
                let mut item = OutlineItem::new("def", name.clone(), public);
                item.doc = docstring_head(&f.body);
                item.span = span;
                item.parent = parent.map(str::to_string);
                out.push(item);
                collect_outline(&f.body, starts, Scope::Function, Some(&name), out);
            }
            _ => {
                // if / try / with 等の制御ブロックはスコープも parent も変えない
                // （`if TYPE_CHECKING:` 直下の def はモジュールレベル扱い）
                for child in child_bodies(stmt) {
                    collect_outline(child, starts, scope, parent, out);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn publics(text: &str) -> Vec<(String, bool)> {
        outline(text)
            .unwrap()
            .into_iter()
            .map(|it| (it.name, it.public))
            .collect()
    }

    #[test]
    fn local_defs_are_not_public() {
        let src = "def run_cycle():\n    def trial():\n        pass\n    class Inner:\n        def m(self):\n            pass\n    return trial\n";
        let got = publics(src);
        assert_eq!(
            got,
            vec![
                ("run_cycle".to_string(), true),
                ("trial".to_string(), false),
                ("Inner".to_string(), false),
                ("m".to_string(), false),
            ]
        );
    }

    #[test]
    fn methods_follow_class_visibility() {
        let src = "class Server:\n    def rpc(self):\n        pass\n    def _hidden(self):\n        pass\n\nclass _Private:\n    def load(self):\n        pass\n";
        let got = publics(src);
        assert_eq!(
            got,
            vec![
                ("Server".to_string(), true),
                ("rpc".to_string(), true),
                ("_hidden".to_string(), false),
                ("_Private".to_string(), false),
                ("load".to_string(), false),
            ]
        );
    }

    #[test]
    fn parent_is_nearest_enclosing_symbol() {
        let src = "def outer():\n    def inner():\n        pass\n\nclass Server:\n    def rpc(self):\n        pass\n";
        let items = outline(src).unwrap();
        let parents: Vec<(String, Option<String>)> =
            items.into_iter().map(|it| (it.name, it.parent)).collect();
        assert_eq!(
            parents,
            vec![
                ("outer".to_string(), None),
                ("inner".to_string(), Some("outer".to_string())),
                ("Server".to_string(), None),
                ("rpc".to_string(), Some("Server".to_string())),
            ]
        );
    }

    #[test]
    fn control_blocks_keep_module_scope() {
        // if / try 直下の def はモジュールレベル（公開判定は名前のみ）
        let src = "if True:\n    def conditional():\n        pass\ntry:\n    def fallback():\n        pass\nexcept ImportError:\n    pass\n";
        let got = publics(src);
        assert_eq!(
            got,
            vec![
                ("conditional".to_string(), true),
                ("fallback".to_string(), true),
            ]
        );
    }
}
