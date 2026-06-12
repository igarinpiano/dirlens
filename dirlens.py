#!/usr/bin/env python3
"""
dirlens – ファイルサイズ付きディレクトリツリー表示ツール
対応環境: macOS / Linux / Windows  (Python 3.8+)
"""

import io, json, os, sys, stat as _stat, argparse, fnmatch, datetime, subprocess
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

def c(text, *codes):
    return ("".join(codes) + text + RESET) if USE_COLOR else text


# ─── フォーマット ─────────────────────────────────────────────
def fmt_size(n):
    if n == 0: return "0 bytes"
    for unit, f in (("TB",1<<40),("GB",1<<30),("MB",1<<20),("KB",1<<10)):
        if n >= f:
            return f"{str(f'{n/f:.2f}').rstrip('0').rstrip('.')} {unit}"
    return f"{n} {'byte' if n==1 else 'bytes'}"

def fmt_count(nd, nf):
    return f"{nd} {'dir' if nd==1 else 'dirs'}, {nf} {'file' if nf==1 else 'files'}"

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

import re as _re
def strip_ansi(text):
    return _re.sub(r'\033\[[0-9;]*[mK]', '', text)


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
_sz_cache = {}

def dir_size(path):
    if path in _sz_cache: return _sz_cache[path]
    total = 0
    try:
        with os.scandir(path) as it:
            for e in it:
                try:
                    if e.is_file(follow_symlinks=False):
                        total += e.stat(follow_symlinks=False).st_size
                    elif e.is_dir(follow_symlinks=False):
                        total += dir_size(e.path)
                except OSError: pass
    except OSError: pass
    _sz_cache[path] = total
    return total

def _prefetch_sizes(root_path):
    try:
        top = [e.path for e in os.scandir(root_path)
               if e.is_dir(follow_symlinks=False)]
    except OSError: return
    if len(top) < 2: return
    workers = min(len(top), (os.cpu_count() or 1), 8)
    with ThreadPoolExecutor(max_workers=workers) as ex:
        list(ex.map(dir_size, top))


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


# ─── 共通フィルタリング ───────────────────────────────────────
def _filter(path, cfg, active_pats):
    try: raw = list(os.scandir(path))
    except PermissionError: return [], []

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
                    and e.is_dir(follow_symlinks=True)]
        dirs = dirs + sym_dirs

    # -d (dirs only): ファイルを非表示
    if cfg.dirs_only:
        files = []
    else:
        files = [e for e in entries if not e.is_dir(follow_symlinks=False)
                 and not (cfg.follow_syms and e.is_symlink() and e.is_dir(follow_symlinks=True))]

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
    return len(dirs), len(files)

def _has_content(path, depth, cfg, active_pats):
    if cfg.max_depth is not None and depth >= cfg.max_depth: return False
    pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, pats)
    if files: return True
    for d in dirs:
        if _has_content(d.path, depth + 1, cfg, pats): return True
    return False


# ─── ソートヘルパー ───────────────────────────────────────────
def _sort_entries(dirs, files, cfg):
    """設定に基づいてdirs/filesをソートする（-t/-c/-S/-r対応）。"""
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
    if cfg.sort_mtime:          # -t: 更新日時順（新しい順）
        dirs.sort(key=emtime,  reverse=not rev)
        files.sort(key=emtime, reverse=not rev)
    elif cfg.sort_ctime:         # -c: ctime順（新しい順）
        dirs.sort(key=ectime,  reverse=not rev)
        files.sort(key=ectime, reverse=not rev)
    elif cfg.by_size:            # -S: サイズ順（大きい順）
        dirs.sort(key=lambda e: dir_size(e.path), reverse=not rev)
        files.sort(key=esz, reverse=not rev)
    else:                        # デフォルト: アルファベット順
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

    if cfg.prune:
        dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, cur_pats)]

    dirs, files = _sort_entries(dirs, files, cfg)
    combined = (files + dirs) if cfg.files_first else (dirs + files)
    cur_dir_size = dir_size(path)

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
                       (cfg.follow_syms and entry.is_symlink() and entry.is_dir(follow_symlinks=True))

        if is_dir_entry:
            sz     = dir_size(entry.path)
            nd, nf = count_entries(entry.path, cfg, cur_pats)
            stats["dirs"] += 1

            emoji = (get_emoji(entry.name, is_dir=True) + " ") if cfg.show_emoji else ""
            parts = [fmt_count(nd, nf), fmt_size(sz)]
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

            emoji = (get_emoji(entry.name) + " ") if cfg.show_emoji else ""
            parts = [fmt_size(sz)]
            if cfg.show_date:
                mt = emtime(entry)
                if mt: parts.append(fmt_date(mt))
            bar = (" " + fmt_bar(sz, cur_dir_size)) if cfg.show_bar and cur_dir_size else ""

            name = c(f"{emoji}{display}{sym_target}",
                     MAGENTA if entry.is_symlink() else GREEN)
            meta = c(f"({', '.join(parts)}){bar}", DIM)
            print(f"{prefix}{branch}{perm_prefix}{name} {meta}")


# ─── JSON出力 ─────────────────────────────────────────────────
def build_json_tree(path, depth, cfg, active_pats):
    cur_pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, cur_pats)
    if cfg.prune:
        dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, cur_pats)]
    dirs, files = _sort_entries(dirs, files, cfg)
    sz = dir_size(path)

    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0

    children = []
    if cfg.max_depth is None or depth < cfg.max_depth:
        combined = (files + dirs) if cfg.files_first else (dirs + files)
        for entry in combined:
            if entry.is_dir(follow_symlinks=False):
                children.append(build_json_tree(entry.path, depth + 1, cfg, cur_pats))
            else:
                f_sz = esz(entry)
                children.append({
                    "name": entry.name, "type": "file",
                    "size": f_sz, "size_human": fmt_size(f_sz),
                    "ext": os.path.splitext(entry.name)[1].lower(),
                    "path": os.path.relpath(entry.path, cfg.root),
                })

    name = os.path.basename(path) or path
    return {
        "name": name, "type": "directory",
        "size": sz, "size_human": fmt_size(sz),
        "path": os.path.relpath(path, cfg.root) if path != cfg.root else ".",
        "item_count": {"dirs": len(dirs), "files": len(files)},
        "children": children,
    }


# ─── HTML出力 ─────────────────────────────────────────────────
def generate_html(root_path, cfg, active_pats):
    def _node(path, depth, cur_pats):
        pats = _extend_pats(cur_pats, path, cfg)
        dirs, files = _filter(path, cfg, pats)
        if cfg.prune:
            dirs = [d for d in dirs if _has_content(d.path, depth + 1, cfg, pats)]
        dirs, files = _sort_entries(dirs, files, cfg)
        sz = dir_size(path)
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
                    ch += (f'<div class="item dir-leaf">📁 {entry.name}/'
                           f' <span class="sz">{fmt_size(dir_size(entry.path))}</span></div>\n')
            else:
                f_sz = esz(entry)
                sym = ""
                if entry.is_symlink():
                    try: sym = f' → {os.readlink(entry.path)}'
                    except OSError: sym = ' →'
                ch += (f'<div class="item file">'
                       f'<span class="emoji">{get_emoji(entry.name)}</span>'
                       f'<span class="fname"> {entry.name}{sym}</span>'
                       f'<span class="sz"> {fmt_size(f_sz)}</span></div>\n')

        nd, nf = len(dirs), len(files)
        opened = " open" if depth == 0 else ""
        return (f'<details{opened}><summary>📁 <strong>{name}/</strong>'
                f' <span class="sz">({fmt_count(nd, nf)}, {fmt_size(sz)})</span>'
                f'</summary><div class="ch">{ch}</div></details>\n')

    root_name = os.path.basename(root_path) or root_path
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
</style></head><body>
<h1>🌳 dirlens — {root_name}</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">{tree}</div>
<script>
function search(q){{
  q=q.toLowerCase().trim();
  // まず全要素を表示に戻す
  document.querySelectorAll('.hidden').forEach(el=>el.classList.remove('hidden'));
  if(!q) return;
  // 全ディレクトリを展開
  document.querySelectorAll('details').forEach(d=>d.open=true);
  // マッチしないファイルを非表示
  document.querySelectorAll('.file').forEach(el=>{{
    const n=el.querySelector('.fname')?.textContent.toLowerCase()||'';
    if(!n.includes(q)) el.classList.add('hidden');
  }});
  // visible なファイルを1つも持たないディレクトリを非表示
  // （空ディレクトリ・検索に引っかからないディレクトリ両方を処理）
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
            "  dirlens --ai            AIチャット貼り付け用（推奨）\n"
            "  dirlens -d              ディレクトリのみ表示（tree -d 互換）\n"
            "  dirlens -L 2            深さ 2 まで表示（tree -L 互換）\n"
            "  dirlens -t              更新日時順にソート（tree -t 互換）\n"
            "  dirlens -G --prune      gitignore除外 + 空枝を剪定\n"
            "  dirlens -p -u -g        パーミッション・ユーザー・グループ表示\n"
            "  dirlens -e py           .pyのみ表示（旧 -t）\n"
            "  dirlens -S              サイズ順ソート（旧 -s）\n"
            "  dirlens -C              クリップボードにコピー（旧 -c）\n"
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
                    help="-G --date -m -C のショートカット")
    args = ap.parse_args()

    # ── エイリアスのマージ ────────────────────────────────────
    if args.level      is not None: args.depth    = args.level
    if args.date_tree:              args.date     = True
    if args.include_tree:           args.include  = (args.include or []) + args.include_tree
    if args.exclude_tree:           args.exclude  = (args.exclude or []) + args.exclude_tree
    if args.no_color_tree:          args.no_color = True
    if args.json_tree:              args.json     = True

    # --ai: -G --date -m -C のショートカット
    if args.ai:
        args.gitignore = True
        args.date      = True
        args.markdown  = True
        args.copy      = True

    if args.no_color or args.markdown or args.json:
        USE_COLOR = False

    target = Path(args.path).resolve()
    if not target.exists():
        print(f"エラー: '{args.path}' が見つかりません", file=sys.stderr); sys.exit(1)
    if not target.is_dir():
        print(f"エラー: '{args.path}' はディレクトリではありません", file=sys.stderr); sys.exit(1)

    cfg = Cfg(args, str(target))
    active_pats = load_gitignore(str(target)) if args.gitignore else []
    _prefetch_sizes(str(target))

    # ── JSON ─────────────────────────────────────────────────
    if args.json:
        print(json.dumps(build_json_tree(str(target), 0, cfg, active_pats),
                         ensure_ascii=False, indent=2)); return

    # ── HTML ─────────────────────────────────────────────────
    if args.html:
        out = Path(args.html)
        out.write_text(generate_html(str(target), cfg, active_pats), encoding="utf-8")
        print(f"✓ {out} を生成しました ({fmt_size(out.stat().st_size)})"); return

    # ── テキスト出力 ─────────────────────────────────────────
    if args.copy:
        _buf = io.StringIO(); _old = sys.stdout; sys.stdout = _buf

    if args.markdown: print("```")

    root_sz          = dir_size(str(target))
    root_nd, root_nf = count_entries(str(target), cfg, active_pats)
    root_label       = target.name if target.name else str(target)
    root_parts       = [fmt_count(root_nd, root_nf), fmt_size(root_sz)]
    if args.date:
        try: root_parts.append(fmt_date(target.stat().st_mtime))
        except OSError: pass

    root_emoji = (get_emoji(root_label, is_dir=True) + " ") if args.emoji else ""
    print(f"{c(root_emoji + root_label + '/', BOLD, BLUE)} "
          f"{c('(' + ', '.join(root_parts) + ')', DIM)}")

    stats = {"files": 0, "dirs": 0, "extensions": {}}
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
