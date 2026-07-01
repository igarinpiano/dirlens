//! Go / C の AST 解析（tree-sitter・feature gate）。
//! C 依存があるため wasm ビルドでは無効（正規表現縮退）。

use crate::fmt::OutlineItem;

#[cfg(any(feature = "lang-go", feature = "lang-c"))]
fn parse_with(language: &tree_sitter::Language, text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    let tree = parser.parse(text, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    Some(tree)
}

#[cfg(any(feature = "lang-go", feature = "lang-c"))]
fn node_text<'a>(node: tree_sitter::Node, text: &'a str) -> &'a str {
    &text[node.byte_range()]
}

#[cfg(feature = "lang-go")]
pub fn outline_go(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_go::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    let root = tree.root_node();
    let mut out = Vec::new();
    let is_public = |name: &str| {
        name.chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    };
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "function_declaration" | "method_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let n = node_text(name, text).to_string();
                    let p = is_public(&n);
                    out.push(("func".to_string(), n, p));
                }
            }
            "type_declaration" => {
                let mut c2 = node.walk();
                for spec in node.children(&mut c2) {
                    if spec.kind() == "type_spec" {
                        let name = spec.child_by_field_name("name");
                        let ty = spec.child_by_field_name("type");
                        if let (Some(name), Some(ty)) = (name, ty) {
                            let kind = match ty.kind() {
                                "struct_type" => Some("struct"),
                                "interface_type" => Some("interface"),
                                _ => None,
                            };
                            if let Some(kind) = kind {
                                let n = node_text(name, text).to_string();
                                let p = is_public(&n);
                                out.push((kind.to_string(), n, p));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Some(out)
}

#[cfg(feature = "lang-go")]
pub fn imports_go(text: &str) -> Option<Vec<String>> {
    let lang = tree_sitter_go::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    let root = tree.root_node();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() != "import_declaration" {
            continue;
        }
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "import_spec" {
                if let Some(path) = n.child_by_field_name("path") {
                    let raw = node_text(path, text);
                    out.push(raw.trim_matches('"').to_string());
                }
            } else {
                let mut c2 = n.walk();
                let children: Vec<_> = n.children(&mut c2).collect();
                for ch in children.into_iter().rev() {
                    stack.push(ch);
                }
            }
        }
    }
    Some(out)
}

#[cfg(feature = "lang-c")]
pub fn outline_c(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_c::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    let root = tree.root_node();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "function_definition" => {
                if let Some(decl) = node.child_by_field_name("declarator") {
                    if let Some(name) = find_identifier(decl, text) {
                        out.push(("func".to_string(), name, true));
                    }
                }
            }
            "struct_specifier" => {
                if let Some(name) = node.child_by_field_name("name") {
                    out.push(("struct".to_string(), node_text(name, text).to_string(), true));
                }
            }
            _ => {}
        }
    }
    Some(out)
}

#[cfg(feature = "lang-c")]
fn find_identifier(node: tree_sitter::Node, text: &str) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(node_text(node, text).to_string());
    }
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for ch in children {
        if let Some(n) = find_identifier(ch, text) {
            return Some(n);
        }
    }
    None
}
