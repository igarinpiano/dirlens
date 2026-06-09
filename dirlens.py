#!/usr/bin/env python3
"""
dirlens – ファイルサイズ付きディレクトリツリー表示ツール
対応環境: macOS / Linux / Windows  (Python 3.8+)
"""

import io, json, os, sys, argparse, fnmatch, datetime, subprocess
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


# ─── クリップボード ───────────────────────────────────────────
def copy_to_clipboard(text):
    try:
        if sys.platform == "darwin":
            subprocess.run(["pbcopy"], input=text.encode(), check=True,
                           stderr=subprocess.DEVNULL)
            return True
        if sys.platform == "win32":
            subprocess.run(["clip"], input=text.encode("utf-16"), check=True,
                           stderr=subprocess.DEVNULL)
            return True
        for cmd in [["wl-copy"],
                    ["xclip", "-selection", "clipboard"],
                    ["xsel", "--clipboard", "--input"]]:
            try:
                subprocess.run(cmd, input=text.encode(), check=True,
                               stderr=subprocess.DEVNULL)
                return True
            except FileNotFoundError:
                continue
        return False
    except Exception:
        return False


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


# ─── .gitignore ───────────────────────────────────────────────
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
    rel = rel_path.replace("\\", "/")
    for pat in patterns:
        if pat.startswith("!"): continue
        dir_only = pat.endswith("/")
        p = pat.rstrip("/")
        if dir_only and not is_dir: continue
        if p.startswith("/"):
            if fnmatch.fnmatch(rel, p.lstrip("/")): return True
        else:
            if fnmatch.fnmatch(name, p): return True
            if fnmatch.fnmatch(rel, p): return True
            if fnmatch.fnmatch(rel, "*/" + p): return True
    return False

def _extend_pats(active_pats, path, cfg):
    """サブディレクトリの .gitignore を読み込んでパターンを拡張する（ルートは除く）。"""
    if not cfg.use_gitignore: return active_pats
    if os.path.normpath(path) == os.path.normpath(cfg.root): return active_pats
    local = load_gitignore(path)
    if not local: return active_pats
    rel_dir = os.path.relpath(path, cfg.root).replace("\\", "/")
    adjusted = []
    for pat in local:
        if not pat.startswith("!") and pat.startswith("/"):
            adjusted.append("/" + rel_dir + pat)
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


# ─── 設定クラス ───────────────────────────────────────────────
class Cfg:
    def __init__(self, args, root):
        self.root          = root
        self.max_depth     = args.depth
        self.show_all      = args.all
        self.by_size       = args.sort_size
        self.show_date     = args.date
        self.use_gitignore = args.gitignore
        self.show_bar      = args.bar
        self.min_size      = parse_size(args.min_size) if args.min_size else None
        self.max_size      = parse_size(args.max_size) if args.max_size else None
        self.excludes      = args.exclude or []
        self.includes      = args.include or []
        self.show_emoji    = args.emoji
        self.type_ext      = ("." + args.type.lstrip(".")).lower() if args.type else None


# ─── 共通フィルタリング ───────────────────────────────────────
def _filter(path, cfg, active_pats):
    """エントリを取得してフィルタリングする。"""
    try: raw = list(os.scandir(path))
    except PermissionError: return [], []

    entries = [e for e in raw if cfg.show_all or not e.name.startswith(".")]

    if active_pats:
        entries = [e for e in entries
                   if not is_ignored(e.name,
                                     os.path.relpath(e.path, cfg.root),
                                     e.is_dir(follow_symlinks=False),
                                     active_pats)]

    dirs  = [e for e in entries if     e.is_dir(follow_symlinks=False)]
    files = [e for e in entries if not e.is_dir(follow_symlinks=False)]

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


# ─── ツリー描画 ───────────────────────────────────────────────
PIPE = "│   "; FORK = "├── "; LAST = "└── "; BLANK = "    "

def render(path, prefix, depth, cfg, stats, active_pats):
    if cfg.max_depth is not None and depth >= cfg.max_depth: return

    cur_pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, cur_pats)

    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0
    def emtime(e):
        try: return e.stat(follow_symlinks=True).st_mtime
        except OSError: return 0

    if cfg.by_size:
        dirs.sort(key=lambda e: dir_size(e.path), reverse=True)
        files.sort(key=lambda e: esz(e), reverse=True)
    else:
        dirs.sort(key=lambda e: e.name.casefold())
        files.sort(key=lambda e: e.name.casefold())

    combined = dirs + files
    cur_dir_size = dir_size(path)

    for i, entry in enumerate(combined):
        is_last = (i == len(combined) - 1)
        branch = LAST if is_last else FORK
        cont   = BLANK if is_last else PIPE

        if entry.is_dir(follow_symlinks=False):
            sz     = dir_size(entry.path)
            nd, nf = count_entries(entry.path, cfg, cur_pats)
            stats["dirs"] += 1

            emoji = (get_emoji(entry.name, is_dir=True) + " ") if cfg.show_emoji else ""
            parts = [fmt_count(nd, nf), fmt_size(sz)]
            if cfg.show_date:
                try: parts.append(fmt_date(entry.stat(follow_symlinks=False).st_mtime))
                except OSError: pass
            bar = (" " + fmt_bar(sz, cur_dir_size)) if cfg.show_bar and cur_dir_size else ""

            name = c(f"{emoji}{entry.name}/", BOLD, CYAN)
            meta = c(f"({', '.join(parts)}){bar}", DIM)
            print(f"{prefix}{branch}{name} {meta}")
            render(entry.path, prefix + cont, depth + 1, cfg, stats, cur_pats)

        else:
            sz  = esz(entry)
            sym = " →" if entry.is_symlink() else ""
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

            name = c(f"{emoji}{entry.name}{sym}", MAGENTA if entry.is_symlink() else GREEN)
            meta = c(f"({', '.join(parts)}){bar}", DIM)
            print(f"{prefix}{branch}{name} {meta}")


# ─── JSON出力 ─────────────────────────────────────────────────
def build_json_tree(path, depth, cfg, active_pats):
    cur_pats = _extend_pats(active_pats, path, cfg)
    dirs, files = _filter(path, cfg, cur_pats)
    sz = dir_size(path)

    def esz(e):
        try: return e.stat(follow_symlinks=True).st_size
        except OSError: return 0

    if cfg.by_size:
        dirs.sort(key=lambda e: dir_size(e.path), reverse=True)
        files.sort(key=lambda e: esz(e), reverse=True)
    else:
        dirs.sort(key=lambda e: e.name.casefold())
        files.sort(key=lambda e: e.name.casefold())

    children = []
    if cfg.max_depth is None or depth < cfg.max_depth:
        for d in dirs:
            children.append(build_json_tree(d.path, depth + 1, cfg, cur_pats))
        for f in files:
            f_sz = esz(f)
            children.append({
                "name": f.name, "type": "file",
                "size": f_sz, "size_human": fmt_size(f_sz),
                "ext": os.path.splitext(f.name)[1].lower(),
                "path": os.path.relpath(f.path, cfg.root),
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
        sz = dir_size(path)
        name = os.path.basename(path) or path

        def esz(e):
            try: return e.stat(follow_symlinks=True).st_size
            except OSError: return 0

        if cfg.by_size:
            dirs.sort(key=lambda e: dir_size(e.path), reverse=True)
            files.sort(key=lambda e: esz(e), reverse=True)
        else:
            dirs.sort(key=lambda e: e.name.casefold())
            files.sort(key=lambda e: e.name.casefold())

        ch = ""
        if cfg.max_depth is None or depth < cfg.max_depth:
            for d in dirs:
                ch += _node(d.path, depth + 1, pats)
        else:
            for d in dirs:
                ch += (f'<div class="item dir-leaf">📁 {d.name}/'
                       f' <span class="sz">{fmt_size(dir_size(d.path))}</span></div>\n')

        for f in files:
            f_sz = esz(f)
            ch += (f'<div class="item file">'
                   f'<span class="emoji">{get_emoji(f.name)}</span>'
                   f'<span class="fname"> {f.name}</span>'
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
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
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
.fname{{color:#a6e3a1}}
.sz{{color:#585b70;font-size:12px}}
.emoji{{width:1.6em;display:inline-block}}
.hidden{{display:none!important}}
</style></head><body>
<h1>🌳 dirlens — {root_name}</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">{tree}</div>
<script>
function search(q){{
  q=q.toLowerCase();
  document.querySelectorAll('.file').forEach(el=>{{
    const n=el.querySelector('.fname')?.textContent.toLowerCase()||'';
    el.classList.toggle('hidden',!!q&&!n.includes(q));
  }});
  if(q) document.querySelectorAll('details').forEach(d=>d.open=true);
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
            "  dirlens -g -m -c            gitignore除外→Markdown→クリップボードコピー\n"
            "  dirlens --bar               ディスク占有率バーを表示\n"
            "  dirlens --min-size 1M       1MB以上のファイルのみ\n"
            "  dirlens --exclude '*.log'   パターンで除外（複数指定可）\n"
            "  dirlens --include 'test_*'  パターンで抽出（複数指定可）\n"
            "  dirlens --emoji             絵文字アイコンを表示\n"
            "  dirlens --json              JSON形式で出力\n"
            "  dirlens --html              HTMLレポートを生成 (dirlens.html)\n"
            "  dirlens --no-color          カラーなしで表示\n"
            "  dirlens > dirlens.txt       dirlens.txt に書き出す"
        ),
    )
    # ── 既存オプション ────────────────────────────────────────
    ap.add_argument("path",              nargs="?", default=".")
    ap.add_argument("-d", "--depth",     type=int,  metavar="N")
    ap.add_argument("-a", "--all",       action="store_true")
    ap.add_argument("-s", "--sort-size", action="store_true")
    ap.add_argument("-g", "--gitignore", action="store_true")
    ap.add_argument("--date",            action="store_true")
    ap.add_argument("-t", "--type",      metavar="EXT")
    ap.add_argument("-m", "--markdown",  action="store_true")
    ap.add_argument("--no-color",        action="store_true")
    # ── 新規オプション ────────────────────────────────────────
    ap.add_argument("--bar",             action="store_true",
                    help="親ディレクトリに対するディスク占有率バーを表示")
    ap.add_argument("--min-size",        metavar="SIZE",
                    help="指定サイズ以上のファイルのみ表示 (例: 1M, 500K)")
    ap.add_argument("--max-size",        metavar="SIZE",
                    help="指定サイズ以下のファイルのみ表示 (例: 10M)")
    ap.add_argument("--exclude",         metavar="PATTERN", action="append",
                    help="除外パターン（複数指定可）")
    ap.add_argument("--include",         metavar="PATTERN", action="append",
                    help="このパターンのみ表示（複数指定可）")
    ap.add_argument("--emoji",           action="store_true",
                    help="拡張子に応じた絵文字を表示")
    ap.add_argument("--json",            action="store_true",
                    help="JSON形式で標準出力に出力")
    ap.add_argument("--html",            nargs="?", const="dirlens.html", metavar="FILE",
                    help="HTMLレポートを生成 (デフォルト: dirlens.html)")
    ap.add_argument("-c", "--copy",      action="store_true",
                    help="出力をクリップボードにコピー")
    args = ap.parse_args()

    if args.no_color or args.markdown or args.json:
        USE_COLOR = False

    target = Path(args.path).resolve()
    if not target.exists():
        print(f"エラー: '{args.path}' が見つかりません", file=sys.stderr); sys.exit(1)
    if not target.is_dir():
        print(f"エラー: '{args.path}' はディレクトリではありません", file=sys.stderr); sys.exit(1)

    cfg = Cfg(args, str(target))
    active_pats = load_gitignore(str(target)) if args.gitignore else []

    # ── JSON ────────────────────────────────────────────────
    if args.json:
        print(json.dumps(build_json_tree(str(target), 0, cfg, active_pats),
                         ensure_ascii=False, indent=2))
        return

    # ── HTML ────────────────────────────────────────────────
    if args.html:
        out = Path(args.html)
        out.write_text(generate_html(str(target), cfg, active_pats), encoding="utf-8")
        print(f"✓ {out} を生成しました ({fmt_size(out.stat().st_size)})")
        return

    # ── テキスト出力（--copy はバッファ経由）──────────────────
    if args.copy:
        _buf = io.StringIO()
        _old = sys.stdout
        sys.stdout = _buf

    if args.markdown:
        print("```")

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
    summary = f"  {stats['dirs']} ディレクトリ,  {stats['files']} ファイル"
    if args.gitignore:  summary += "  (.gitignore 適用済み)"
    if cfg.type_ext:    summary += f"  (フィルタ: {cfg.type_ext})"
    if cfg.excludes:    summary += f"  (除外: {', '.join(cfg.excludes)})"
    if cfg.includes:    summary += f"  (抽出: {', '.join(cfg.includes)})"
    if cfg.min_size:    summary += f"  (最小: {fmt_size(cfg.min_size)})"
    if cfg.max_size:    summary += f"  (最大: {fmt_size(cfg.max_size)})"
    print(c(summary, DIM))

    if stats["extensions"]:
        exts = sorted(stats["extensions"].items(), key=lambda x: -x[1])
        print(c("  " + "  ".join(f"{e} ×{n}" for e, n in exts[:8]), DIM))

    if args.markdown:
        print("```")

    # ── クリップボードコピー ──────────────────────────────────
    if args.copy:
        sys.stdout = _old
        text = _buf.getvalue()
        print(text, end="")
        ok = copy_to_clipboard(text)
        msg = "✓ クリップボードにコピーしました" if ok \
              else "✗ コピー失敗 (pbcopy / xclip / wl-copy が必要)"
        print(c(msg, BOLD, GREEN if ok else DIM), file=sys.stderr)


if __name__ == "__main__":
    main()
