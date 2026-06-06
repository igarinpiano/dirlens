#!/usr/bin/env python3
"""
dirlens – ファイルサイズ＋アイテム数付きディレクトリツリー表示ツール
対応環境: macOS / Linux / Windows  (Python 3.8+)
"""

import os
import sys
import argparse
from pathlib import Path

# ─── カラー設定 ──────────────────────────────────────────────
def _enable_color():
    if not hasattr(sys.stdout, "isatty") or not sys.stdout.isatty():
        return False
    if os.name == "nt":
        try:
            import ctypes
            kernel32 = ctypes.windll.kernel32
            kernel32.SetConsoleMode(kernel32.GetStdHandle(-11), 7)
        except Exception:
            pass
        return bool(
            os.environ.get("WT_SESSION")
            or os.environ.get("TERM_PROGRAM")
            or os.environ.get("TERM")
            or os.environ.get("ANSICON")
        )
    return True

USE_COLOR = _enable_color()

RESET   = "\033[0m"
BOLD    = "\033[1m"
DIM     = "\033[2m"
BLUE    = "\033[34m"
CYAN    = "\033[36m"
GREEN   = "\033[32m"
MAGENTA = "\033[35m"

def c(text, *codes):
    return ("".join(codes) + text + RESET) if USE_COLOR else text

# ─── サイズ表示 ───────────────────────────────────────────────
def fmt_size(n):
    if n == 0:
        return "0 bytes"
    for unit, factor in (("TB", 1 << 40), ("GB", 1 << 30), ("MB", 1 << 20), ("KB", 1 << 10)):
        if n >= factor:
            s = f"{n / factor:.2f}".rstrip("0").rstrip(".")
            return f"{s} {unit}"
    return f"{n} {'byte' if n == 1 else 'bytes'}"

# ─── ディレクトリサイズ（キャッシュ付き） ─────────────────────
_cache = {}

def dir_size(path):
    if path in _cache:
        return _cache[path]
    total = 0
    try:
        with os.scandir(path) as it:
            for e in it:
                try:
                    if e.is_file(follow_symlinks=False):
                        total += e.stat(follow_symlinks=False).st_size
                    elif e.is_dir(follow_symlinks=False):
                        total += dir_size(e.path)
                except OSError:
                    pass
    except OSError:
        pass
    _cache[path] = total
    return total

# ─── アイテム数カウント ────────────────────────────────────────
def count_items(path, show_all):
    """ディレクトリ直下のアイテム数 (num_dirs, num_files) を返す。"""
    try:
        entries = list(os.scandir(path))
    except OSError:
        return (0, 0)
    if not show_all:
        entries = [e for e in entries if not e.name.startswith(".")]
    nd = sum(1 for e in entries if e.is_dir(follow_symlinks=False))
    nf = sum(1 for e in entries if not e.is_dir(follow_symlinks=False))
    return (nd, nf)

def fmt_meta(nd, nf, sz):
    """アイテム数＋サイズをまとめて文字列化する。"""
    d_str = f"{nd} {'dir' if nd == 1 else 'dirs'}"
    f_str = f"{nf} {'file' if nf == 1 else 'files'}"
    return f"({d_str}, {f_str}, {fmt_size(sz)})"

# ─── ツリー描画 ───────────────────────────────────────────────
PIPE  = "│   "
FORK  = "├── "
LAST  = "└── "
BLANK = "    "

def render(path, prefix, depth, max_depth, show_all, by_size, stats):
    if max_depth is not None and depth >= max_depth:
        return

    try:
        raw = list(os.scandir(path))
    except PermissionError:
        print(f"{prefix}{LAST}{c('[アクセス拒否]', DIM)}")
        return

    entries = [e for e in raw if show_all or not e.name.startswith(".")]
    dirs  = [e for e in entries if     e.is_dir(follow_symlinks=False)]
    files = [e for e in entries if not e.is_dir(follow_symlinks=False)]

    def entry_size(e):
        try:
            return e.stat(follow_symlinks=True).st_size
        except OSError:
            return 0

    if by_size:
        dirs.sort(key=lambda e: dir_size(e.path), reverse=True)
        files.sort(key=lambda e: entry_size(e), reverse=True)
    else:
        dirs.sort(key=lambda e: e.name.casefold())
        files.sort(key=lambda e: e.name.casefold())

    combined = dirs + files

    for i, entry in enumerate(combined):
        is_last = (i == len(combined) - 1)
        branch  = LAST if is_last else FORK
        cont    = BLANK if is_last else PIPE

        if entry.is_dir(follow_symlinks=False):
            sz       = dir_size(entry.path)
            nd, nf   = count_items(entry.path, show_all)
            stats["dirs"] += 1
            name = c(f"{entry.name}/", BOLD, CYAN)
            meta = c(fmt_meta(nd, nf, sz), DIM)
            print(f"{prefix}{branch}{name} {meta}")
            render(entry.path, prefix + cont, depth + 1, max_depth, show_all, by_size, stats)
        else:
            sz  = entry_size(entry)
            sym = " →" if entry.is_symlink() else ""
            stats["files"] += 1
            name = c(f"{entry.name}{sym}", MAGENTA if entry.is_symlink() else GREEN)
            size = c(f"({fmt_size(sz)})", DIM)
            print(f"{prefix}{branch}{name} {size}")

# ─── エントリポイント ─────────────────────────────────────────
def main():
    global USE_COLOR
    sys.setrecursionlimit(10_000)

    ap = argparse.ArgumentParser(
        prog="dirlens",
        description="ファイルサイズ＋アイテム数付きのディレクトリツリーを表示します",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "使用例:\n"
            "  dirlens                  カレントディレクトリを表示\n"
            "  dirlens ~/Desktop        指定したディレクトリを表示\n"
            "  dirlens -d 2             深さ 2 階層まで表示\n"
            "  dirlens -a               隠しファイル (.xxx) も表示\n"
            "  dirlens -s               サイズの大きい順に表示\n"
            "  dirlens --no-color       カラーなしで表示"
        ),
    )
    ap.add_argument("path",              nargs="?", default=".", help="対象ディレクトリ（省略時はカレント）")
    ap.add_argument("-d", "--depth",     type=int,  metavar="N", help="表示する最大の深さ")
    ap.add_argument("-a", "--all",       action="store_true",    help="隠しファイルも表示する")
    ap.add_argument("-s", "--sort-size", action="store_true",    help="サイズが大きい順に並べる")
    ap.add_argument("--no-color",        action="store_true",    help="カラー表示を無効化する")
    args = ap.parse_args()

    if args.no_color:
        USE_COLOR = False

    target = Path(args.path).resolve()
    if not target.exists():
        print(f"エラー: '{args.path}' が見つかりません", file=sys.stderr)
        sys.exit(1)
    if not target.is_dir():
        print(f"エラー: '{args.path}' はディレクトリではありません", file=sys.stderr)
        sys.exit(1)

    root_label = target.name if target.name else str(target)
    root_sz    = dir_size(str(target))
    nd, nf     = count_items(str(target), args.all)

    root_name = c(f"{root_label}/", BOLD, BLUE)
    root_meta = c(fmt_meta(nd, nf, root_sz), DIM)
    print(f"{root_name} {root_meta}")

    stats = {"files": 0, "dirs": 0}
    render(str(target), "", 0, args.depth, args.all, args.sort_size, stats)

    print()
    print(c(f"  {stats['dirs']} ディレクトリ,  {stats['files']} ファイル", DIM))

if __name__ == "__main__":
    main()
