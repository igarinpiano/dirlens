//! プロジェクト全体プレスキャン（-V/-N/-F/-M）。dirlens.py の build_project_index /
//! detect_cycles / 各 import 抽出・解決関数の等価移植。
//!
//! Python の import 抽出は現段階では行ベースの簡易パーサ（後続ステージで
//! ruff_python_parser による AST 抽出に置き換え、失敗時にこちらへ縮退する）。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use indexmap::IndexMap;
use regex::Regex;

use crate::cfg::Cfg;
use crate::fmt::splitext;
use crate::gitignore::{extend_pats, is_ignored, relpath, relpath_slash};
use crate::provider::FsProvider;
use crate::pyc::{decode_utf8_ignore, py_strip};
use crate::session::Session;

pub const SOURCE_EXTS_FOR_TESTS: &[&str] = &[".py", ".js", ".jsx", ".ts", ".tsx", ".go"];

const ENTRY_NAMES_LOWER: &[&str] = &[
    "main.py", "__main__.py", "app.py", "server.py", "manage.py", "wsgi.py", "asgi.py",
    "index.js", "index.ts", "index.mjs", "index.cjs",
    "main.js", "main.ts", "server.js", "server.ts", "app.js", "app.ts",
    "main.go", "main.rs",
    "makefile", "dockerfile", "docker-compose.yml", "docker-compose.yaml",
];

const CONFIG_NAMES_LOWER: &[&str] = &[
    ".env", ".env.local", ".env.example", ".env.development", ".env.production",
    "tsconfig.json", "jsconfig.json", "babel.config.js", ".babelrc",
    "webpack.config.js", "vite.config.js", "vite.config.ts", "rollup.config.js",
    "eslint.config.js", ".eslintrc", ".eslintrc.json", ".eslintrc.js", ".prettierrc",
    "pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "pipfile",
    "cargo.toml", "go.mod", "go.sum",
    "dockerfile", "docker-compose.yml", "docker-compose.yaml",
    ".gitignore", ".gitattributes",
    "tox.ini", "pytest.ini", "jest.config.js", "jest.config.ts",
    "next.config.js", "next.config.ts", "nuxt.config.js", "svelte.config.js",
    "tailwind.config.js", "tailwind.config.ts", "postcss.config.js",
    ".npmrc", ".nvmrc", ".python-version", ".ruby-version",
    "makefile", "cmakelists.txt",
];

pub fn is_test_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    let (stem, _ext) = splitext(&lower);
    stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with(".test")
        || stem.ends_with(".spec")
}

/// posixpath.normpath 相当（"/" 区切りの相対/絶対パス用）。
pub fn normpath(p: &str) -> String {
    let leading = p.starts_with('/');
    let mut comps: Vec<&str> = Vec::new();
    for c in p.split('/') {
        if c.is_empty() || c == "." {
            continue;
        }
        if c == ".." {
            if !comps.is_empty() && *comps.last().unwrap() != ".." {
                comps.pop();
            } else if !leading {
                comps.push("..");
            }
        } else {
            comps.push(c);
        }
    }
    let joined = comps.join("/");
    if leading {
        format!("/{}", joined)
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

fn pjoin(base: &str, target: &str) -> String {
    if target.starts_with('/') || base.is_empty() {
        target.to_string()
    } else {
        format!("{}/{}", base, target)
    }
}

/// "/" 区切りの dirname（os.path.dirname 相当）。
pub fn dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

/// 'pkg/sub/mod.py' -> 'pkg.sub.mod'、'pkg/sub/__init__.py' -> 'pkg.sub'
fn py_module_key(relpath: &str) -> String {
    let mut parts: Vec<&str> = relpath.split('/').collect();
    let last = *parts.last().unwrap();
    if last == "__init__.py" {
        parts.pop();
        parts.join(".")
    } else if let Some(stripped) = last.strip_suffix(".py") {
        let n = parts.len();
        let mut owned: Vec<String> = parts[..n - 1].iter().map(|s| s.to_string()).collect();
        owned.push(stripped.to_string());
        owned.join(".")
    } else {
        parts.join(".")
    }
}

/// JS/TS の相対 import をプロジェクト内ファイルに解決する。
fn resolve_relative_path(
    base_dir: &str,
    target: &str,
    project_files: &BTreeSet<String>,
) -> Option<String> {
    let candidate = normpath(&pjoin(base_dir, target)).replace('\\', "/");
    for suffix in [
        "", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
        "/index.js", "/index.ts", "/index.jsx", "/index.tsx",
    ] {
        let cand = format!("{}{}", candidate, suffix);
        if project_files.contains(&cand) {
            return Some(cand);
        }
    }
    None
}

/// 'crate::foo::bar::Baz' をプロジェクト内ファイルに解決する（src/foo/bar.rs 等）。
fn resolve_rust_crate_path(use_path: &str, project_files: &BTreeSet<String>) -> Option<String> {
    let body = use_path.strip_prefix("crate::")?;
    let segments: Vec<&str> = body
        .split("::")
        .filter(|s| !s.is_empty() && *s != "self" && *s != "*")
        .collect();
    let n = segments.len() as i64;
    for cut in [n, n - 1] {
        if cut <= 0 {
            continue;
        }
        let path_part = segments[..cut as usize].join("/");
        for cand in [
            format!("src/{}.rs", path_part),
            format!("src/{}/mod.rs", path_part),
        ] {
            if project_files.contains(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

// ─── import 抽出 ─────────────────────────────────────────────

/// Python import 抽出（行ベース簡易版・ast 相当の出力形式）。
/// 戻り値: (module, level, names)。level>0 は相対 import。
pub fn extract_imports_py(text: &str) -> Vec<(String, u32, Option<Vec<String>>)> {
    let mut out = Vec::new();
    // 括弧が閉じるまで論理行を結合する（from x import (a,\n b) 対応）
    let mut logical: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut depth: i32 = 0;
    for line in text.split('\n') {
        let code = line.split('#').next().unwrap_or("");
        if depth > 0 {
            buf.push(' ');
            buf.push_str(code);
        } else {
            buf = code.to_string();
        }
        depth += code.matches('(').count() as i32;
        depth -= code.matches(')').count() as i32;
        if depth <= 0 {
            depth = 0;
            logical.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        logical.push(buf);
    }
    for line in &logical {
        let t = py_strip(line);
        if let Some(rest) = t.strip_prefix("import ") {
            for item in rest.split(',') {
                let item = py_strip(item);
                let module = item.split_whitespace().next().unwrap_or("");
                if !module.is_empty() {
                    out.push((module.to_string(), 0, None));
                }
            }
        } else if let Some(rest) = t.strip_prefix("from ") {
            if let Some((module_part, names_part)) = rest.split_once(" import ") {
                let module_part = py_strip(module_part);
                let level = module_part.chars().take_while(|&c| c == '.').count() as u32;
                let module: String = module_part.chars().skip(level as usize).collect();
                let names: Vec<String> = names_part
                    .trim_matches(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .split(',')
                    .map(|n| {
                        let n = py_strip(n);
                        n.split_whitespace().next().unwrap_or("").to_string()
                    })
                    .filter(|n| !n.is_empty())
                    .collect();
                out.push((module, level, Some(names)));
            }
        }
    }
    out
}

fn js_patterns() -> &'static [Regex] {
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    RES.get_or_init(|| {
        vec![
            Regex::new(r#"import\s+(?:[\w*\s{},]+\s+from\s+)?['"]([^'"]+)['"]"#).unwrap(),
            Regex::new(r#"export\s+(?:[\w*\s{},]+\s+from\s+)?['"]([^'"]+)['"]"#).unwrap(),
            Regex::new(r#"require\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
            Regex::new(r#"import\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
        ]
    })
}

pub fn extract_imports_js(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for pat in js_patterns() {
        for m in pat.captures_iter(text) {
            found.push(m.get(1).unwrap().as_str().to_string());
        }
    }
    found
}

pub fn extract_imports_go(text: &str) -> Vec<String> {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static LINE: OnceLock<Regex> = OnceLock::new();
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let block = BLOCK.get_or_init(|| Regex::new(r"(?s)import\s*\(([^)]*)\)").unwrap());
    let line = LINE.get_or_init(|| Regex::new(r#"import\s+"([^"]+)""#).unwrap());
    let item = ITEM.get_or_init(|| Regex::new(r#""([^"]+)""#).unwrap());
    let mut found = Vec::new();
    if let Some(b) = block.captures(text) {
        for m in item.captures_iter(b.get(1).unwrap().as_str()) {
            found.push(m.get(1).unwrap().as_str().to_string());
        }
    }
    for m in line.captures_iter(text) {
        found.push(m.get(1).unwrap().as_str().to_string());
    }
    found
}

/// 戻り値: (use 文のパスリスト, mod 宣言のモジュール名リスト)
pub fn extract_imports_rs(text: &str) -> (Vec<String>, Vec<String>) {
    static USE: OnceLock<Regex> = OnceLock::new();
    static MOD: OnceLock<Regex> = OnceLock::new();
    let use_re = USE.get_or_init(|| Regex::new(r"(?m)^\s*(?:pub\s+)?use\s+([\w:]+)").unwrap());
    let mod_re = MOD.get_or_init(|| Regex::new(r"(?m)^\s*(?:pub\s+)?mod\s+(\w+)\s*;").unwrap());
    let uses = use_re
        .captures_iter(text)
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .collect();
    let mods = mod_re
        .captures_iter(text)
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .collect();
    (uses, mods)
}

// ─── 循環依存検出 ────────────────────────────────────────────

pub fn detect_cycles(imports_map: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;

    fn dfs(
        node: &str,
        imports_map: &BTreeMap<String, Vec<String>>,
        color: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
        seen_keys: &mut HashSet<BTreeSet<String>>,
    ) {
        color.insert(node.to_string(), GRAY);
        stack.push(node.to_string());
        if let Some(nexts) = imports_map.get(node) {
            for nxt in nexts {
                let c = color.get(nxt).copied().unwrap_or(WHITE);
                if c == WHITE {
                    dfs(nxt, imports_map, color, stack, cycles, seen_keys);
                } else if c == GRAY {
                    let idx = stack.iter().position(|s| s == nxt).unwrap();
                    let mut cycle: Vec<String> = stack[idx..].to_vec();
                    cycle.push(nxt.clone());
                    let key: BTreeSet<String> =
                        cycle[..cycle.len() - 1].iter().cloned().collect();
                    if seen_keys.insert(key) {
                        cycles.push(cycle);
                    }
                }
            }
        }
        stack.pop();
        color.insert(node.to_string(), BLACK);
    }

    let mut color = HashMap::new();
    let mut stack = Vec::new();
    let mut cycles = Vec::new();
    let mut seen_keys = HashSet::new();
    for node in imports_map.keys() {
        if color.get(node).copied().unwrap_or(WHITE) == WHITE {
            dfs(node, imports_map, &mut color, &mut stack, &mut cycles, &mut seen_keys);
        }
    }
    cycles
}

// ─── プロジェクトインデックス本体 ─────────────────────────────

#[derive(Debug, Default)]
pub struct ProjectIndex {
    pub untested: HashSet<String>,
    pub entry_set: BTreeSet<String>,
    pub config_set: HashSet<String>,
    pub imports_map: BTreeMap<String, Vec<String>>,
    pub imported_by_map: IndexMap<String, Vec<String>>,
    pub external_map: HashMap<String, Vec<String>>,
    pub cycles: Vec<Vec<String>>,
}

struct WalkState {
    all_names: HashSet<String>,
    all_relpaths: BTreeSet<String>,
    source_files: Vec<(String, String, String)>, // (relpath, stem(原文), ext(小文字))
    entry_set: BTreeSet<String>,
    pkg_entry_candidates: BTreeSet<String>,
    config_set: HashSet<String>,
    py_module_map: HashMap<String, String>,
    go_module_name: Option<String>,
}

pub fn build_project_index<F: FsProvider>(
    sess: &Session<F>,
    root: &Path,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> ProjectIndex {
    let mut st = WalkState {
        all_names: HashSet::new(),
        all_relpaths: BTreeSet::new(),
        source_files: Vec::new(),
        entry_set: BTreeSet::new(),
        pkg_entry_candidates: BTreeSet::new(),
        config_set: HashSet::new(),
        py_module_map: HashMap::new(),
        go_module_name: None,
    };

    walk(sess, root, cfg, active_pats.clone(), &mut st);

    // package.json の main/bin は実在するファイルのみエントリーポイントとして扱う
    for cand in &st.pkg_entry_candidates {
        if st.all_relpaths.contains(cand) {
            st.entry_set.insert(cand.clone());
        }
    }

    let mut untested = HashSet::new();
    for (relpath, stem, ext) in &st.source_files {
        let candidates = [
            format!("test_{}{}", stem, ext),
            format!("{}_test{}", stem, ext),
            format!("{}.test{}", stem, ext),
            format!("{}.spec{}", stem, ext),
        ];
        if !candidates.iter().any(|c| st.all_names.contains(c)) {
            untested.insert(relpath.clone());
        }
    }

    let mut imports_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut imported_by_acc: IndexMap<String, BTreeSet<String>> = IndexMap::new();
    let mut external_map: HashMap<String, Vec<String>> = HashMap::new();

    if cfg.show_imports {
        // Go のローカル import 解決用の前計算
        let mut go_files_by_dir: HashMap<String, Vec<String>> = HashMap::new();
        for r in &st.all_relpaths {
            if r.ends_with(".go") {
                go_files_by_dir
                    .entry(dirname(r).to_string())
                    .or_default()
                    .push(r.clone());
            }
        }

        for relpath in &st.all_relpaths {
            let (_, ext_raw) = splitext(relpath.rsplit('/').next().unwrap_or(relpath));
            let ext = ext_raw.to_lowercase();
            let base_dir = dirname(relpath).to_string();
            let mut local_targets: BTreeSet<String> = BTreeSet::new();
            let mut external_raw: Vec<String> = Vec::new();

            let read_text = || {
                let full = root.join(relpath.replace('/', std::path::MAIN_SEPARATOR_STR));
                sess.fs
                    .read_prefix(&full, usize::MAX)
                    .map(|d| decode_utf8_ignore(&d))
                    .unwrap_or_default()
            };

            match ext.as_str() {
                ".py" => {
                    for (module, level, names) in extract_imports_py(&read_text()) {
                        if level > 0 {
                            let mut pkg_parts: Vec<String> = if base_dir.is_empty() {
                                Vec::new()
                            } else {
                                base_dir.split('/').map(|s| s.to_string()).collect()
                            };
                            let up = (level - 1) as usize;
                            if up > 0 {
                                if up <= pkg_parts.len() {
                                    pkg_parts.truncate(pkg_parts.len() - up);
                                } else {
                                    pkg_parts.clear();
                                }
                            }
                            let mut key_parts = pkg_parts;
                            if !module.is_empty() {
                                key_parts.push(module.clone());
                            }
                            let target_key = key_parts.join(".");
                            let mut resolved: Option<String> = None;
                            if let Some(names) = &names {
                                for nm in names {
                                    let cand_key = if target_key.is_empty() {
                                        nm.clone()
                                    } else {
                                        format!("{}.{}", target_key, nm)
                                    };
                                    if let Some(r) = st.py_module_map.get(&cand_key) {
                                        resolved = Some(r.clone());
                                        break;
                                    }
                                }
                            }
                            if resolved.is_none() {
                                resolved = st.py_module_map.get(&target_key).cloned();
                            }
                            match resolved {
                                Some(r) if r != *relpath => {
                                    local_targets.insert(r);
                                }
                                _ => {
                                    external_raw
                                        .push(format!("{}{}", ".".repeat(level as usize), module));
                                }
                            }
                        } else {
                            match st.py_module_map.get(&module) {
                                Some(r) if r != relpath => {
                                    local_targets.insert(r.clone());
                                }
                                _ => external_raw.push(module),
                            }
                        }
                    }
                }
                ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" => {
                    for spec in extract_imports_js(&read_text()) {
                        if spec.starts_with('.') || spec.starts_with('/') {
                            match resolve_relative_path(&base_dir, &spec, &st.all_relpaths) {
                                Some(r) if r != *relpath => {
                                    local_targets.insert(r);
                                }
                                _ => external_raw.push(spec),
                            }
                        } else {
                            external_raw.push(spec);
                        }
                    }
                }
                ".go" => {
                    for spec in extract_imports_go(&read_text()) {
                        let matched = st
                            .go_module_name
                            .as_ref()
                            .filter(|m| spec.starts_with(*m))
                            .cloned();
                        if let Some(mod_name) = matched {
                            let sub = spec[mod_name.len()..].trim_start_matches('/').to_string();
                            let mut candidates: Vec<String> = Vec::new();
                            for (d, files) in &go_files_by_dir {
                                if *d == sub
                                    || (!sub.is_empty() && d.starts_with(&format!("{}/", sub)))
                                {
                                    candidates.extend(files.iter().cloned());
                                }
                            }
                            if !candidates.is_empty() {
                                for cand in candidates {
                                    if cand != *relpath {
                                        local_targets.insert(cand);
                                    }
                                }
                            } else {
                                external_raw.push(spec);
                            }
                        } else {
                            external_raw.push(spec);
                        }
                    }
                }
                ".rs" => {
                    let text = read_text();
                    let (uses, mods) = extract_imports_rs(&text);
                    for m in mods {
                        let cands = if base_dir.is_empty() {
                            [format!("{}.rs", m), format!("{}/mod.rs", m)]
                        } else {
                            [
                                format!("{}/{}.rs", base_dir, m),
                                format!("{}/{}/mod.rs", base_dir, m),
                            ]
                        };
                        for cand in cands {
                            if st.all_relpaths.contains(&cand) {
                                local_targets.insert(cand);
                            }
                        }
                    }
                    for u in uses {
                        match resolve_rust_crate_path(&u, &st.all_relpaths) {
                            Some(r) if r != *relpath => {
                                local_targets.insert(r);
                            }
                            _ => external_raw.push(u),
                        }
                    }
                }
                _ => {}
            }

            if !local_targets.is_empty() {
                let sorted: Vec<String> = local_targets.iter().cloned().collect();
                imports_map.insert(relpath.clone(), sorted);
                for t in &local_targets {
                    imported_by_acc
                        .entry(t.clone())
                        .or_default()
                        .insert(relpath.clone());
                }
            }
            if !external_raw.is_empty() {
                let mut seen: Vec<String> = Vec::new();
                for x in external_raw {
                    if !x.is_empty() && !seen.contains(&x) {
                        seen.push(x);
                    }
                }
                seen.truncate(10);
                external_map.insert(relpath.clone(), seen);
            }
        }
    }

    let imported_by_map: IndexMap<String, Vec<String>> = imported_by_acc
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    let cycles = if cfg.show_imports {
        detect_cycles(&imports_map)
    } else {
        Vec::new()
    };

    ProjectIndex {
        untested,
        entry_set: st.entry_set,
        config_set: st.config_set,
        imports_map,
        imported_by_map,
        external_map,
        cycles,
    }
}

fn walk<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: Arc<Vec<String>>,
    st: &mut WalkState,
) {
    let pats = if cfg.use_gitignore {
        extend_pats(sess, &active_pats, path, cfg)
    } else {
        active_pats.clone()
    };
    let entries = match sess.fs.scan_dir(path) {
        Ok(e) => e,
        Err(()) => return,
    };
    let mut entries: Vec<_> = entries
        .into_iter()
        .filter(|e| cfg.show_all || !e.name.starts_with('.'))
        .collect();
    if let Some(git_set) = &sess.git_ignored {
        if cfg.use_gitignore {
            entries.retain(|e| !git_set.contains(&relpath_slash(&e.path, &cfg.root)));
        }
    } else if !pats.is_empty() {
        entries.retain(|e| {
            !is_ignored(
                &e.name,
                &relpath(&e.path, &cfg.root),
                e.is_dir_nofollow,
                &pats,
            )
        });
    }
    for e in entries {
        if e.is_dir_nofollow {
            walk(sess, &e.path, cfg, pats.clone(), st);
            continue;
        }
        let rel = relpath_slash(&e.path, &cfg.root);
        let (stem, ext_raw) = splitext(&e.name);
        let ext = ext_raw.to_lowercase();
        let name_lower = e.name.to_lowercase();
        st.all_names.insert(name_lower.clone());
        st.all_relpaths.insert(rel.clone());
        if SOURCE_EXTS_FOR_TESTS.contains(&ext.as_str()) && !is_test_file(&e.name) {
            st.source_files.push((rel.clone(), stem.to_string(), ext.clone()));
        }
        if ENTRY_NAMES_LOWER.contains(&name_lower.as_str()) {
            st.entry_set.insert(rel.clone());
        }
        if CONFIG_NAMES_LOWER.contains(&name_lower.as_str()) {
            st.config_set.insert(rel.clone());
        }
        if ext == ".py" {
            st.py_module_map.insert(py_module_key(&rel), rel.clone());
        }
        if e.name == "go.mod" {
            if let Some(data) = sess.fs.read_prefix(&e.path, usize::MAX) {
                let text = decode_utf8_ignore(&data);
                for line in text.split('\n') {
                    let line = py_strip(line);
                    if line.starts_with("module ") {
                        st.go_module_name = Some(py_strip(&line["module".len()..]).to_string());
                        break;
                    }
                }
            }
        }
        if e.name == "package.json" {
            if let Some(data) = sess.fs.read_prefix(&e.path, usize::MAX) {
                if let Ok(text) = std::str::from_utf8(&data) {
                    if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(text) {
                        let base_dir = dirname(&rel).to_string();
                        let mut add = |v: &str| {
                            st.pkg_entry_candidates
                                .insert(normpath(&pjoin(&base_dir, v)).replace('\\', "/"));
                        };
                        if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
                            add(main);
                        }
                        match pkg.get("bin") {
                            Some(serde_json::Value::String(s)) => add(s),
                            Some(serde_json::Value::Object(map)) => {
                                for v in map.values() {
                                    if let Some(s) = v.as_str() {
                                        add(s);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
