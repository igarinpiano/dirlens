#!/usr/bin/env python3
"""
dirlens – ファイルサイズ付きディレクトリツリー表示ツール
対応環境: macOS / Linux / Windows  (Python 3.8+)

Copyright 2026 Igarin
Licensed under the Apache License, Version 2.0.
See the LICENSE file or http://www.apache.org/licenses/LICENSE-2.0
"""

import io, json, os, sys, stat as _stat, argparse, fnmatch, datetime, subprocess, re, ast, html
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

# ─── カラー設定 ──────────────────────────────────────────────
def _enable_color():
    if not hasattr(sys.stdout, "isatty") or not sys.stdout.isatty():
        return False
    if os.name == "nt":
        try:
            import ctypes
            k = ctypes.windll.kernel32
            k.SetConsoleMode(k.GetStdHandle(-11), 7)
        except Exception:
            pass
        return bool(os.environ.get("WT_SESSION") or os.environ.get("TERM_PROGRAM")
                    or os.environ.get("TERM") or os.environ.get("ANSICON"))
    return True

USE_COLOR = _enable_color()
RESET = "\033[0m"; BOLD = "\033[1m"; DIM = "\033[2m"
BLUE = "\033[34m"; CYAN = "\033[36m"; GREEN = "\033[32m"; MAGENTA = "\033[35m"
RED = "\033[31m"; YELLOW = "\033[33m"

def c(text, *codes):
    return ("".join(codes) + text + RESET) if USE_COLOR else text


# ─── フォーマット ─────────────────────────────────────────────
def fmt_size(n, partial=False):
    sfx = "+" if partial else ""
    if n == 0: return f"0{sfx} bytes"
    for unit, f in (("TB",1<<40),("GB",1<<30),("MB",1<<20),("KB",1<<10)):
        if n >= f:
            return f"{str(f'{n/f:.2f}').rstrip('0').rstrip('.')}{sfx} {unit}"
    return f"{n}{sfx} {'byte' if (n==1 and not partial) else 'bytes'}"

def fmt_count(nd, nf, denied=False):
    sfx = "+" if denied else ""
    d_word = "dir"  if (nd == 1 and not denied) else "dirs"
    f_word = "file" if (nf == 1 and not denied) else "files"
    return f"{nd}{sfx} {d_word}, {nf}{sfx} {f_word}"

def fmt_date(mtime):
    sec = int((datetime.datetime.now() - datetime.datetime.fromtimestamp(mtime)).total_seconds())
    if sec <    60: return "今"
    if sec <  3600: return f"{sec//60}分前"
    if sec < 86400: return f"{sec//3600}時間前"
    d = sec // 86400
    if d == 1: return "昨日"
    if d <  7: return f"{d}日前"
    if d < 30: return f"{d//7}週間前"
    if d < 365: return f"{d//30}ヶ月前"
    return f"{d//365}年前"

def fmt_bar(part, total, width=10):
    pct = min(100, int(part * 100 / total)) if total else 0
    filled = round(pct * width / 100)
    return f"[{'█'*filled}{'░'*(width-filled)}]{pct:4d}%"

def parse_size(s):
    s = s.strip()
    for sfx, mult in [("TB",1<<40),("GB",1<<30),("MB",1<<20),("KB",1<<10),
                       ("T",1<<40),("G",1<<30),("M",1<<20),("K",1<<10)]:
        if s.upper().endswith(sfx):
            try: return int(float(s[:-len(sfx)]) * mult)
            except ValueError: break
    try: return int(s)
    except ValueError:
        raise argparse.ArgumentTypeError(f"無効なサイズ: '{s}'（例: 50M, 1G, 500K）")

def fmt_perm_info(entry, cfg):
    """パーミッション・ユーザー・グループ情報を返す。"""
    if not cfg.show_perms and not cfg.show_user and not cfg.show_group:
        return ""
    try:
        st = entry.stat(follow_symlinks=False)
    except OSError:
        return ""
    parts = []
    if cfg.show_perms:
        parts.append(_stat.filemode(st.st_mode))
    if cfg.show_user:
        try:
            import pwd
            parts.append(pwd.getpwuid(st.st_uid).pw_name)
        except (ImportError, KeyError, AttributeError):
            parts.append(str(getattr(st, "st_uid", "?")))
    if cfg.show_group:
        try:
            import grp
            parts.append(grp.getgrgid(st.st_gid).gr_name)
        except (ImportError, KeyError, AttributeError):
            parts.append(str(getattr(st, "st_gid", "?")))
    return c("[" + " ".join(parts) + "] ", DIM) if parts else ""


# ─── クリップボード ───────────────────────────────────────────
def copy_to_clipboard(text):
    try:
        if sys.platform == "darwin":
            subprocess.run(["pbcopy"], input=text.encode(), check=True,
                           stderr=subprocess.DEVNULL); return True
        if sys.platform == "win32":
            subprocess.run(["clip"], input=text.encode("utf-16"), check=True,
                           stderr=subprocess.DEVNULL); return True
        for cmd in [["wl-copy"],["xclip","-selection","clipboard"],
                    ["xsel","--clipboard","--input"]]:
            try:
                subprocess.run(cmd, input=text.encode(), check=True,
                               stderr=subprocess.DEVNULL); return True
            except FileNotFoundError: continue
        return False
    except Exception: return False

def strip_ansi(text):
    return re.sub(r'\033\[[0-9;]*[mK]', '', text)


# ─── 絵文字 ───────────────────────────────────────────────────
_EMOJI_EXT = {
    ".py":"🐍",  ".js":"🟨",  ".ts":"🔷",  ".jsx":"⚛️",  ".tsx":"⚛️",
    ".rs":"🦀",  ".go":"🐹",  ".rb":"💎",  ".java":"☕",  ".kt":"🟣",
    ".c":"🔧",   ".cpp":"🔧", ".h":"🔧",   ".cs":"🔵",   ".php":"🐘",
    ".swift":"🍊",".dart":"🎯",
    ".json":"📋",".yaml":"⚙️",".yml":"⚙️", ".toml":"⚙️", ".xml":"📰",
    ".csv":"📊", ".sql":"🗄️", ".db":"🗄️",  ".ini":"⚙️",  ".env":"🔑",
    ".md":"📝",  ".txt":"📄", ".pdf":"📕", ".doc":"📘",  ".docx":"📘",
    ".html":"🌐",".css":"🎨", ".scss":"🎨",
    ".png":"🖼️", ".jpg":"🖼️", ".jpeg":"🖼️",".gif":"🖼️",
    ".svg":"🎨", ".ico":"🖼️", ".webp":"🖼️",
    ".mp4":"🎬", ".mov":"🎬", ".mp3":"🎵", ".wav":"🎵", ".flac":"🎵",
    ".zip":"📦", ".tar":"📦", ".gz":"📦",  ".rar":"📦", ".7z":"📦",
    ".sh":"📜",  ".bash":"📜",".zsh":"📜", ".bat":"📜", ".ps1":"📜",
}
_EMOJI_NAME = {
    "dockerfile":"🐳","makefile":"⚙️","license":"⚖️",
    ".gitignore":"🚫","package.json":"📦","requirements.txt":"📋",
    "pyproject.toml":"⚙️","cargo.toml":"📦","readme.md":"📖",
}

def get_emoji(name, is_dir=False):
    if is_dir: return "📁"
    lower = name.lower()
    return _EMOJI_NAME.get(lower) or _EMOJI_EXT.get(os.path.splitext(lower)[1], "📄")


# ─── .gitignore（否定パターン対応） ──────────────────────────
_gi_cache = {}

def load_gitignore(directory):
    if directory in _gi_cache: return _gi_cache[directory]
    pats = []
    p = os.path.join(directory, ".gitignore")
    if os.path.isfile(p):
        try:
            with open(p, encoding="utf-8", errors="ignore") as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#"):
                        pats.append(line)
        except OSError: pass
    _gi_cache[directory] = pats
    return pats

def is_ignored(name, rel_path, is_dir, patterns):
    """パターンを順番に評価し最後にマッチしたルールが勝つ（!否定対応）。"""
    rel = rel_path.replace("\\", "/")
    result = False
    for pat in patterns:
        negated = pat.startswith("!")
        p = pat.lstrip("!")
        dir_only = p.endswith("/")
        p = p.rstrip("/")
        if dir_only and not is_dir:
            continue
        matched = False
        if p.startswith("/"):
            matched = fnmatch.fnmatch(rel, p.lstrip("/"))
        else:
            matched = (fnmatch.fnmatch(name, p) or
                       fnmatch.fnmatch(rel, p) or
                       fnmatch.fnmatch(rel, "*/" + p))
        if matched:
            result = not negated
    return result

def _extend_pats(active_pats, path, cfg):
    if not cfg.use_gitignore: return active_pats
    if os.path.normpath(path) == os.path.normpath(cfg.root): return active_pats
    local = load_gitignore(path)
    if not local: return active_pats
    rel_dir = os.path.relpath(path, cfg.root).replace("\\", "/")
    adjusted = []
    for pat in local:
        neg = pat.startswith("!")
        p = pat.lstrip("!")
        if p.startswith("/"):
            adjusted.append(("!" if neg else "") + "/" + rel_dir + p)
        else:
            adjusted.append(pat)
    return active_pats + adjusted


# ─── ディレクトリサイズ ───────────────────────────────────────
# _prefetch_sizes() が ThreadPoolExecutor から dir_size() を並列に呼ぶため、このキャッシュは
# 複数スレッドから読み書きされる。dict への単一代入は GIL 下で原子的なので破損はしないが、
# 同一サブディレクトリが複数スレッドで重複計算されうる（最悪でも無駄な再計算のみで、結果は不変）。
_sz_cache = {}

def dir_size(path):
    """ディレクトリの合計サイズを返す。(size, has_errors) のタプル。"""
    if path in _sz_cache: return _sz_cache[path]
    total = 0
    has_errors = False
    try:
        with os.scandir(path) as it:
            for e in it:
                try:
                    if e.is_file(follow_symlinks=False):
                        total += e.stat(follow_symlinks=False).st_size
                    elif e.is_dir(follow_symlinks=False):
                        sub_sz, sub_err = dir_size(e.path)
                        total += sub_sz
                        if sub_err: has_errors = True
                except OSError:
                    has_errors = True
    except OSError:
        has_errors = True
    result = (total, has_errors)
    _sz_cache[path] = result
    return result

def _prefetch_sizes(root_path):
    try:
        top = [e.path for e in os.scandir(root_path)
               if e.is_dir(follow_symlinks=False)]
    except OSError: return
    if len(top) < 2: return
    workers = min(len(top), (os.cpu_count() or 1), 8)
    with ThreadPoolExecutor(max_workers=workers) as ex:
        list(ex.map(dir_size, top))


# ════════════════════════════════════════════════════════════
# AI/エージェント向け解析機能
# ════════════════════════════════════════════════════════════

_BINARY_EXTS = {
    ".png",".jpg",".jpeg",".gif",".bmp",".ico",".webp",
    ".mp3",".mp4",".mov",".avi",".wav",".flac",".ogg",".webm",".mkv",
    ".zip",".tar",".gz",".rar",".7z",".bz2",".xz",
    ".pdf",".doc",".docx",".xls",".xlsx",".ppt",".pptx",
    ".exe",".dll",".so",".dylib",".bin",".o",".a",".class",".jar",
    ".woff",".woff2",".ttf",".otf",".eot",
    ".db",".sqlite",".sqlite3",".pyc",".pyo",".whl",
}

def _is_probably_binary(name):
    return os.path.splitext(name)[1].lower() in _BINARY_EXTS


# テキスト本文の最大読み込みバイト数。トークン/行数/TODO/アウトラインで共有し、
# 1ファイルにつき open は1回で済ませる（精度の目安についてはREADME「精度について」参照）。
_TEXT_READ_LIMIT = 5_000_000

# --- トークン数概算（-T / --tokens） -------------------------
def estimate_tokens(text, byte_len=0, actual_size=None, truncated=False):
    """テキスト本文からトークン数を概算する（呼び出し側で読み込み済みの本文を渡す）。
    あくまで大まかな目安（英数字記号は約4文字/トークン、それ以外（日本語等）は約1.5文字/トークンとして概算）。
    打ち切られている場合は読み込んだバイト数と実サイズの比でスケール補正する。
    """
    if not text:
        return 0
    ascii_chars = sum(1 for ch in text if ord(ch) < 128)
    other_chars = len(text) - ascii_chars
    tokens = ascii_chars / 4 + other_chars / 1.5
    if truncated and actual_size and byte_len > 0:
        tokens *= actual_size / byte_len
    return max(1, round(tokens))

def fmt_tokens(n):
    if n is None: return None
    if n >= 1000:
        s = f"{n/1000:.1f}".rstrip("0").rstrip(".")
        return f"~{s}K tok"
    return f"~{n} tok"

def count_lines(text, byte_len=0, actual_size=None, truncated=False):
    """テキスト本文の行数を数える（呼び出し側で読み込み済みの本文を渡す）。
    打ち切られている場合は読み込んだバイト数と実サイズの比でスケール補正する。
    """
    if not text:
        return 0
    n = text.count("\n") + (0 if text.endswith("\n") else 1)
    if truncated and actual_size and byte_len > 0:
        n = max(1, round(n * actual_size / byte_len))
    return n


# --- git連携（-H / --git） ------------------------------------
def load_git_log(root, max_commits=2000):
    """直近コミット履歴から各ファイルの最終更新コミット情報と変更回数を取得する。
    gitが無い／リポジトリでない場合は空dict/空dictを返す（エラーにはしない）。
    パフォーマンスのため履歴は直近 max_commits 件までに限定（古いファイルは情報なしになる場合あり）。
    戻り値: (file_map, change_counts)
      file_map: {relpath: {"hash","date","author","subject"}}  最終コミット情報
      change_counts: {relpath: int}  走査した履歴内での変更回数（ホットスポット検出用）
    """
    try:
        # encoding を明示する: Windows では locale (cp1252 等) でデコードされ、
        # 非ASCIIのコミットメッセージで UnicodeDecodeError になるため。
        proc = subprocess.run(
            ["git", "-C", root, "log", "-n", str(max_commits),
             "--name-only", "--date=relative",
             "--pretty=format:\x01%H\x02%ad\x02%an\x02%s\x03"],
            capture_output=True, encoding="utf-8", errors="replace",
            timeout=8, check=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError,
            subprocess.TimeoutExpired, OSError):
        return {}, {}

    file_map = {}
    change_counts = {}
    current = None
    for raw in proc.stdout.split("\n"):
        line = raw.strip("\r")
        if line.startswith("\x01"):
            body = line[1:]
            if body.endswith("\x03"):
                body = body[:-1]
            parts = body.split("\x02", 3)
            current = ({"hash": parts[0][:7], "date": parts[1],
                        "author": parts[2], "subject": parts[3]}
                       if len(parts) == 4 else None)
        elif line.strip() and current is not None:
            fp = line.strip().replace("\\", "/")
            if fp not in file_map:
                file_map[fp] = current
            change_counts[fp] = change_counts.get(fp, 0) + 1
    return file_map, change_counts

def fmt_git(g):
    if not g: return None
    subj = g["subject"].strip()
    if len(subj) > 30:
        subj = subj[:30] + "…"
    return f'"{subj}" ({g["date"]})'


# --- TODO/FIXME抽出（-K / --todo） -----------------------------
_TODO_RE = re.compile(r'\b(TODO|FIXME|HACK|XXX)\b[:\s]?(.*)', re.IGNORECASE)

def scan_todos(text):
    """テキスト本文から TODO/FIXME/HACK/XXX を抽出する（呼び出し側で読み込み済みの本文を渡す）。"""
    if not text:
        return []
    results = []
    for i, line in enumerate(text.split("\n"), 1):
        m = _TODO_RE.search(line)
        if m:
            snippet = line.strip()
            if len(snippet) > 80:
                snippet = snippet[:80] + "…"
            results.append((i, m.group(1).upper(), snippet))
    return results


# --- テスト欠落検知（-V / --missing-tests） ---------------------
_SOURCE_EXTS_FOR_TESTS = {".py", ".js", ".jsx", ".ts", ".tsx", ".go"}

def _is_test_file(name):
    lower = name.lower()
    stem, ext = os.path.splitext(lower)
    if stem.startswith("test_") or stem.endswith("_test"):
        return True
    if stem.endswith(".test") or stem.endswith(".spec"):
        return True
    return False


# --- import/依存グラフ解析（-M / --imports） ---------------------
# Pythonは標準ライブラリの ast モジュールで正確に解析。
# JS/TS/Go/Rustは正規表現ベースの抽出＋相対パス解決（best-effort）。
# 外部パッケージ（react, lodash, requests 等）はプロジェクト内ファイルに
# 解決できないため "external" 扱いとし、依存グラフには含めない。

def extract_imports_py(path):
    """ASTでPythonのimport文を正確に抽出する。
    戻り値: [(module_str, level, [imported_names])]  level>0 は相対import。
    """
    try:
        with open(path, encoding="utf-8", errors="ignore") as f:
            source = f.read()
        tree = ast.parse(source, filename=path)
    except (OSError, SyntaxError, ValueError, RecursionError):
        return []
    out = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                out.append((alias.name, 0, None))
        elif isinstance(node, ast.ImportFrom):
            out.append((node.module or "", node.level, [a.name for a in node.names]))
    return out

_JS_IMPORT_PATTERNS = [
    re.compile(r'''import\s+(?:[\w*\s{},]+\s+from\s+)?['"]([^'"]+)['"]'''),
    re.compile(r'''export\s+(?:[\w*\s{},]+\s+from\s+)?['"]([^'"]+)['"]'''),
    re.compile(r'''require\(\s*['"]([^'"]+)['"]\s*\)'''),
    re.compile(r'''import\(\s*['"]([^'"]+)['"]\s*\)'''),  # 動的import()
]

def extract_imports_js(path):
    try:
        with open(path, encoding="utf-8", errors="ignore") as f:
            text = f.read()
    except OSError:
        return []
    found = []
    for pat in _JS_IMPORT_PATTERNS:
        found.extend(pat.findall(text))
    return found

_GO_IMPORT_BLOCK_RE = re.compile(r'import\s*\(([^)]*)\)', re.DOTALL)
_GO_IMPORT_LINE_RE  = re.compile(r'import\s+"([^"]+)"')
_GO_IMPORT_ITEM_RE  = re.compile(r'"([^"]+)"')

def extract_imports_go(path):
    try:
        with open(path, encoding="utf-8", errors="ignore") as f:
            text = f.read()
    except OSError:
        return []
    found = []
    block = _GO_IMPORT_BLOCK_RE.search(text)
    if block:
        found.extend(_GO_IMPORT_ITEM_RE.findall(block.group(1)))
    found.extend(_GO_IMPORT_LINE_RE.findall(text))
    return found

_RS_USE_RE = re.compile(r'^\s*(?:pub\s+)?use\s+([\w:]+)', re.MULTILINE)
_RS_MOD_RE = re.compile(r'^\s*(?:pub\s+)?mod\s+(\w+)\s*;', re.MULTILINE)

def extract_imports_rs(path):
    """戻り値: (use文のパスリスト, mod宣言のモジュール名リスト)"""
    try:
        with open(path, encoding="utf-8", errors="ignore") as f:
            text = f.read()
    except OSError:
        return [], []
    return _RS_USE_RE.findall(text), _RS_MOD_RE.findall(text)


def _py_module_key(relpath):
    """'pkg/sub/mod.py' -> 'pkg.sub.mod'、'pkg/sub/__init__.py' -> 'pkg.sub'"""
    parts = relpath.replace("\\", "/").split("/")
    if parts[-1] == "__init__.py":
        parts = parts[:-1]
    elif parts[-1].endswith(".py"):
        parts[-1] = parts[-1][:-3]
    return ".".join(parts)

def _resolve_relative_path(base_dir, target, project_files):
    """JS/TS の相対import（'./foo'、'../bar'）をプロジェクト内ファイルに解決する。"""
    candidate = os.path.normpath(os.path.join(base_dir, target)).replace("\\", "/")
    for suffix in ("", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
                   "/index.js", "/index.ts", "/index.jsx", "/index.tsx"):
        cand = candidate + suffix
        if cand in project_files:
            return cand
    return None

def _resolve_rust_crate_path(use_path, project_files):
    """'crate::foo::bar::Baz' をプロジェクト内ファイルに解決する（src/foo/bar.rs 等）。
    self:: / super:: および外部crateは簡易実装のため非対応（external扱い）。
    """
    if not use_path.startswith("crate::"):
        return None
    body = use_path.split("::", 1)[1]
    segments = [s for s in body.split("::") if s and s not in ("self", "*")]
    for cut in (len(segments), len(segments) - 1):
        if cut <= 0: continue
        path_part = "/".join(segments[:cut])
        for cand in (f"src/{path_part}.rs", f"src/{path_part}/mod.rs"):
            if cand in project_files:
                return cand
    return None


# --- エントリーポイント検出（-N / --entry） ----------------------
_ENTRY_NAMES_LOWER = {
    "main.py", "__main__.py", "app.py", "server.py", "manage.py", "wsgi.py", "asgi.py",
    "index.js", "index.ts", "index.mjs", "index.cjs",
    "main.js", "main.ts", "server.js", "server.ts", "app.js", "app.ts",
    "main.go", "main.rs",
    "makefile", "dockerfile", "docker-compose.yml", "docker-compose.yaml",
}

# --- 設定ファイル検出（-F / --config） ---------------------------
_CONFIG_NAMES_LOWER = {
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
}

def detect_cycles(imports_map):
    """importsグラフ（ローカル依存のみ）から循環依存を検出する（DFSベース、best-effort）。
    重複するサイクル（同じファイル集合）は1つにまとめる。
    戻り値: [[file1, file2, ..., file1], ...]  各リストは循環の経路（先頭=末尾で1周）
    """
    WHITE, GRAY, BLACK = 0, 1, 2
    color = {}
    stack = []
    cycles = []
    seen_keys = set()

    def dfs(node):
        color[node] = GRAY
        stack.append(node)
        for nxt in imports_map.get(node, []):
            c = color.get(nxt, WHITE)
            if c == WHITE:
                dfs(nxt)
            elif c == GRAY:
                idx = stack.index(nxt)
                cycle = stack[idx:] + [nxt]
                key = frozenset(cycle[:-1])
                if key not in seen_keys:
                    seen_keys.add(key)
                    cycles.append(cycle)
        stack.pop()
        color[node] = BLACK

    for node in sorted(imports_map.keys()):
        if color.get(node, WHITE) == WHITE:
            dfs(node)
    return cycles


def build_project_index(root, cfg, active_pats=None):
    """テスト欠落検知・エントリーポイント検出・設定ファイル検出・import依存グラフのため、
    プロジェクト全体を一度だけスキャンする。
    .gitignore は -G 指定時のみ尊重する。-G なしで巨大な node_modules 等があると遅くなる場合がある。
    active_pats: ルートの .gitignore パターン（main() で読み込み済みのものを渡す）。
    戻り値: (untested_relpaths, entry_relpaths, config_relpaths,
             imports_map, imported_by_map, external_map, cycles)
    """
    all_names = set()
    all_relpaths = set()
    source_files = []
    entry_set = set()
    pkg_entry_candidates = set()  # package.json の main/bin（実在確認は walk 後にまとめて行う）
    config_set = set()
    py_module_map = {}
    go_module_name = [None]  # nonlocalの代わりにリストで包む

    def walk(path, active_pats):
        pats = _extend_pats(active_pats, path, cfg) if cfg.use_gitignore else active_pats
        try:
            entries = list(os.scandir(path))
        except OSError:
            return
        entries = [e for e in entries if cfg.show_all or not e.name.startswith(".")]
        if pats:
            entries = [e for e in entries
                       if not is_ignored(e.name, os.path.relpath(e.path, root),
                                         e.is_dir(follow_symlinks=False), pats)]
        for e in entries:
            if e.is_dir(follow_symlinks=False):
                walk(e.path, pats)
                continue
            relpath = os.path.relpath(e.path, root).replace("\\", "/")
            stem, ext = os.path.splitext(e.name)
            all_names.add(e.name.lower())
            all_relpaths.add(relpath)
            if ext.lower() in _SOURCE_EXTS_FOR_TESTS and not _is_test_file(e.name):
                source_files.append((relpath, stem, ext.lower()))
            if e.name.lower() in _ENTRY_NAMES_LOWER:
                entry_set.add(relpath)
            if e.name.lower() in _CONFIG_NAMES_LOWER:
                config_set.add(relpath)
            if ext.lower() == ".py":
                py_module_map[_py_module_key(relpath)] = relpath
            if e.name == "go.mod":
                try:
                    with open(e.path, encoding="utf-8", errors="ignore") as f:
                        for line in f:
                            line = line.strip()
                            if line.startswith("module "):
                                go_module_name[0] = line.split(None, 1)[1].strip()
                                break
                except OSError:
                    pass
            if e.name == "package.json":
                try:
                    with open(e.path, encoding="utf-8") as f:
                        pkg = json.load(f)
                    base_dir = os.path.dirname(relpath)
                    main_field = pkg.get("main")
                    if isinstance(main_field, str):
                        pkg_entry_candidates.add(os.path.normpath(
                            os.path.join(base_dir, main_field)).replace("\\", "/"))
                    bin_field = pkg.get("bin")
                    if isinstance(bin_field, str):
                        pkg_entry_candidates.add(os.path.normpath(
                            os.path.join(base_dir, bin_field)).replace("\\", "/"))
                    elif isinstance(bin_field, dict):
                        for v in bin_field.values():
                            if isinstance(v, str):
                                pkg_entry_candidates.add(os.path.normpath(
                                    os.path.join(base_dir, v)).replace("\\", "/"))
                except (OSError, json.JSONDecodeError, AttributeError, ValueError):
                    pass

    walk(root, active_pats or [])

    # package.json の main/bin は実在するファイルのみエントリーポイントとして扱う
    # （未ビルドや gitignore 済みのパスで件数が水増しされるのを防ぐ）。
    entry_set |= (pkg_entry_candidates & all_relpaths)

    untested = set()
    for relpath, stem, ext in source_files:
        candidates = {f"test_{stem}{ext}", f"{stem}_test{ext}",
                      f"{stem}.test{ext}", f"{stem}.spec{ext}"}
        if not (candidates & all_names):
            untested.add(relpath)

    imports_map, imported_by_acc, external_map = {}, {}, {}
    if cfg.show_imports:
        # Goのローカルimport解決を O(import数 × ファイル数) から O(import数 × ディレクトリ数)
        # に下げるため、.goファイルをディレクトリごとに前計算しておく。
        go_files_by_dir = {}
        for r in all_relpaths:
            if r.endswith(".go"):
                go_files_by_dir.setdefault(os.path.dirname(r), []).append(r)

        for relpath in sorted(all_relpaths):
            ext = os.path.splitext(relpath)[1].lower()
            full_path = os.path.join(root, relpath)
            base_dir = os.path.dirname(relpath)
            local_targets, external_raw = set(), []

            if ext == ".py":
                for mod, level, names in extract_imports_py(full_path):
                    if level and level > 0:
                        pkg_parts = base_dir.split("/") if base_dir else []
                        up = level - 1
                        pkg_parts = pkg_parts[:-up] if (up and up <= len(pkg_parts)) else \
                                    (pkg_parts if not up else [])
                        target_key = ".".join(pkg_parts + ([mod] if mod else []))
                        resolved = None
                        # 'from .pkg import sub' は sub がサブモジュールの可能性があるため、
                        # まず names を「パッケージ内のサブモジュール」として解決を試みる
                        # （例: from . import app_helpers → app_helpers.py）
                        if names:
                            for nm in names:
                                cand_key = f"{target_key}.{nm}" if target_key else nm
                                resolved = py_module_map.get(cand_key)
                                if resolved: break
                        # 解決できなければ、import先自体がモジュールである通常パターンにフォールバック
                        # （例: from .utils import helper_function → utils.py）
                        if not resolved:
                            resolved = py_module_map.get(target_key)
                        if resolved and resolved != relpath:
                            local_targets.add(resolved)
                        else:
                            external_raw.append("." * level + (mod or ""))
                    else:
                        resolved = py_module_map.get(mod)
                        if resolved and resolved != relpath:
                            local_targets.add(resolved)
                        else:
                            external_raw.append(mod)

            elif ext in (".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"):
                for spec in extract_imports_js(full_path):
                    if spec.startswith(".") or spec.startswith("/"):
                        resolved = _resolve_relative_path(base_dir, spec, all_relpaths)
                        if resolved and resolved != relpath:
                            local_targets.add(resolved)
                        else:
                            external_raw.append(spec)
                    else:
                        external_raw.append(spec)

            elif ext == ".go":
                mod_name = go_module_name[0]
                for spec in extract_imports_go(full_path):
                    if mod_name and spec.startswith(mod_name):
                        sub = spec[len(mod_name):].lstrip("/")
                        # import先パッケージのディレクトリ sub と、その配下のディレクトリの
                        # .goファイルを集める（元の startswith/dirname 判定と等価）。
                        candidates = []
                        for d, files in go_files_by_dir.items():
                            if d == sub or (sub and d.startswith(sub + "/")):
                                candidates.extend(files)
                        if candidates:
                            for cand in candidates:
                                if cand != relpath:
                                    local_targets.add(cand)
                        else:
                            external_raw.append(spec)
                    else:
                        external_raw.append(spec)

            elif ext == ".rs":
                uses, mods = extract_imports_rs(full_path)
                for m in mods:
                    for cand in (f"{base_dir}/{m}.rs" if base_dir else f"{m}.rs",
                                f"{base_dir}/{m}/mod.rs" if base_dir else f"{m}/mod.rs"):
                        if cand in all_relpaths:
                            local_targets.add(cand)
                for u in uses:
                    resolved = _resolve_rust_crate_path(u, all_relpaths)
                    if resolved and resolved != relpath:
                        local_targets.add(resolved)
                    else:
                        external_raw.append(u)

            if local_targets:
                imports_map[relpath] = sorted(local_targets)
                # sorted() で回す: set の順序はハッシュランダム化で実行毎に変わるため、
                # ここが imported_by_acc の挿入順（＝依存度タイ時の表示順）を非決定論にしていた。
                for t in sorted(local_targets):
                    imported_by_acc.setdefault(t, set()).add(relpath)
            if external_raw:
                # 重複除去しつつ最大10件まで
                seen = []
                for x in external_raw:
                    if x and x not in seen:
                        seen.append(x)
                external_map[relpath] = seen[:10]

    imported_by_map = {k: sorted(v) for k, v in imported_by_acc.items()}
    cycles = detect_cycles(imports_map) if cfg.show_imports else []
    return untested, entry_set, config_set, imports_map, imported_by_map, external_map, cycles


# --- シンボルアウトライン（-O / --outline） ----------------------
# 正規表現ベースの簡易抽出。AST解析ではないため、デコレータが複雑な場合や
# 複数行にまたがる関数シグネチャ等は取得漏れすることがある（best-effort）。
_JS_TS_PATTERNS = [
    (re.compile(r'^\s*(?:export\s+)?(?:default\s+)?class\s+(\w+)'), "class"),
    (re.compile(r'^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s+(\w+)\s*\('), "func"),
    (re.compile(r'^\s*export\s+(?:default\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\('), "func"),
    (re.compile(r'^\s*(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(.*\)\s*=>'), "func"),
]
_OUTLINE_PATTERNS = {
    ".py": [
        (re.compile(r'^(\s*)class\s+(\w+)'), "class"),
        (re.compile(r'^(\s*)(?:async\s+)?def\s+(\w+)\s*\('), "def"),
    ],
    ".go": [
        (re.compile(r'^func\s+(?:\([^)]*\)\s+)?(\w+)\s*\('), "func"),
        (re.compile(r'^type\s+(\w+)\s+struct'), "struct"),
        (re.compile(r'^type\s+(\w+)\s+interface'), "interface"),
    ],
    ".rs": [
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)'), "fn"),
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)'), "struct"),
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)'), "enum"),
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)'), "trait"),
    ],
}
for _e in (".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"):
    _OUTLINE_PATTERNS[_e] = _JS_TS_PATTERNS

def _is_public_symbol(ext, line, name):
    """言語ごとの公開API判定（best-effort）。
    Python: アンダースコア始まりでない / JS-TS: exportキーワードを含む行 /
    Go: 識別子が大文字始まり（言語の公開規約そのもの） / Rust: pubキーワードを含む行。
    """
    if ext == ".py":
        return not name.startswith("_")
    if ext in (".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"):
        return bool(re.search(r'\bexport\b', line))
    if ext == ".go":
        return bool(name) and name[0:1].isupper()
    if ext == ".rs":
        return bool(re.search(r'\bpub\b', line))
    return True  # 不明な場合は除外しない（保守的に倒す）

def extract_outline(text, ext, limit_lines=4000):
    """対応言語（Python/JS/TS/Go/Rust）の関数・クラス名を正規表現で簡易抽出する。
    対応外の拡張子は None を返す（「対応していない」ことを明示するため空リストとは区別）。
    text は呼び出し側で読み込み済みの本文。読めなかった場合は空リストを渡す。
    戻り値: [(kind, name, is_public), ...]
    """
    patterns = _OUTLINE_PATTERNS.get(ext)
    if not patterns:
        return None
    if not text:
        return []
    lines = text.split("\n")[:limit_lines]
    out = []
    for line in lines:
        for pat, kind in patterns:
            m = pat.match(line)
            if m:
                name = m.group(m.lastindex)
                out.append((kind, name, _is_public_symbol(ext, line, name)))
                break
    return out

def fmt_outline(outline, limit=5):
    if not outline:
        return None
    items = [f"{kind} {name}" for kind, name, _pub in outline]
    shown = items[:limit]
    s = ", ".join(shown)
    if len(items) > limit:
        s += f", +{len(items)-limit}"
    return s


def _file_extras(entry, relpath, cfg):
    """有効になっているAI解析フラグに応じて、ファイル単位の追加情報を計算する。"""
    extras = {}
    ext = os.path.splitext(entry.name)[1].lower()

    # トークン数・行数・TODO・アウトラインはいずれもファイル本文を必要とする。
    # 同じファイルを複数回開かないよう、本文はここで一度だけ読み込んで共有する。
    need_text = cfg.show_tokens or cfg.show_todo or cfg.show_outline
    text = None
    byte_len = 0
    truncated = False
    is_binary = _is_probably_binary(entry.name)
    if need_text and not is_binary:
        try:
            with open(entry.path, "rb") as f:
                data = f.read(_TEXT_READ_LIMIT + 1)
        except OSError:
            data = None
        if data is not None:
            if b"\x00" in data[:8192]:
                is_binary = True
            else:
                truncated = len(data) > _TEXT_READ_LIMIT
                if truncated:
                    data = data[:_TEXT_READ_LIMIT]
                byte_len = len(data)
                text = data.decode("utf-8", errors="ignore")

    if cfg.show_tokens:
        if is_binary:
            extras["tokens"] = None
            extras["lines"] = None
        else:
            try: sz = entry.stat(follow_symlinks=True).st_size
            except OSError: sz = None
            extras["tokens"] = estimate_tokens(text, byte_len, sz, truncated)
            extras["lines"] = count_lines(text, byte_len, sz, truncated)

    if cfg.show_git:
        extras["git"] = cfg.git_map.get(relpath)

    if cfg.show_todo:
        extras["todos"] = scan_todos(text)

    if cfg.show_entry:
        extras["is_entry"] = relpath in cfg.entry_set

    if cfg.show_config:
        extras["is_config"] = relpath in cfg.config_set

    if cfg.show_tests:
        extras["no_test"] = relpath in cfg.untested_set

    if cfg.show_outline:
        outline = extract_outline(text, ext)
        if outline and cfg.public_only:
            outline = [item for item in outline if item[2]]
        extras["outline"] = outline

    if cfg.show_imports:
        extras["imports"] = cfg.imports_map.get(relpath, [])
        extras["imported_by"] = cfg.imported_by_map.get(relpath, [])
        extras["external_imports"] = cfg.external_map.get(relpath, [])

    return extras


# ════════════════════════════════════════════════════════════


# ─── 読み始め候補 ─────────────────────────────────────────────
def reading_order_candidates(cfg, top_n, limit):
    """「読み始めの候補」を返す（エントリーポイント→依存度の高い順）。
    JSON・テキスト両出力で共有する。top_n: 依存度上位から補充する件数、limit: 最終件数上限。
    """
    cand = sorted(cfg.entry_set)
    if cfg.imported_by_map:
        for p, _ in sorted(cfg.imported_by_map.items(), key=lambda kv: -len(kv[1]))[:top_n]:
            if p not in cand:
                cand.append(p)
    return cand[:limit]


# ─── 設定クラス ───────────────────────────────────────────────
class Cfg:
    def __init__(self, args, root):
        self.root          = root
        self.max_depth     = args.depth
        self.show_all      = args.all
        self.by_size       = args.sort_size    # -S
        self.sort_mtime    = args.sort_mtime   # -t (tree)
        self.sort_ctime    = args.sort_ctime   # -c (tree)
        self.show_date     = args.date
        self.use_gitignore = args.gitignore    # -G
        self.show_bar      = args.bar
        self.min_size      = parse_size(args.min_size) if args.min_size else None
        self.max_size      = parse_size(args.max_size) if args.max_size else None
        self.excludes      = args.exclude or []
        self.includes      = args.include or []
        self.show_emoji    = args.emoji
        self.type_ext      = ("." + args.type.lstrip(".")).lower() if args.type else None  # -e
        self.show_perms    = args.perms
        self.show_user     = args.user
        self.show_group    = args.show_group   # -g (tree)
        self.dirs_only     = args.dirs_only    # -d (tree)
        self.follow_syms   = args.follow
        self.full_path     = args.full_path
        self.prune         = args.prune
        self.reverse       = args.reverse
        self.files_first   = args.filesfirst

        # AI/エージェント向け解析フラグ
        self.show_tokens   = args.tokens       # -T
        self.show_git      = args.git          # -H
        self.show_todo     = args.todo         # -K
        self.show_tests    = args.tests        # -V
        self.show_entry    = args.entry        # -N
        self.show_outline  = args.outline      # -O
        self.show_imports  = args.imports      # -M
        self.show_config   = args.config       # -F
        self.public_only   = args.api          # -A（-Oと併用時のみ意味を持つ）
        self.has_extras    = any([self.show_tokens, self.show_git, self.show_todo,
                                   self.show_tests, self.show_entry, self.show_outline,
                                   self.show_imports, self.show_config])
        # main() 側で必要に応じて埋める
        self.git_map          = {}
        self.git_change_counts = {}
        self.untested_set     = set()
        self.entry_set        = set()
        self.config_set       = set()
        self.imports_map      = {}
        self.imported_by_map  = {}
        self.external_map     = {}
        self.cycles           = []


# ─── 共通フィルタリング ───────────────────────────────────────
def _is_dir_follow(e):
    """DirEntry.is_dir(follow_symlinks=True) の安全版。
    Windows では壊れたsymlinkで FileNotFoundError 以外の OSError
    （WinError 123 等）が伝播してくることがあるため握りつぶす。"""
    try:
        return e.is_dir(follow_symlinks=True)
    except OSError:
        return False


def _filter(path, cfg, active_pats):
    try: raw = list(os.scandir(path))
    except OSError: return None, None  # アクセス拒否・走査中の削除・ELOOP等のシグナル

    entries = [e for e in raw if cfg.show_all or not e.name.startswith(".")]

    if active_pats:
        entries = [e for e in entries
                   if not is_ignored(e.name,
                                     os.path.relpath(e.path, cfg.root),
                                     e.is_dir(follow_symlinks=False),
                                     active_pats)]

    dirs  = [e for e in entries if e.is_dir(follow_symlinks=False)]

    if cfg.follow_syms:
        sym_dirs = [e for e in entries
                    if e.is_symlink() and not e.is_dir(follow_symlinks=False)
                    and _is_dir_follow(e)]
        dirs = dirs + sym_dirs

    if cfg.dirs_only:
        files = []
    else:
        files = [e for e in entries if not e.is_dir(follow_symlinks=False)
                 and not (cfg.follow_syms and e.is_symlink() and _is_dir_follow(e))]

    if cfg.excludes:
        dirs  = [d for d in dirs  if not any(fnmatch.fnmatch(d.name, p) for p in cfg.excludes)]
        files = [f for f in files if not any(fnmatch.fnmatch(f.name, p) for p in cfg.excludes)]

    if cfg.includes:
        files = [f for f in files if any(fnmatch.fnmatch(f.name, p) for p in cfg.includes)]

    if cfg.type_ext:
        files = [f for f in files if os.path.splitext(f.name)[1].lower() == cfg.type_ext]

    if cfg.min_size is not None or cfg.max_size is not None:
        def in_range(e):
            try: sz = e.stat(follow_symlinks=True).st_size
            except OSError: return True
            if cfg.min_size is not None and sz < cfg.min_size: return False
            if cfg.max_size is not None and sz > cfg.max_size: return False
            return True
        files = [f for f in files if in_range(f)]

    return dirs, files

def count_entries(path, cfg, active_pats):
    pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, pats)
    if dirs is None: return 0, 0, True   # アクセス拒否
    return len(dirs), len(files), False

def _has_content(path, depth, cfg, active_pats):
    if cfg.max_depth is not None and depth >= cfg.max_depth: return False
    pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, pats)
    if dirs is None: return False
    if files: return True
    for d in dirs:
        if _has_content(d.path, depth + 1, cfg, pats): return True
    return False


# ─── ソートヘルパー ───────────────────────────────────────────
def _sort_entries(dirs, files, cfg):
    def emtime(e):
        try: return e.stat(follow_symlinks=True).st_mtime
        except OSError: return 0
    def ectime(e):
        try: return e.stat(follow_symlinks=True).st_ctime
        except OSError: return 0
    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0

    rev = cfg.reverse
    if cfg.sort_mtime:
        dirs.sort(key=emtime,  reverse=not rev)
        files.sort(key=emtime, reverse=not rev)
    elif cfg.sort_ctime:
        dirs.sort(key=ectime,  reverse=not rev)
        files.sort(key=ectime, reverse=not rev)
    elif cfg.by_size:
        dirs.sort(key=lambda e: dir_size(e.path)[0], reverse=not rev)
        files.sort(key=esz, reverse=not rev)
    else:
        dirs.sort(key=lambda e: e.name.casefold(), reverse=rev)
        files.sort(key=lambda e: e.name.casefold(), reverse=rev)

    return dirs, files


# ─── ツリー描画 ───────────────────────────────────────────────
PIPE = "│   "; FORK = "├── "; LAST = "└── "; BLANK = "    "

def render(path, prefix, depth, cfg, stats, active_pats, _seen=None):
    if cfg.max_depth is not None and depth >= cfg.max_depth: return

    if cfg.follow_syms:
        if _seen is None: _seen = set()
        real = os.path.realpath(path)
        if real in _seen:
            print(f"{prefix}{LAST}{c('[循環リンク]', DIM)}")
            return
        _seen = _seen | {real}

    cur_pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, cur_pats)
    if dirs is None:
        print(f"{prefix}{LAST}{c('[アクセス拒否]', BOLD, RED)}")
        return

    if cfg.prune:
        dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, cur_pats)]

    dirs, files = _sort_entries(dirs, files, cfg)
    combined = (files + dirs) if cfg.files_first else (dirs + files)
    cur_dir_size, _ = dir_size(path)

    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0
    def emtime(e):
        try: return e.stat(follow_symlinks=True).st_mtime
        except OSError: return 0

    for i, entry in enumerate(combined):
        is_last = (i == len(combined) - 1)
        branch = LAST if is_last else FORK
        cont   = BLANK if is_last else PIPE

        if entry.is_symlink():
            try:    sym_target = f" → {os.readlink(entry.path)}"
            except OSError: sym_target = " →"
        else:
            sym_target = ""

        display = ("./" + os.path.relpath(entry.path, cfg.root).replace("\\", "/")) \
                  if cfg.full_path else entry.name

        perm_prefix = fmt_perm_info(entry, cfg)

        is_dir_entry = entry.is_dir(follow_symlinks=False) or \
                       (cfg.follow_syms and entry.is_symlink() and _is_dir_follow(entry))

        if is_dir_entry:
            sz, sz_err     = dir_size(entry.path)
            nd, nf, denied = count_entries(entry.path, cfg, cur_pats)
            stats["dirs"] += 1

            emoji = (get_emoji(entry.name, is_dir=True) + " ") if cfg.show_emoji else ""
            parts = [fmt_count(nd, nf, denied), fmt_size(sz, sz_err)]
            if cfg.show_date:
                try: parts.append(fmt_date(entry.stat(follow_symlinks=False).st_mtime))
                except OSError: pass
            bar = (" " + fmt_bar(sz, cur_dir_size)) if cfg.show_bar and cur_dir_size else ""

            name = c(f"{emoji}{display}{sym_target}/", BOLD, CYAN)
            meta = c(f"({', '.join(parts)}){bar}", DIM)
            print(f"{prefix}{branch}{perm_prefix}{name} {meta}")
            render(entry.path, prefix + cont, depth + 1, cfg, stats, cur_pats, _seen)

        else:
            sz  = esz(entry)
            stats["files"] += 1
            ext = os.path.splitext(entry.name)[1].lower()
            stats["extensions"][ext or "(no ext)"] = \
                stats["extensions"].get(ext or "(no ext)", 0) + 1

            rel = os.path.relpath(entry.path, cfg.root).replace("\\", "/")
            extras = _file_extras(entry, rel, cfg) if cfg.has_extras else {}

            entry_mark = ""
            if extras.get("is_entry"):
                entry_mark = "🎯 " if cfg.show_emoji else "* "

            config_mark = ""
            if extras.get("is_config"):
                config_mark = "⚙ " if not entry_mark else ""

            emoji = (get_emoji(entry.name) + " ") if cfg.show_emoji else ""
            parts = [fmt_size(sz)]
            if cfg.show_date:
                mt = emtime(entry)
                if mt: parts.append(fmt_date(mt))

            if cfg.show_tokens and extras.get("tokens") is not None:
                parts.append(fmt_tokens(extras["tokens"]))
                stats["tokens"] += extras["tokens"]
                if extras.get("lines") is not None:
                    parts.append(f"{extras['lines']} lines")

            if cfg.show_git and extras.get("git"):
                parts.append(fmt_git(extras["git"]))

            if cfg.show_todo and extras.get("todos"):
                n_todo = len(extras["todos"])
                parts.append(f"TODO×{n_todo}")
                stats["todo_total"] += n_todo
                for item in extras["todos"][:3]:
                    if len(stats["todo_samples"]) < 20:
                        stats["todo_samples"].append((rel, *item))

            if cfg.show_tests and extras.get("no_test"):
                parts.append("テスト無し")

            if cfg.show_config and extras.get("is_config"):
                parts.append("config")

            if cfg.show_outline and extras.get("outline"):
                ostr = fmt_outline(extras["outline"])
                if ostr: parts.append(ostr)

            if cfg.show_imports:
                imp_n  = len(extras.get("imports") or [])
                used_n = len(extras.get("imported_by") or [])
                if imp_n:  parts.append(f"imports×{imp_n}")
                if used_n: parts.append(f"used-by×{used_n}")

            bar = (" " + fmt_bar(sz, cur_dir_size)) if cfg.show_bar and cur_dir_size else ""

            name = c(f"{entry_mark}{config_mark}{display}{sym_target}",
                     MAGENTA if entry.is_symlink() else GREEN)
            meta = c(f"({', '.join(parts)}){bar}", DIM)
            print(f"{prefix}{branch}{perm_prefix}{name} {meta}")


# ─── JSON出力 ─────────────────────────────────────────────────
def build_json_tree(path, depth, cfg, active_pats, stats=None):
    if stats is None:
        stats = {"tokens": 0, "todo_total": 0, "todo_samples": [], "no_test": 0, "entries": 0, "configs": 0}

    cur_pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, cur_pats)
    denied = dirs is None
    if denied: dirs, files = [], []
    if cfg.prune:
        dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, cur_pats)]
    dirs, files = _sort_entries(dirs, files, cfg)
    sz, sz_err = dir_size(path)

    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0

    children = []
    if cfg.max_depth is None or depth < cfg.max_depth:
        combined = (files + dirs) if cfg.files_first else (dirs + files)
        for entry in combined:
            if entry.is_dir(follow_symlinks=False):
                children.append(build_json_tree(entry.path, depth + 1, cfg, cur_pats, stats))
            else:
                f_sz = esz(entry)
                rel = os.path.relpath(entry.path, cfg.root).replace("\\", "/")
                extras = _file_extras(entry, rel, cfg) if cfg.has_extras else {}

                file_obj = {
                    "name": entry.name, "type": "file",
                    "size": f_sz, "size_human": fmt_size(f_sz),
                    "ext": os.path.splitext(entry.name)[1].lower(),
                    "path": rel,
                }
                if cfg.show_tokens:
                    file_obj["tokens"] = extras.get("tokens")
                    file_obj["lines"] = extras.get("lines")
                    if extras.get("tokens") is not None:
                        stats["tokens"] += extras["tokens"]
                if cfg.show_git:
                    file_obj["git"] = extras.get("git")
                if cfg.show_todo:
                    todos = extras.get("todos") or []
                    file_obj["todos"] = [{"line": ln, "kind": k, "text": s} for ln, k, s in todos]
                    stats["todo_total"] += len(todos)
                    for item in todos[:3]:
                        if len(stats["todo_samples"]) < 20:
                            stats["todo_samples"].append((rel, *item))
                if cfg.show_tests:
                    file_obj["has_test"] = not extras.get("no_test", False)
                if cfg.show_entry:
                    file_obj["is_entry"] = bool(extras.get("is_entry"))
                if cfg.show_config:
                    file_obj["is_config"] = bool(extras.get("is_config"))
                if cfg.show_outline:
                    outline = extras.get("outline")
                    file_obj["outline"] = ([{"kind": k, "name": n, "public": p} for k, n, p in outline]
                                           if outline else outline)  # None=対応外言語
                if cfg.show_imports:
                    file_obj["imports"] = extras.get("imports") or []
                    file_obj["imported_by"] = extras.get("imported_by") or []
                    file_obj["external_imports"] = extras.get("external_imports") or []

                children.append(file_obj)

    name = os.path.basename(path) or path
    return {
        "name": name, "type": "directory",
        "size": sz, "size_human": fmt_size(sz, sz_err),
        "path": os.path.relpath(path, cfg.root) if path != cfg.root else ".",
        "item_count": {"dirs": len(dirs), "files": len(files),
                       "permission_denied": denied},
        "children": children,
    }


# ─── HTML出力 ─────────────────────────────────────────────────
def generate_html(root_path, cfg, active_pats):
    def _node(path, depth, cur_pats):
        pats = _extend_pats(cur_pats, path, cfg)
        dirs, files = _filter(path, cfg, pats)
        denied = dirs is None
        if denied: dirs, files = [], []
        if cfg.prune:
            dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, pats)]
        dirs, files = _sort_entries(dirs, files, cfg)
        sz, sz_err = dir_size(path)
        name = os.path.basename(path) or path

        def esz(e):
            try: return e.stat(follow_symlinks=True).st_size
            except OSError: return 0

        combined = (files + dirs) if cfg.files_first else (dirs + files)
        ch = ""
        for entry in combined:
            if entry.is_dir(follow_symlinks=False):
                if cfg.max_depth is None or depth < cfg.max_depth:
                    ch += _node(entry.path, depth + 1, pats)
                else:
                    e_sz, e_err = dir_size(entry.path)
                    ch += (f'<div class="item dir-leaf">📁 {html.escape(entry.name)}/'
                           f' <span class="sz">{fmt_size(e_sz, e_err)}</span></div>\n')
            else:
                f_sz = esz(entry)
                sym = ""
                if entry.is_symlink():
                    try: sym = f' → {html.escape(os.readlink(entry.path))}'
                    except OSError: sym = ' →'

                rel = os.path.relpath(entry.path, cfg.root).replace("\\", "/")
                extras = _file_extras(entry, rel, cfg) if cfg.has_extras else {}
                badges = ""
                if extras.get("is_entry"):
                    badges += '<span class="badge entry">entry</span>'
                if extras.get("no_test"):
                    badges += '<span class="badge notest">no test</span>'
                if extras.get("todos"):
                    badges += f'<span class="badge todo">TODO×{len(extras["todos"])}</span>'

                ch += (f'<div class="item file">'
                       f'<span class="emoji">{get_emoji(entry.name)}</span>'
                       f'<span class="fname"> {html.escape(entry.name)}{sym}</span>'
                       f'<span class="sz"> {fmt_size(f_sz)}</span>{badges}</div>\n')

        nd, nf = len(dirs), len(files)
        opened = " open" if depth == 0 else ""
        return (f'<details{opened}><summary>📁 <strong>{html.escape(name)}/</strong>'
                f' <span class="sz">({fmt_count(nd, nf, denied)}, {fmt_size(sz, sz_err)})</span>'
                f'</summary><div class="ch">{ch}</div></details>\n')

    root_name = html.escape(os.path.basename(root_path) or root_path)
    tree = _node(root_path, 0, active_pats)
    return f'''<!DOCTYPE html>
<html lang="ja"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>dirlens — {root_name}</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:Menlo,Consolas,monospace;font-size:14px;background:#1e1e2e;color:#cdd6f4;padding:24px}}
h1{{color:#89b4fa;margin-bottom:12px;font-size:18px}}
#q{{background:#313244;border:1px solid #45475a;color:#cdd6f4;padding:6px 12px;
    border-radius:6px;font-size:13px;margin-bottom:16px;width:280px;outline:none}}
#q:focus{{border-color:#89b4fa}}
details{{margin-left:18px}}
summary{{cursor:pointer;padding:2px 6px;border-radius:4px;list-style:none;
         white-space:nowrap;color:#89dceb}}
summary::-webkit-details-marker{{display:none}}
summary::before{{content:"▶ ";font-size:10px;opacity:.4}}
details[open]>summary::before{{content:"▼ "}}
summary:hover{{background:rgba(255,255,255,.06)}}
.ch{{border-left:1px solid rgba(255,255,255,.08);margin-left:10px}}
.item{{padding:2px 6px;white-space:nowrap;margin-left:18px}}
.item:hover{{background:rgba(255,255,255,.05);border-radius:4px}}
.fname{{color:#a6e3a1}}.sz{{color:#585b70;font-size:12px}}
.emoji{{width:1.6em;display:inline-block}}.hidden{{display:none!important}}
.badge{{display:inline-block;margin-left:6px;padding:0 6px;border-radius:8px;
        font-size:10px;vertical-align:middle}}
.badge.entry{{background:#89b4fa;color:#1e1e2e}}
.badge.notest{{background:#f9e2af;color:#1e1e2e}}
.badge.todo{{background:#f38ba8;color:#1e1e2e}}
</style></head><body>
<h1>🌳 dirlens — {root_name}</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">{tree}</div>
<script>
function search(q){{
  q=q.toLowerCase().trim();
  document.querySelectorAll('.hidden').forEach(el=>el.classList.remove('hidden'));
  if(!q) return;
  document.querySelectorAll('details').forEach(d=>d.open=true);
  document.querySelectorAll('.file').forEach(el=>{{
    const n=el.querySelector('.fname')?.textContent.toLowerCase()||'';
    if(!n.includes(q)) el.classList.add('hidden');
  }});
  document.querySelectorAll('#tree details').forEach(detail=>{{
    const hasVisible=[...detail.querySelectorAll('.file')]
      .some(f=>!f.classList.contains('hidden'));
    if(!hasVisible) detail.classList.add('hidden');
  }});
}}
</script></body></html>'''


# ─── エントリポイント ─────────────────────────────────────────
def main():
    global USE_COLOR
    sys.setrecursionlimit(10_000)

    ap = argparse.ArgumentParser(
        prog="dirlens",
        description="ファイルサイズ付きのディレクトリツリーを表示します",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "使用例:\n"
            "  dirlens --ai             AIチャット貼り付け用（人間がコピペする想定）\n"
            "  dirlens --agent          エージェント向け解析（カラーなし・クリップボードは使わない）\n"
            "  dirlens -d               ディレクトリのみ表示（tree -d 互換）\n"
            "  dirlens -L 2             深さ 2 まで表示（tree -L 互換）\n"
            "  dirlens -G --prune       gitignore除外 + 空枝を剪定\n"
            "  dirlens -T               ファイルごとの推定トークン数を表示\n"
            "  dirlens -H               最終コミット情報を表示（要git）\n"
            "  dirlens -K               TODO/FIXME/HACKを抽出\n"
            "  dirlens -V               テストが無いソースファイルを表示\n"
            "  dirlens -N               エントリーポイントらしきファイルをマーク\n"
            "  dirlens -O               関数・クラスの簡易アウトラインを表示\n"
            "  dirlens -M               ローカルなimport/依存関係を解析\n"
            "  dirlens --no-color > dirlens.txt   ファイルに書き出す"
        ),
    )

    # ── tree互換フラグ（5つ） ─────────────────────────────────
    ap.add_argument("-d",                action="store_true", dest="dirs_only",
                    help="ディレクトリのみ表示（tree -d 互換）")
    ap.add_argument("-g",                action="store_true", dest="show_group",
                    help="グループ名を表示（tree -g 互換）")
    ap.add_argument("-s",                action="store_true", dest="show_size_compat",
                    help="サイズ表示（常時有効・tree -s 互換）")
    ap.add_argument("-t",                action="store_true", dest="sort_mtime",
                    help="更新日時順にソート（tree -t 互換）")
    ap.add_argument("-c",                action="store_true", dest="sort_ctime",
                    help="ステータス変更日時順にソート（tree -c 互換）")

    # ── dirlens独自フラグ（tree互換でないもの） ───────────────
    ap.add_argument("-G", "--gitignore", action="store_true",
                    help=".gitignoreのファイルを除外（旧 -g）")
    ap.add_argument("-S", "--sort-size", action="store_true",
                    help="サイズ順にソート（旧 -s）")
    ap.add_argument("-e", "--type",      metavar="EXT",
                    help="指定した拡張子のみ表示（旧 -t）")
    ap.add_argument("-C", "--copy",      action="store_true",
                    help="クリップボードにコピー（旧 -c）")

    # ── tree互換フラグ（変更なし） ────────────────────────────
    ap.add_argument("-a", "--all",       action="store_true")
    ap.add_argument("-f", "--full-path", action="store_true", dest="full_path")
    ap.add_argument("-l", "--follow",    action="store_true")
    ap.add_argument("-p", "--perms",     action="store_true")
    ap.add_argument("-u", "--user",      action="store_true")
    ap.add_argument("-r", "--reverse",   action="store_true")
    ap.add_argument("-n", dest="no_color_tree", action="store_true",
                    help="カラーなし（tree -n 互換）")
    ap.add_argument("-J", dest="json_tree", action="store_true",
                    help="JSON形式で出力（tree -J 互換）")
    ap.add_argument("-L", dest="level",  type=int,  metavar="N",
                    help="表示する最大の深さ（tree -L 互換）")
    ap.add_argument("-D", dest="date_tree", action="store_true",
                    help="最終更新日時を表示（tree -D 互換）")
    ap.add_argument("-P", dest="include_tree", metavar="PATTERN", action="append",
                    help="このパターンのみ表示（tree -P 互換）")
    ap.add_argument("-I", dest="exclude_tree", metavar="PATTERN", action="append",
                    help="除外パターン（tree -I 互換）")

    # ── AI/エージェント向け解析フラグ ─────────────────────────
    ap.add_argument("-T", "--tokens",    action="store_true",
                    help="ファイルごとの推定トークン数を表示（概算）")
    ap.add_argument("-H", "--git",       action="store_true",
                    help="最終コミット情報を表示（要git、直近2000コミットまで走査）")
    ap.add_argument("-K", "--todo",      action="store_true",
                    help="TODO/FIXME/HACK/XXXコメントを抽出")
    ap.add_argument("-V", "--missing-tests", action="store_true", dest="tests",
                    help="対応するテストファイルが見つからないソースファイルを表示")
    ap.add_argument("-N", "--entry",     action="store_true",
                    help="エントリーポイントらしきファイルを検出してマーク")
    ap.add_argument("-O", "--outline",   action="store_true",
                    help="関数・クラスの簡易アウトラインを表示（正規表現ベース・対応言語限定）")
    ap.add_argument("-M", "--imports",   action="store_true",
                    help="ローカルなimport/依存関係を解析して表示（Python/JS/TS/Go/Rust対応、"
                         "正確さは言語による。外部パッケージは対象外）。循環依存も併せて検出")
    ap.add_argument("-A", "--api",       action="store_true",
                    help="公開API（exportされたシンボル）のみに絞り込む（-O を自動的に有効化）")
    ap.add_argument("-F", "--config",    action="store_true",
                    help="設定ファイル（.env, tsconfig.json, pyproject.toml等）を検出してマーク")

    # ── dirlens独自オプション ─────────────────────────────────
    ap.add_argument("path",              nargs="?", default=".")
    ap.add_argument("--depth",           type=int,  metavar="N",
                    help="表示する最大の深さ（-L と同じ）")
    ap.add_argument("--date",            action="store_true")
    ap.add_argument("-m", "--markdown",  action="store_true")
    ap.add_argument("--no-color",        action="store_true")
    ap.add_argument("--bar",             action="store_true")
    ap.add_argument("--min-size",        metavar="SIZE")
    ap.add_argument("--max-size",        metavar="SIZE")
    ap.add_argument("--exclude",         metavar="PATTERN", action="append")
    ap.add_argument("--include",         metavar="PATTERN", action="append")
    ap.add_argument("--emoji",           action="store_true")
    ap.add_argument("--json",            action="store_true")
    ap.add_argument("--html",            nargs="?", const="dirlens.html", metavar="FILE")
    ap.add_argument("--prune",           action="store_true")
    ap.add_argument("--filesfirst",      action="store_true")
    ap.add_argument("--ai",              action="store_true",
                    help="-G --date -m -C のショートカット（人間がAIチャットに貼り付ける用）")
    ap.add_argument("--agent",           action="store_true",
                    help="-G -T -H -K -V -N -O -M -F --no-color のショートカット（エージェント向け解析、カラーなし・クリップボードは使わない）")
    args = ap.parse_args()

    # ── エイリアスのマージ ────────────────────────────────────
    if args.level      is not None: args.depth    = args.level
    if args.date_tree:              args.date     = True
    if args.include_tree:           args.include  = (args.include or []) + args.include_tree
    if args.exclude_tree:           args.exclude  = (args.exclude or []) + args.exclude_tree
    if args.no_color_tree:          args.no_color = True
    if args.json_tree:              args.json     = True

    # --ai: 人間がAIチャットに貼り付けるためのショートカット（クリップボードを使う）
    if args.ai:
        args.gitignore = True
        args.date      = True
        args.markdown  = True
        args.copy      = True

    # --agent: エージェントが自律実行しても安全なショートカット（クリップボードは使わない）
    # エージェント出力／ログとして扱う前提のため、ANSIカラーも自動で無効化する
    # （--agent 単体で --no-color を兼ねる）。
    if args.agent:
        args.gitignore = True
        args.date      = True
        args.tokens    = True
        args.git       = True
        args.todo      = True
        args.tests     = True
        args.entry     = True
        args.outline   = True
        args.imports   = True
        args.config    = True
        args.no_color  = True

    # -A（公開APIのみ）は -O（アウトライン）を自動的に有効化する
    if args.api:
        args.outline = True

    if args.no_color or args.markdown or args.json:
        USE_COLOR = False

    try:
        target = Path(args.path).resolve()
    except PermissionError:
        # カレントディレクトリ自体がサンドボックス等でアクセス不能な場合、
        # Path.resolve() 内部の os.getcwd() がここで例外を出す。
        print(f"エラー: 現在のディレクトリへのアクセス権限がありません。", file=sys.stderr)
        print(f"絶対パスを明示的に指定してください（例: dirlens /path/to/project）。", file=sys.stderr)
        sys.exit(1)
    if not target.exists():
        print(f"エラー: '{args.path}' が見つかりません", file=sys.stderr); sys.exit(1)
    if not target.is_dir():
        print(f"エラー: '{args.path}' はディレクトリではありません", file=sys.stderr); sys.exit(1)

    cfg = Cfg(args, str(target))
    active_pats = load_gitignore(str(target)) if args.gitignore else []

    if cfg.show_tests or cfg.show_entry or cfg.show_config or cfg.show_imports:
        (cfg.untested_set, cfg.entry_set, cfg.config_set,
         cfg.imports_map, cfg.imported_by_map, cfg.external_map, cfg.cycles) = \
            build_project_index(str(target), cfg, active_pats)
    if cfg.show_git:
        cfg.git_map, cfg.git_change_counts = load_git_log(str(target))

    _prefetch_sizes(str(target))

    # ── JSON ─────────────────────────────────────────────────
    if args.json:
        stats = {"tokens": 0, "todo_total": 0, "todo_samples": [], "no_test": 0, "entries": 0, "configs": 0}
        tree = build_json_tree(str(target), 0, cfg, active_pats, stats)
        if cfg.has_extras:
            most_depended = None
            if cfg.show_imports and cfg.imported_by_map:
                top = sorted(cfg.imported_by_map.items(), key=lambda kv: -len(kv[1]))[:10]
                most_depended = [{"path": p, "used_by_count": len(v)} for p, v in top]

            hotspots = None
            if cfg.show_git and cfg.git_change_counts:
                top_hot = sorted(cfg.git_change_counts.items(), key=lambda kv: -kv[1])[:10]
                hotspots = [{"path": p, "change_count": n} for p, n in top_hot]

            reading_order = None
            if cfg.show_entry and cfg.show_imports and (cfg.entry_set or cfg.imported_by_map):
                reading_order = reading_order_candidates(cfg, top_n=5, limit=8)

            tree["project_summary"] = {
                "estimated_tokens": stats["tokens"] if cfg.show_tokens else None,
                "todo_count": stats["todo_total"] if cfg.show_todo else None,
                "missing_tests_count": len(cfg.untested_set) if cfg.show_tests else None,
                "entry_points_count": len(cfg.entry_set) if cfg.show_entry else None,
                "config_files_count": len(cfg.config_set) if cfg.show_config else None,
                "git_available": bool(cfg.git_map) if cfg.show_git else None,
                "most_depended_on": most_depended,
                "hotspots": hotspots,
                "circular_dependencies": (
                    list(cfg.cycles) if cfg.show_imports and cfg.cycles else
                    ([] if cfg.show_imports else None)
                ),
                "reading_order_candidates": reading_order,
            }
        print(json.dumps(tree, ensure_ascii=False, indent=2)); return

    # ── HTML ─────────────────────────────────────────────────
    if args.html:
        out = Path(args.html)
        out.write_text(generate_html(str(target), cfg, active_pats), encoding="utf-8")
        print(f"✓ {out} を生成しました ({fmt_size(out.stat().st_size)})"); return

    # ── テキスト出力 ─────────────────────────────────────────
    if args.copy:
        _buf = io.StringIO(); _old = sys.stdout; sys.stdout = _buf

    if args.markdown: print("```")

    root_sz,  root_sz_err          = dir_size(str(target))
    root_nd, root_nf, root_denied  = count_entries(str(target), cfg, active_pats)
    root_label = target.name if target.name else str(target)

    root_parts = [fmt_count(root_nd, root_nf, root_denied), fmt_size(root_sz, root_sz_err)]
    if args.date:
        try: root_parts.append(fmt_date(target.stat().st_mtime))
        except OSError: pass

    root_emoji = (get_emoji(root_label, is_dir=True) + " ") if args.emoji else ""
    print(f"{c(root_emoji + root_label + '/', BOLD, BLUE)} "
          f"{c('(' + ', '.join(root_parts) + ')', DIM)}")

    stats = {"files": 0, "dirs": 0, "extensions": {},
             "tokens": 0, "todo_total": 0, "todo_samples": [], "no_test": 0, "entries": 0,
             "configs": 0}
    render(str(target), "", 0, cfg, stats, active_pats)

    print()
    summary = f"  合計  {stats['dirs']} ディレクトリ"
    if not cfg.dirs_only:
        summary += f",  {stats['files']} ファイル"
    if args.gitignore: summary += "  (.gitignore 適用済み)"
    if cfg.type_ext:   summary += f"  (フィルタ: {cfg.type_ext})"
    if cfg.excludes:   summary += f"  (除外: {', '.join(cfg.excludes)})"
    if cfg.includes:   summary += f"  (抽出: {', '.join(cfg.includes)})"
    if cfg.min_size:   summary += f"  (最小: {fmt_size(cfg.min_size)})"
    if cfg.max_size:   summary += f"  (最大: {fmt_size(cfg.max_size)})"
    if cfg.prune:      summary += "  (剪定済み)"
    if cfg.dirs_only:  summary += "  (ディレクトリのみ)"
    print(c(summary, DIM))

    if not cfg.dirs_only and stats["extensions"]:
        exts = sorted(stats["extensions"].items(), key=lambda x: -x[1])
        print(c("  " + "  ".join(f"{e} ×{n}" for e, n in exts[:8]), DIM))

    if cfg.show_tokens:
        print(c(f"  推定トークン数: {fmt_tokens(stats['tokens'])}", DIM))

    if cfg.show_todo:
        if stats["todo_total"]:
            print(c(f"  TODO/FIXME等: {stats['todo_total']}件", DIM))
            for rel, ln, kind, snippet in stats["todo_samples"][:8]:
                print(c(f"    {rel}:{ln} [{kind}] {snippet}", DIM))
            if stats["todo_total"] > min(len(stats["todo_samples"]), 8):
                rest = stats["todo_total"] - min(len(stats["todo_samples"]), 8)
                print(c(f"    …他 {rest} 件", DIM))
        else:
            print(c("  TODO/FIXME等: 0件", DIM))

    if cfg.show_tests:
        # -L等で表示が深さ制限されていても、テスト欠落検知はプロジェクト全体のスキャン結果。
        print(c(f"  テスト未整備: {len(cfg.untested_set)} ファイル", DIM))

    if cfg.show_entry:
        print(c(f"  エントリーポイント候補: {len(cfg.entry_set)} 件検出", DIM))

    if cfg.show_config:
        print(c(f"  設定ファイル: {len(cfg.config_set)} 件検出", DIM))

    if cfg.show_imports and cfg.imported_by_map:
        top = sorted(cfg.imported_by_map.items(), key=lambda kv: -len(kv[1]))[:5]
        print(c("  依存度が高いファイル（多くのファイルから参照されている）:", DIM))
        for relpath, importers in top:
            print(c(f"    {relpath}  (used by {len(importers)})", DIM))

    if cfg.show_imports and cfg.cycles:
        print(c(f"  循環依存: {len(cfg.cycles)} 件検出", DIM))
        for cycle in cfg.cycles[:5]:
            print(c(f"    {' → '.join(cycle)}", DIM))
        if len(cfg.cycles) > 5:
            print(c(f"    …他 {len(cfg.cycles) - 5} 件", DIM))

    if cfg.show_git and cfg.git_change_counts:
        top_hot = sorted(cfg.git_change_counts.items(), key=lambda kv: -kv[1])[:5]
        if top_hot and top_hot[0][1] > 1:  # 1回しか変更されていないなら出す価値が薄い
            print(c("  変更頻度が高いファイル（直近の履歴内）:", DIM))
            for relpath, n in top_hot:
                print(c(f"    {relpath}  ({n} 回変更)", DIM))

    if cfg.show_entry and cfg.show_imports and (cfg.entry_set or cfg.imported_by_map):
        candidates = reading_order_candidates(cfg, top_n=3, limit=5)
        if candidates:
            print(c("  読み始めの候補（エントリーポイント→依存度の高い順）:", DIM))
            for i, p in enumerate(candidates, 1):
                print(c(f"    {i}. {p}", DIM))

    if cfg.show_git and not cfg.git_map:
        print(c("  (gitリポジトリではないか、git未インストールのためコミット情報は取得できませんでした)", DIM))

    if args.markdown: print("```")

    if args.copy:
        sys.stdout = _old
        text = _buf.getvalue()
        print(text, end="")
        ok = copy_to_clipboard(strip_ansi(text))
        print(c("✓ クリップボードにコピーしました" if ok
                else "✗ コピー失敗 (pbcopy / xclip / wl-copy が必要)",
                BOLD, GREEN if ok else DIM), file=sys.stderr)


if __name__ == "__main__":
    main()
