//! Rust の AST 解析（syn）。

use syn::spanned::Spanned;
use syn::{Attribute, Expr, ImplItem, Item, Lit, Meta, TraitItem, UseTree, Visibility};

use crate::fmt::OutlineItem;

fn is_pub(vis: &Visibility) -> bool {
    // 正規表現版の「行に pub を含む」相当: pub / pub(crate) 等はすべて公開扱い
    !matches!(vis, Visibility::Inherited)
}

/// `///` doc コメントの先頭1行（`#[doc = "..."]` 属性の最初のもの）。
fn doc_head(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(nv) = &attr.meta {
                if let Expr::Lit(l) = &nv.value {
                    if let Lit::Str(s) = &l.lit {
                        let head = s.value().trim().to_string();
                        if !head.is_empty() {
                            return Some(head);
                        }
                    }
                }
            }
        }
    }
    None
}

/// span → 1-indexed 行範囲（proc-macro2 の span-locations 前提）。
fn span_lines<T: Spanned>(node: &T) -> Option<(u32, u32)> {
    let s = node.span();
    let (a, b) = (s.start().line, s.end().line);
    if a == 0 {
        None // span-locations 無効時は 0 が返る
    } else {
        Some((a as u32, b as u32))
    }
}

fn item(kind: &str, name: String, public: bool, doc: Option<String>, span: Option<(u32, u32)>) -> OutlineItem {
    let mut it = OutlineItem::new(kind, name, public);
    it.doc = doc;
    it.span = span;
    it
}

/// impl の self 型名（パスの最終セグメント）。`impl Foo` / `impl Trait for Foo` の Foo。
fn impl_type_name(ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(tp) = ty {
        tp.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

/// ソース順で fn / struct / enum / trait を抽出する（mod / impl / trait 内も再帰）。
pub fn outline(text: &str) -> Option<Vec<OutlineItem>> {
    let file = syn::parse_file(text).ok()?;
    let mut out = Vec::new();
    collect_items(&file.items, &mut out);
    Some(out)
}

fn collect_items(items: &[Item], out: &mut Vec<OutlineItem>) {
    for it in items {
        match it {
            Item::Fn(f) => out.push(item(
                "fn",
                f.sig.ident.to_string(),
                is_pub(&f.vis),
                doc_head(&f.attrs),
                span_lines(f),
            )),
            Item::Struct(s) => out.push(item(
                "struct",
                s.ident.to_string(),
                is_pub(&s.vis),
                doc_head(&s.attrs),
                span_lines(s),
            )),
            Item::Enum(e) => out.push(item(
                "enum",
                e.ident.to_string(),
                is_pub(&e.vis),
                doc_head(&e.attrs),
                span_lines(e),
            )),
            Item::Trait(t) => {
                out.push(item(
                    "trait",
                    t.ident.to_string(),
                    is_pub(&t.vis),
                    doc_head(&t.attrs),
                    span_lines(t),
                ));
                for ti in &t.items {
                    if let TraitItem::Fn(f) = ti {
                        // trait 内メソッド: 明示的な pub は無いので非公開扱い
                        // （正規表現版の「行に pub を含むか」と同じ結果になる）
                        let mut it = item(
                            "fn",
                            f.sig.ident.to_string(),
                            false,
                            doc_head(&f.attrs),
                            span_lines(f),
                        );
                        it.parent = Some(t.ident.to_string());
                        out.push(it);
                    }
                }
            }
            Item::Impl(imp) => {
                // 複数 impl の同名メソッド（例: ModelSel::label と KeyMode::label）
                // を区別できるよう、self 型の名前を parent に入れる
                let self_ty = impl_type_name(&imp.self_ty);
                for ii in &imp.items {
                    if let ImplItem::Fn(f) = ii {
                        let mut it = item(
                            "fn",
                            f.sig.ident.to_string(),
                            is_pub(&f.vis),
                            doc_head(&f.attrs),
                            span_lines(f),
                        );
                        it.parent = self_ty.clone();
                        out.push(it);
                    }
                }
            }
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    collect_items(items, out);
                }
            }
            _ => {}
        }
    }
}

/// use / mod 抽出。use ツリーは展開して "a::b::c" 形式で返す
/// （`use a::{b, c};` → "a::b", "a::c"。glob は "a::*" → セグメントとしては a まで）。
pub fn imports(text: &str) -> Option<(Vec<String>, Vec<String>)> {
    let file = syn::parse_file(text).ok()?;
    let mut uses = Vec::new();
    let mut mods = Vec::new();
    collect_imports(&file.items, &mut uses, &mut mods);
    Some((uses, mods))
}

fn collect_imports(items: &[Item], uses: &mut Vec<String>, mods: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Use(u) => expand_use_tree(&u.tree, String::new(), uses),
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    collect_imports(items, uses, mods);
                } else {
                    // `mod foo;` 宣言のみ対象（正規表現版と同じ）
                    mods.push(m.ident.to_string());
                }
            }
            _ => {}
        }
    }
}

fn expand_use_tree(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    let join = |prefix: &str, seg: &str| {
        if prefix.is_empty() {
            seg.to_string()
        } else {
            format!("{}::{}", prefix, seg)
        }
    };
    match tree {
        UseTree::Path(p) => expand_use_tree(&p.tree, join(&prefix, &p.ident.to_string()), out),
        UseTree::Name(n) => out.push(join(&prefix, &n.ident.to_string())),
        UseTree::Rename(r) => out.push(join(&prefix, &r.ident.to_string())),
        UseTree::Glob(_) => out.push(join(&prefix, "*")),
        UseTree::Group(g) => {
            for t in &g.items {
                expand_use_tree(t, prefix.clone(), out);
            }
        }
    }
}
