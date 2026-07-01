//! Rust の AST 解析（syn）。

use syn::{ImplItem, Item, TraitItem, UseTree, Visibility};

use crate::fmt::OutlineItem;

fn is_pub(vis: &Visibility) -> bool {
    // 正規表現版の「行に pub を含む」相当: pub / pub(crate) 等はすべて公開扱い
    !matches!(vis, Visibility::Inherited)
}

/// ソース順で fn / struct / enum / trait を抽出する（mod / impl / trait 内も再帰）。
pub fn outline(text: &str) -> Option<Vec<OutlineItem>> {
    let file = syn::parse_file(text).ok()?;
    let mut out = Vec::new();
    collect_items(&file.items, &mut out);
    Some(out)
}

fn collect_items(items: &[Item], out: &mut Vec<OutlineItem>) {
    for item in items {
        match item {
            Item::Fn(f) => out.push((
                "fn".to_string(),
                f.sig.ident.to_string(),
                is_pub(&f.vis),
            )),
            Item::Struct(s) => out.push((
                "struct".to_string(),
                s.ident.to_string(),
                is_pub(&s.vis),
            )),
            Item::Enum(e) => out.push((
                "enum".to_string(),
                e.ident.to_string(),
                is_pub(&e.vis),
            )),
            Item::Trait(t) => {
                out.push(("trait".to_string(), t.ident.to_string(), is_pub(&t.vis)));
                for ti in &t.items {
                    if let TraitItem::Fn(f) = ti {
                        // trait 内メソッド: 明示的な pub は無いので非公開扱い
                        // （正規表現版の「行に pub を含むか」と同じ結果になる）
                        out.push(("fn".to_string(), f.sig.ident.to_string(), false));
                    }
                }
            }
            Item::Impl(imp) => {
                for ii in &imp.items {
                    if let ImplItem::Fn(f) = ii {
                        out.push(("fn".to_string(), f.sig.ident.to_string(), is_pub(&f.vis)));
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
