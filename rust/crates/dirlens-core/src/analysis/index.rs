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

use crate::analysis::text_metrics::TEXT_READ_LIMIT;
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

/// テスト欠落検知（-V）の判定対象となるファイルか。
/// 対象は命名規則で追える言語（SOURCE_EXTS_FOR_TESTS）と、enhanced 時の
/// .rs（インラインテスト＋テストからの import 追跡）。対象外のファイルに
/// has_test の真偽値を返すと「テスト有り」に見えてしまうため、JSON では
/// これが false のとき null を出す。
pub fn test_detection_applies(name: &str, enhanced: bool) -> bool {
    let lower = name.to_lowercase();
    let (_, ext) = splitext(&lower);
    SOURCE_EXTS_FOR_TESTS.contains(&ext) || (enhanced && ext == ".rs")
}

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

/// require() / 動的 import() のパターン（AST 第1段の補完にも使う）。
pub fn js_call_patterns() -> &'static [Regex] {
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    RES.get_or_init(|| {
        vec![
            Regex::new(r#"require\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
            Regex::new(r#"import\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap(),
        ]
    })
}

/// tsconfig.json 等の JSONC（コメント・末尾カンマ許容）を素の JSON に変換する。
fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    // 末尾カンマの除去
    static TRAILING: OnceLock<Regex> = OnceLock::new();
    let re = TRAILING.get_or_init(|| Regex::new(r",\s*([}\]])").unwrap());
    re.replace_all(&out, "$1").into_owned()
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

    // 明示スタックによる DFS（3色法）。再帰版と同じ訪問順・記録順を保ちつつ、
    // 数万ファイル規模の import 連鎖でもコールスタックを溢れさせない。
    let mut color: HashMap<&str, u8> = HashMap::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut seen_keys: HashSet<BTreeSet<String>> = HashSet::new();

    for start in imports_map.keys() {
        if color.get(start.as_str()).copied().unwrap_or(WHITE) != WHITE {
            continue;
        }
        color.insert(start, GRAY);
        // path: 現在の探索経路（再帰版の stack 相当）
        // frames: (ノード, 次に見る隣接ノードの添字)
        let mut path: Vec<&str> = vec![start];
        let mut frames: Vec<(&str, usize)> = vec![(start, 0)];
        while let Some((node, i)) = frames.last_mut() {
            let node: &str = node;
            let nexts = imports_map.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if *i >= nexts.len() {
                path.pop();
                color.insert(node, BLACK);
                frames.pop();
                continue;
            }
            let nxt = nexts[*i].as_str();
            *i += 1;
            match color.get(nxt).copied().unwrap_or(WHITE) {
                WHITE => {
                    color.insert(nxt, GRAY);
                    path.push(nxt);
                    frames.push((nxt, 0));
                }
                GRAY => {
                    let idx = path.iter().position(|s| *s == nxt).unwrap();
                    let mut cycle: Vec<String> =
                        path[idx..].iter().map(|s| s.to_string()).collect();
                    cycle.push(nxt.to_string());
                    let key: BTreeSet<String> =
                        cycle[..cycle.len() - 1].iter().cloned().collect();
                    if seen_keys.insert(key) {
                        cycles.push(cycle);
                    }
                }
                _ => {}
            }
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
    /// Cargo.toml のあるディレクトリ（"" = スキャンルート）。
    /// Rust の crate:: 解決をクレート単位に分けるために使う
    cargo_dirs: BTreeSet<String>,
    source_files: Vec<(String, String, String)>, // (relpath, stem(原文), ext(小文字))
    entry_set: BTreeSet<String>,
    pkg_entry_candidates: BTreeSet<String>,
    config_set: HashSet<String>,
    py_module_map: HashMap<String, String>,
    go_module_name: Option<String>,
    // import 解決改善（マニフェスト読込・ルート直下のもののみ）
    ts_base_url: String,
    ts_paths: Vec<(String, Vec<String>)>,
    pkg_imports: Vec<(String, String)>,
}

/// ファイルの属するクレートルート（Cargo.toml のあるディレクトリの最長一致）。
/// Cargo.toml がどこにも無い、またはどのクレートにも属さない場合はスキャン
/// ルート（""）を返す（v1.2.6 以前の「ルートの src/ 前提」と同じ挙動）。
/// これによりモノレポ/ワークスペースのサブクレートでも crate:: 解決が働く。
pub fn rs_crate_of(relpath: &str, cargo_dirs: &BTreeSet<String>) -> String {
    let mut best = "";
    for d in cargo_dirs {
        if !d.is_empty()
            && d.len() > best.len()
            && relpath.starts_with(d.as_str())
            && relpath.as_bytes().get(d.len()) == Some(&b'/')
        {
            best = d;
        }
    }
    best.to_string()
}

/// クレートルート基準のモジュールパス（src/lib.rs / src/main.rs = ルート、
/// foo.rs / foo/mod.rs = crate::foo）。src/ 配下でなければ None。
fn rs_mod_key(relpath: &str, crate_root: &str) -> Option<Vec<String>> {
    let local = if crate_root.is_empty() {
        relpath
    } else {
        relpath
            .strip_prefix(crate_root)
            .and_then(|s| s.strip_prefix('/'))?
    };
    let rest = local.strip_prefix("src/")?;
    let stem = rest.strip_suffix(".rs")?;
    let mut parts: Vec<String> = stem.split('/').map(|s| s.to_string()).collect();
    if parts == ["main"] || parts == ["lib"] {
        return Some(Vec::new());
    }
    if parts.last().map(|s| s == "mod").unwrap_or(false) {
        parts.pop();
    }
    Some(parts)
}

/// Rust のモジュールツリー（(クレートルート, module path) → relpath）を構築する。
fn build_rs_module_map(
    all_relpaths: &BTreeSet<String>,
    cargo_dirs: &BTreeSet<String>,
) -> HashMap<(String, Vec<String>), String> {
    let mut map: HashMap<(String, Vec<String>), String> = HashMap::new();
    for r in all_relpaths {
        if !r.ends_with(".rs") {
            continue;
        }
        let crate_root = rs_crate_of(r, cargo_dirs);
        let Some(key) = rs_mod_key(r, &crate_root) else { continue };
        // クレートルートは lib.rs を優先する（BTreeSet 順で lib.rs が先に来る）
        if key.is_empty()
            && map.contains_key(&(crate_root.clone(), key.clone()))
            && r.ends_with("main.rs")
        {
            continue;
        }
        map.insert((crate_root, key), r.clone());
    }
    map
}

/// use パスをモジュールツリーで解決する（crate:: / self:: / super:: 対応）。
/// 解決は同一クレート内に限る（外部クレート・path 依存は対象外）。
fn resolve_rs_module(
    use_path: &str,
    crate_root: &str,
    cur_mod: &[String],
    map: &HashMap<(String, Vec<String>), String>,
    self_relpath: &str,
) -> Option<String> {
    let segs: Vec<&str> = use_path.split("::").filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return None;
    }
    let (mut base, mut idx): (Vec<String>, usize) = match segs[0] {
        "crate" => (Vec::new(), 1),
        "self" => (cur_mod.to_vec(), 1),
        "super" => {
            let mut b = cur_mod.to_vec();
            let mut i = 0;
            while i < segs.len() && segs[i] == "super" {
                if b.pop().is_none() {
                    return None;
                }
                i += 1;
            }
            (b, i)
        }
        _ => return None, // 外部 crate（or 2015 エディションの相対パス）は対象外
    };
    while idx < segs.len() && (segs[idx] == "self" || segs[idx] == "*") {
        idx += 1;
    }
    for s in &segs[idx..] {
        if *s != "*" && *s != "self" {
            base.push(s.to_string());
        }
    }
    let n = base.len() as i64;
    for cut in [n, n - 1] {
        if cut < 0 {
            continue;
        }
        if let Some(r) = map.get(&(crate_root.to_string(), base[..cut as usize].to_vec())) {
            if r != self_relpath {
                return Some(r.clone());
            }
        }
    }
    None
}

/// パターン中の '*' を挟んだ前方/後方一致で spec をマッチし、'*' 部分を返す。
fn star_match(pat: &str, spec: &str) -> Option<String> {
    match pat.find('*') {
        Some(pos) => {
            let (pre, post) = (&pat[..pos], &pat[pos + 1..]);
            if spec.len() >= pre.len() + post.len()
                && spec.starts_with(pre)
                && spec.ends_with(post)
            {
                Some(spec[pre.len()..spec.len() - post.len()].to_string())
            } else {
                None
            }
        }
        None => {
            if pat == spec {
                Some(String::new())
            } else {
                None
            }
        }
    }
}

/// bare import を tsconfig paths / baseUrl / package.json imports で解決する（改善）。
fn resolve_js_manifest(spec: &str, st: &WalkState) -> Option<String> {
    if spec.starts_with('#') {
        for (key, target) in &st.pkg_imports {
            if let Some(mid) = star_match(key, spec) {
                let cand = target.replace('*', &mid);
                if let Some(r) = resolve_relative_path("", &cand, &st.all_relpaths) {
                    return Some(r);
                }
            }
        }
        return None;
    }
    for (pat, targets) in &st.ts_paths {
        if let Some(mid) = star_match(pat, spec) {
            for t in targets {
                let cand = t.replace('*', &mid);
                let full = normpath(&pjoin(&st.ts_base_url, &cand));
                if let Some(r) = resolve_relative_path("", &full, &st.all_relpaths) {
                    return Some(r);
                }
            }
        }
    }
    if !st.ts_base_url.is_empty() {
        let full = normpath(&pjoin(&st.ts_base_url, spec));
        if let Some(r) = resolve_relative_path("", &full, &st.all_relpaths) {
            return Some(r);
        }
    }
    None
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
        cargo_dirs: BTreeSet::new(),
        source_files: Vec::new(),
        entry_set: BTreeSet::new(),
        pkg_entry_candidates: BTreeSet::new(),
        config_set: HashSet::new(),
        py_module_map: HashMap::new(),
        go_module_name: None,
        ts_base_url: String::new(),
        ts_paths: Vec::new(),
        pkg_imports: Vec::new(),
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
    // -V 精度向上（enhanced のみ）: テストからの import を辿るため、
    // -M が無くても import 解決を回す。Rust のインラインテストもここで検出する。
    let compute_imports = cfg.show_imports || (cfg.show_tests && cfg.enhanced_analysis);
    let mut rs_self_tested: HashSet<String> = HashSet::new();
    // Rust の `mod x;` 宣言「のみ」で結ばれたエッジ（use で参照されていないもの）。
    // モジュールツリー構造そのものなので、循環依存のシグナルからは除外する
    let mut rs_decl_edges: HashSet<(String, String)> = HashSet::new();

    if compute_imports {
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

        // Rust モジュールツリー（crate::/self::/super:: の解決改善用）。
        // Cargo.toml を境界にクレート単位で構築する（モノレポ/ワークスペース対応）
        let rs_module_map = if cfg.enhanced_analysis {
            build_rs_module_map(&st.all_relpaths, &st.cargo_dirs)
        } else {
            HashMap::new()
        };

        for relpath in &st.all_relpaths {
            let (_, ext_raw) = splitext(relpath.rsplit('/').next().unwrap_or(relpath));
            let ext = ext_raw.to_lowercase();
            let base_dir = dirname(relpath).to_string();
            let mut local_targets: BTreeSet<String> = BTreeSet::new();
            let mut external_raw: Vec<String> = Vec::new();

            let read_text = || {
                let full = root.join(relpath.replace('/', std::path::MAIN_SEPARATOR_STR));
                // -T の本文読込と同じ上限。import 文はファイル先頭に集中するため
                // 打ち切りの影響は実質なく、巨大ファイルによる OOM を防ぐ
                sess.fs
                    .read_prefix(&full, TEXT_READ_LIMIT)
                    .map(|d| decode_utf8_ignore(&d))
                    .unwrap_or_default()
            };

            match ext.as_str() {
                ".py" => {
                    let text = read_text();
                    let imports = if cfg.enhanced_analysis {
                        crate::analysis::ast::ast_imports_py(&text)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| extract_imports_py(&text));
                    for (module, level, names) in imports {
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
                    let text = read_text();
                    let specs = if cfg.enhanced_analysis {
                        crate::analysis::ast::ast_imports_js(&text, &ext)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| extract_imports_js(&text));
                    for spec in specs {
                        if spec.starts_with('.') || spec.starts_with('/') {
                            match resolve_relative_path(&base_dir, &spec, &st.all_relpaths) {
                                Some(r) if r != *relpath => {
                                    local_targets.insert(r);
                                }
                                _ => external_raw.push(spec),
                            }
                        } else {
                            // 改善: tsconfig paths / baseUrl / package.json imports で
                            // エイリアスをローカルファイルに解決してから external に落とす
                            let resolved = if cfg.enhanced_analysis {
                                resolve_js_manifest(&spec, &st)
                            } else {
                                None
                            };
                            match resolved {
                                Some(r) if r != *relpath => {
                                    local_targets.insert(r);
                                }
                                _ => external_raw.push(spec),
                            }
                        }
                    }
                }
                ".go" => {
                    let text = read_text();
                    let specs = if cfg.enhanced_analysis {
                        crate::analysis::ast::ast_imports_go(&text)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| extract_imports_go(&text));
                    for spec in specs {
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
                // 追加言語（enhanced のみ・正規表現ベースの軽量抽出）:
                //   Java / Kotlin: import a.b.C → a/b/C.java 等のサフィックス一致で解決
                //   PHP: use A\B\C → A/B/C.php、require/include → 相対解決
                //   Ruby: require_relative → 相対解決、require → external
                //   C# / Swift: 名前空間・モジュール単位のため external のみ
                ".java" | ".kt" | ".kts" if cfg.enhanced_analysis => {
                    let text = read_text();
                    static IMPORT_RE: OnceLock<Regex> = OnceLock::new();
                    let re = IMPORT_RE.get_or_init(|| {
                        Regex::new(r"(?m)^\s*import\s+([\w.]+)").unwrap()
                    });
                    for m in re.captures_iter(&text) {
                        let fq = m.get(1).unwrap().as_str();
                        // JVM 系は .java / .kt が混在するため両方を試す
                        let base = fq.replace('.', "/");
                        let hit = [".java", ".kt"].iter().find_map(|se| {
                            let suffix = format!("{}{}", base, se);
                            st.all_relpaths
                                .iter()
                                .find(|r| r.ends_with(&suffix) && *r != relpath)
                        });
                        match hit {
                            Some(r) => {
                                local_targets.insert(r.clone());
                            }
                            None => external_raw.push(fq.to_string()),
                        }
                    }
                }
                ".php" if cfg.enhanced_analysis => {
                    let text = read_text();
                    static USE_RE: OnceLock<Regex> = OnceLock::new();
                    static REQ_RE: OnceLock<Regex> = OnceLock::new();
                    let use_re = USE_RE
                        .get_or_init(|| Regex::new(r"(?m)^\s*use\s+([\w\\]+)").unwrap());
                    let req_re = REQ_RE.get_or_init(|| {
                        Regex::new(r#"(?:require|include)(?:_once)?\s*\(?\s*['"]([^'"]+)['"]"#)
                            .unwrap()
                    });
                    for m in use_re.captures_iter(&text) {
                        let fq = m.get(1).unwrap().as_str();
                        let path_suffix = format!("{}.php", fq.replace('\\', "/"));
                        let hit = st
                            .all_relpaths
                            .iter()
                            .find(|r| r.ends_with(&path_suffix) && *r != relpath);
                        match hit {
                            Some(r) => {
                                local_targets.insert(r.clone());
                            }
                            None => external_raw.push(fq.to_string()),
                        }
                    }
                    for m in req_re.captures_iter(&text) {
                        let spec = m.get(1).unwrap().as_str();
                        let cand = normpath(&pjoin(&base_dir, spec));
                        if st.all_relpaths.contains(&cand) && cand != *relpath {
                            local_targets.insert(cand);
                        } else {
                            external_raw.push(spec.to_string());
                        }
                    }
                }
                ".rb" if cfg.enhanced_analysis => {
                    let text = read_text();
                    static REQ_RE: OnceLock<Regex> = OnceLock::new();
                    let re = REQ_RE.get_or_init(|| {
                        Regex::new(r#"(?m)^\s*require(_relative)?\s+['"]([^'"]+)['"]"#).unwrap()
                    });
                    for m in re.captures_iter(&text) {
                        let relative = m.get(1).is_some();
                        let spec = m.get(2).unwrap().as_str();
                        if relative {
                            let mut cand = normpath(&pjoin(&base_dir, spec));
                            if !cand.ends_with(".rb") {
                                cand.push_str(".rb");
                            }
                            if st.all_relpaths.contains(&cand) && cand != *relpath {
                                local_targets.insert(cand);
                            } else {
                                external_raw.push(spec.to_string());
                            }
                        } else {
                            external_raw.push(spec.to_string());
                        }
                    }
                }
                ".rs" => {
                    let text = read_text();
                    // Rust のインラインテスト検出（-V 用・enhanced のみ）
                    if cfg.show_tests
                        && cfg.enhanced_analysis
                        && (text.contains("#[cfg(test)]") || text.contains("#[test]"))
                    {
                        rs_self_tested.insert(relpath.clone());
                    }
                    let (uses, mods) = if cfg.enhanced_analysis {
                        crate::analysis::ast::ast_imports_rs(&text)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| extract_imports_rs(&text));
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
                                // `mod x;` 宣言によるエッジを記録する（下の use で
                                // 同じ相手を参照していれば「宣言のみ」ではなくなる）
                                rs_decl_edges.insert((relpath.clone(), cand.clone()));
                                local_targets.insert(cand);
                            }
                        }
                    }
                    for u in uses {
                        // 改善: モジュールツリーで crate::/self::/super:: を解決し、
                        // 失敗時は従来の src/ ヒューリスティックへ
                        let mut resolved: Option<String> = None;
                        if cfg.enhanced_analysis {
                            let crate_root = rs_crate_of(relpath, &st.cargo_dirs);
                            if let Some(cur) = rs_mod_key(relpath, &crate_root) {
                                resolved = resolve_rs_module(
                                    &u,
                                    &crate_root,
                                    &cur,
                                    &rs_module_map,
                                    relpath,
                                );
                            }
                        }
                        if resolved.is_none() {
                            resolved = resolve_rust_crate_path(&u, &st.all_relpaths)
                                .filter(|r| r != relpath);
                        }
                        match resolved {
                            Some(r) => {
                                rs_decl_edges.remove(&(relpath.clone(), r.clone()));
                                local_targets.insert(r);
                            }
                            None => external_raw.push(u),
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
        if cfg.enhanced_analysis && !rs_decl_edges.is_empty() {
            // `mod x;` 宣言のみのエッジは Rust のモジュールツリー構造そのもので、
            // lib.rs/mod.rs ⇄ 子モジュールの「往復」が全て循環として報告されて
            // しまう。循環検出だけ宣言のみのエッジを除いたグラフで行う
            // （imports / imported_by / focus のグラフには残す）
            let mut filtered = imports_map.clone();
            for (from, to) in &rs_decl_edges {
                if let Some(v) = filtered.get_mut(from) {
                    v.retain(|t| t != to);
                }
            }
            detect_cycles(&filtered)
        } else {
            detect_cycles(&imports_map)
        }
    } else {
        Vec::new()
    };

    // -V 精度向上（enhanced のみ）: テストファイル（命名規則 or tests/ 配下）から
    // 推移的に import されているソースは「テスト有り」とみなす。
    if cfg.show_tests && cfg.enhanced_analysis {
        let is_testish = |rel: &str| {
            let fname = rel.rsplit('/').next().unwrap_or(rel);
            is_test_file(fname)
                || rel.starts_with("tests/")
                || rel.starts_with("test/")
                || rel.contains("/tests/")
                || rel.contains("/test/")
                || rel.contains("/__tests__/")
        };
        let mut covered: HashSet<&str> = HashSet::new();
        let mut queue: Vec<&str> = Vec::new();
        for from in imports_map.keys() {
            if is_testish(from) {
                queue.push(from);
            }
        }
        while let Some(cur) = queue.pop() {
            if let Some(tos) = imports_map.get(cur) {
                for t in tos {
                    if covered.insert(t) {
                        queue.push(t);
                    }
                }
            }
        }
        untested.retain(|p| !covered.contains(p.as_str()));
        untested.retain(|p| !rs_self_tested.contains(p));
        // Rust: インラインテストが無い .rs をテスト未整備として追加
        // （命名規則ベースの対象外だったため、ここで補完する）
        for rel in &st.all_relpaths {
            if rel.ends_with(".rs")
                && !is_testish(rel)
                && !rs_self_tested.contains(rel)
                && !covered.contains(rel.as_str())
                && rel.rsplit('/').next().map(|f| f != "mod.rs" && f != "lib.rs" && f != "main.rs").unwrap_or(true)
            {
                untested.insert(rel.clone());
            }
        }
    }

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
        if name_lower == "cargo.toml" {
            st.cargo_dirs.insert(dirname(&rel).to_string());
        }
        if ext == ".py" {
            st.py_module_map.insert(py_module_key(&rel), rel.clone());
        }
        if e.name == "go.mod" {
            if let Some(data) = sess.fs.read_prefix(&e.path, TEXT_READ_LIMIT) {
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
        if (e.name == "tsconfig.json" || e.name == "jsconfig.json") && !rel.contains('/') {
            // ルート直下のみ対象。tsconfig を優先（jsconfig は未設定時のみ反映）
            if e.name == "tsconfig.json" || (st.ts_paths.is_empty() && st.ts_base_url.is_empty()) {
                if let Some(data) = sess.fs.read_prefix(&e.path, TEXT_READ_LIMIT) {
                    let text = strip_jsonc(&decode_utf8_ignore(&data));
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(co) = v.get("compilerOptions") {
                            if let Some(b) = co.get("baseUrl").and_then(|b| b.as_str()) {
                                st.ts_base_url = b.trim_start_matches("./").to_string();
                                if st.ts_base_url == "." {
                                    st.ts_base_url = String::new();
                                }
                            }
                            if let Some(paths) = co.get("paths").and_then(|p| p.as_object()) {
                                st.ts_paths = paths
                                    .iter()
                                    .map(|(k, v)| {
                                        let targets: Vec<String> = v
                                            .as_array()
                                            .map(|arr| {
                                                arr.iter()
                                                    .filter_map(|t| t.as_str())
                                                    .map(|t| t.to_string())
                                                    .collect()
                                            })
                                            .unwrap_or_default();
                                        (k.clone(), targets)
                                    })
                                    .collect();
                            }
                        }
                    }
                }
            }
        }
        if e.name == "package.json" {
            if let Some(data) = sess.fs.read_prefix(&e.path, TEXT_READ_LIMIT) {
                if let Ok(text) = std::str::from_utf8(&data) {
                    if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(text) {
                        let base_dir = dirname(&rel).to_string();
                        // ルート package.json の "imports"（# エイリアス）を記録
                        if base_dir.is_empty() {
                            if let Some(imp) = pkg.get("imports").and_then(|i| i.as_object()) {
                                for (k, v) in imp {
                                    if let Some(s) = v.as_str() {
                                        st.pkg_imports.push((k.clone(), s.to_string()));
                                    }
                                }
                            }
                        }
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

#[cfg(test)]
mod tests {
    use super::{build_rs_module_map, detect_cycles, rs_crate_of, rs_mod_key};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn crate_of_nested_cargo() {
        let dirs: BTreeSet<String> =
            ["".to_string(), "sub".to_string(), "rust/crates/core".to_string()]
                .into_iter()
                .collect();
        assert_eq!(rs_crate_of("src/lib.rs", &dirs), "");
        assert_eq!(rs_crate_of("sub/src/cli.rs", &dirs), "sub");
        assert_eq!(rs_crate_of("rust/crates/core/src/run.rs", &dirs), "rust/crates/core");
        // "subXtra" のような前方一致の誤マッチをしない
        assert_eq!(rs_crate_of("subxtra/src/a.rs", &dirs), "");
        // Cargo.toml がどこにも無ければスキャンルート扱い（従来挙動）
        assert_eq!(rs_crate_of("src/lib.rs", &BTreeSet::new()), "");
    }

    #[test]
    fn mod_key_is_crate_relative() {
        assert_eq!(rs_mod_key("sub/src/lib.rs", "sub"), Some(vec![]));
        assert_eq!(
            rs_mod_key("sub/src/util/mod.rs", "sub"),
            Some(vec!["util".to_string()])
        );
        assert_eq!(rs_mod_key("sub/src/cli.rs", "sub"), Some(vec!["cli".to_string()]));
        // クレートの src/ 外は対象外
        assert_eq!(rs_mod_key("sub/build.rs", "sub"), None);
        assert_eq!(rs_mod_key("sub/src/cli.rs", ""), None);
    }

    #[test]
    fn module_map_separates_crates() {
        let files: BTreeSet<String> = [
            "src/lib.rs",
            "src/config.rs",
            "sub/src/lib.rs",
            "sub/src/config.rs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let dirs: BTreeSet<String> = ["".to_string(), "sub".to_string()].into_iter().collect();
        let map = build_rs_module_map(&files, &dirs);
        assert_eq!(
            map.get(&("".to_string(), vec!["config".to_string()])),
            Some(&"src/config.rs".to_string())
        );
        assert_eq!(
            map.get(&("sub".to_string(), vec!["config".to_string()])),
            Some(&"sub/src/config.rs".to_string())
        );
        assert_eq!(
            map.get(&("sub".to_string(), vec![])),
            Some(&"sub/src/lib.rs".to_string())
        );
    }

    #[test]
    fn cycles_basic() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), vec!["b".to_string()]);
        m.insert("b".to_string(), vec!["a".to_string()]);
        assert_eq!(
            detect_cycles(&m),
            vec![vec!["a".to_string(), "b".to_string(), "a".to_string()]]
        );
    }

    #[test]
    fn cycles_deep_chain_no_stack_overflow() {
        // 10万ノードの直列 import 連鎖＋末尾→先頭の逆辺（1つの巨大サイクル）。
        // 再帰 DFS だとコールスタックが溢れるケース。
        let n = 100_000usize;
        let mut m = BTreeMap::new();
        for i in 0..n {
            let next = if i + 1 == n { 0 } else { i + 1 };
            m.insert(format!("f{:06}", i), vec![format!("f{:06}", next)]);
        }
        let cycles = detect_cycles(&m);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), n + 1);
    }
}
