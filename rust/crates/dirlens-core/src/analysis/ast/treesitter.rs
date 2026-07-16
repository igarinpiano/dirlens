//! Go / C ほか tree-sitter 系言語の AST 解析（feature gate）。
//! C 依存があるため wasm ビルドでは無効（正規表現縮退）。

use crate::fmt::OutlineItem;

#[cfg(any(
    feature = "lang-go", feature = "lang-c", feature = "lang-java",
    feature = "lang-ruby", feature = "lang-php", feature = "lang-csharp",
    feature = "lang-kotlin", feature = "lang-swift"
))]
fn parse_with(language: &tree_sitter::Language, text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    let tree = parser.parse(text, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    Some(tree)
}

#[cfg(any(
    feature = "lang-go", feature = "lang-c", feature = "lang-java",
    feature = "lang-ruby", feature = "lang-php", feature = "lang-csharp",
    feature = "lang-kotlin", feature = "lang-swift"
))]
fn node_text<'a>(node: tree_sitter::Node, text: &'a str) -> &'a str {
    &text[node.byte_range()]
}

#[cfg(any(
    feature = "lang-go", feature = "lang-c", feature = "lang-java",
    feature = "lang-ruby", feature = "lang-php", feature = "lang-csharp",
    feature = "lang-kotlin", feature = "lang-swift"
))]
fn node_span(node: tree_sitter::Node) -> Option<(u32, u32)> {
    Some((
        node.start_position().row as u32 + 1,
        node.end_position().row as u32 + 1,
    ))
}

#[cfg(any(
    feature = "lang-go", feature = "lang-c", feature = "lang-java",
    feature = "lang-ruby", feature = "lang-php", feature = "lang-csharp",
    feature = "lang-kotlin", feature = "lang-swift"
))]
fn ts_item(kind: &str, name: String, public: bool, node: tree_sitter::Node) -> OutlineItem {
    let mut it = OutlineItem::new(kind, name, public);
    it.span = node_span(node);
    it
}

/// 汎用アウトラインウォーカー: ツリーを再帰的に歩き、
/// 対象 kind のノードから name フィールド（無ければ最初の識別子）を拾う。
/// 公開判定は言語ごとのクロージャに委譲する。
#[cfg(any(
    feature = "lang-java", feature = "lang-ruby", feature = "lang-php",
    feature = "lang-csharp", feature = "lang-kotlin", feature = "lang-swift"
))]
/// is_public に渡す情報: (宣言ノード, 宣言ヘッダ（ノード先頭〜名前の直前）, 名前)。
/// ヘッダで判定することで、ネストした子の修飾子を誤って拾わない。
fn walk_outline(
    text: &str,
    root: tree_sitter::Node,
    kinds: &[(&str, &str)], // (node kind, 表示ラベル)
    name_kinds: &[&str],    // name フィールドが無いときに探す識別子ノードの kind
    is_public: &dyn Fn(tree_sitter::Node, &str, &str) -> bool,
) -> Vec<OutlineItem> {
    let name_of = |node: tree_sitter::Node| -> Option<String> {
        let nn = node.child_by_field_name("name").or_else(|| {
            let mut c = node.walk();
            node.children(&mut c).find(|ch| name_kinds.contains(&ch.kind()))
        })?;
        let name = node_text(nn, text).to_string();
        (!name.is_empty()).then_some(name)
    };
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if let Some((_, label)) = kinds.iter().find(|(k, _)| *k == node.kind()) {
            let name_node = node.child_by_field_name("name").or_else(|| {
                let mut c = node.walk();
                node.children(&mut c).find(|ch| name_kinds.contains(&ch.kind()))
            });
            if let Some(nn) = name_node {
                let name = node_text(nn, text).to_string();
                if !name.is_empty() {
                    let header = &text[node.start_byte()..nn.start_byte().max(node.start_byte())];
                    let public = is_public(node, header, &name);
                    let mut it = ts_item(label, name, public, node);
                    // 直近の外側アウトライン対象ノード（クラス等）の名前を parent に
                    // 入れる（同名メソッドの区別用）
                    let mut anc = node.parent();
                    while let Some(a) = anc {
                        if kinds.iter().any(|(k, _)| *k == a.kind()) {
                            it.parent = name_of(a);
                            break;
                        }
                        anc = a.parent();
                    }
                    out.push(it);
                }
            }
        }
        let mut c = node.walk();
        let children: Vec<_> = node.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    out
}

/// modifiers 系の子ノードのテキストに `word` が含まれるか（Java / C# の public 判定）。
#[cfg(any(feature = "lang-java", feature = "lang-csharp"))]
fn has_modifier(node: tree_sitter::Node, text: &str, word: &str) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" || ch.kind() == "modifier" {
            if node_text(ch, text).split_whitespace().any(|w| w == word) {
                return true;
            }
        }
    }
    false
}

#[cfg(feature = "lang-java")]
pub fn outline_java(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_java::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class_declaration", "class"),
            ("interface_declaration", "interface"),
            ("enum_declaration", "enum"),
            ("record_declaration", "record"),
            ("method_declaration", "method"),
            ("constructor_declaration", "method"),
        ],
        &["identifier"],
        &|node, _, _| has_modifier(node, text, "public"),
    ))
}

#[cfg(feature = "lang-ruby")]
pub fn outline_ruby(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_ruby::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class", "class"),
            ("module", "module"),
            ("method", "def"),
            ("singleton_method", "def"),
        ],
        &["constant", "identifier"],
        // Ruby はデフォルト public。_ 始まりは慣習的に内部扱い
        &|_, _, name| !name.starts_with('_'),
    ))
}

#[cfg(feature = "lang-php")]
pub fn outline_php(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_php::LANGUAGE_PHP.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class_declaration", "class"),
            ("interface_declaration", "interface"),
            ("trait_declaration", "trait"),
            ("enum_declaration", "enum"),
            ("function_definition", "func"),
            ("method_declaration", "method"),
        ],
        &["name"],
        // PHP は可視性修飾が無ければ public（ヘッダ = 宣言先頭〜名前の直前で判定）
        &|_, header, _| !header.contains("private") && !header.contains("protected"),
    ))
}

#[cfg(feature = "lang-csharp")]
pub fn outline_csharp(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_c_sharp::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class_declaration", "class"),
            ("interface_declaration", "interface"),
            ("struct_declaration", "struct"),
            ("enum_declaration", "enum"),
            ("record_declaration", "record"),
            ("method_declaration", "method"),
        ],
        &["identifier"],
        &|node, _, _| has_modifier(node, text, "public"),
    ))
}

#[cfg(feature = "lang-kotlin")]
pub fn outline_kotlin(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_kotlin_ng::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class_declaration", "class"),
            ("object_declaration", "object"),
            ("function_declaration", "fun"),
        ],
        &["type_identifier", "simple_identifier", "identifier"],
        // Kotlin はデフォルト public。ヘッダの修飾子で判定
        &|_, header, _| !header.contains("private") && !header.contains("internal"),
    ))
}

#[cfg(feature = "lang-swift")]
pub fn outline_swift(text: &str) -> Option<Vec<OutlineItem>> {
    let lang = tree_sitter_swift::LANGUAGE.into();
    let tree = parse_with(&lang, text)?;
    Some(walk_outline(
        text,
        tree.root_node(),
        &[
            ("class_declaration", "type"), // class / struct / enum / extension を包含
            ("protocol_declaration", "protocol"),
            ("function_declaration", "func"),
        ],
        &["type_identifier", "simple_identifier", "identifier"],
        // Swift はデフォルト internal（モジュール内公開）。private/fileprivate のみ非公開扱い
        &|_, header, _| !header.contains("private"),
    ))
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
                    out.push(ts_item("func", n, p, node));
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
                                out.push(ts_item(kind, n, p, node));
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
                        out.push(ts_item("func", name, true, node));
                    }
                }
            }
            "struct_specifier" => {
                if let Some(name) = node.child_by_field_name("name") {
                    out.push(ts_item("struct", node_text(name, text).to_string(), true, node));
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
