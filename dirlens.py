#!/usr/bin/env python3
"""
dirlens – ファイルサイズ付きディレクトリツリー表示ツール
対応環境: macOS / Linux / Windows  (Python 3.8+)
"""

import os
import sys
import argparse
import fnmatch
import datetime
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


# ─── アイテム数表示 ────────────────────────────────────────────
def fmt_count(nd, nf):
    d     = f"{nd} {'dir'  if nd == 1 else 'dirs'}"
    f_str = f"{nf} {'file' if nf == 1 else 'files'}"
    return f"{d}, {f_str}"


# ─── 日時表示 ─────────────────────────────────────────────────
def fmt_date(mtime):
    sec = int((datetime.datetime.now() -
               datetime.datetime.fromtimestamp(mtime)).total_seconds())
    if sec <    60: return "今"
    if sec <  3600: return f"{sec // 60}分前"
    if sec < 86400: return f"{sec // 3600}時間前"
    days = sec // 86400
    if days ==  1: return "昨日"
    if days <   7: return f"{days}日前"
    if days <  30: return f"{days // 7}週間前"
    if days < 365: return f"{days // 30}ヶ月前"
    return f"{days // 365}年前"


# ─── .gitignore サポート ──────────────────────────────────────
def load_gitignore(directory):
    """指定ディレクトリの .gitignore パターンを読み込む。"""
    patterns = []
    path = os.path.join(directory, ".gitignore")
    if os.path.isfile(path):
        try:
            with open(path, encoding="utf-8", errors="ignore") as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#"):
                        patterns.append(line)
        except OSError:
            pass
    return patterns

def is_ignored(name, rel_path, is_dir, patterns):
    """gitignore パターンにマッチするか判定する（簡易実装）。"""
    rel = rel_path.replace("\\", "/")
    for pat in patterns:
        if pat.startswith("!"):
            continue
        dir_only = pat.endswith("/")
        p = pat.rstrip("/")
        if dir_only and not is_dir:
            continue
        if p.startswith("/"):
            if fnmatch.fnmatch(rel, p.lstrip("/")):
                return True
        else:
            if fnmatch.fnmatch(name, p):
                return True
            if fnmatch.fnmatch(rel, p):
                return True
            if fnmatch.fnmatch(rel, "*/" + p):
                return True
    return False


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
def count_entries(path, show_all, active_pats, root, type_ext, use_gitignore=False):
    try:
        raw = list(os.scandir(path))
    except OSError:
        return 0, 0
    entries = [e for e in raw if show_all or not e.name.startswith(".")]

    # count_entries の対象ディレクトリ自身の .gitignore も反映する
    local_pats = active_pats
    if use_gitignore:
        local = load_gitignore(path)
        if local:
            rel_dir = os.path.relpath(path, root).replace("\\", "/")
            adjusted = []
            for pat in local:
                if not pat.startswith("!") and pat.startswith("/"):
                    adjusted.append("/" + rel_dir + pat)
                else:
                    adjusted.append(pat)
            local_pats = active_pats + adjusted

    if local_pats:
        entries = [e for e in entries
                   if not is_ignored(e.name,
                                     os.path.relpath(e.path, root),
                                     e.is_dir(follow_symlinks=False),
                                     local_pats)]
    nd    = sum(1 for e in entries if e.is_dir(follow_symlinks=False))
    files = [e for e in entries if not e.is_dir(follow_symlinks=False)]
    if type_ext:
        files = [f for f in files
                 if os.path.splitext(f.name)[1].lower() == type_ext]
    return nd, len(files)


# ─── ツリー描画 ───────────────────────────────────────────────
PIPE  = "│   "
FORK  = "├── "
LAST  = "└── "
BLANK = "    "

def render(path, prefix, depth, opts, stats, active_pats):
    """
    active_pats: 現在の階層で有効な gitignore パターンの累積リスト。
    サブディレクトリに入るたびにローカルの .gitignore を読み込んで追記する。
    リストは新規作成して渡すため、兄弟ディレクトリには影響しない。
    """
    max_depth, show_all, by_size, show_date, use_gitignore, root, type_ext = opts

    if max_depth is not None and depth >= max_depth:
        return

    # このディレクトリの .gitignore を読んでパターンを積み重ねる（ルートは main で読み済み）
    if use_gitignore and depth > 0:
        local = load_gitignore(path)
        if local:
            # アンカーパターン（/xxx）をルートからの相対パスに変換する
            # 例: src/.gitignore の /build → /src/build
            rel_dir = os.path.relpath(path, root).replace("\\", "/")
            adjusted = []
            for pat in local:
                if not pat.startswith("!") and pat.startswith("/"):
                    adjusted.append("/" + rel_dir + pat)
                else:
                    adjusted.append(pat)
            active_pats = active_pats + adjusted  # 新しいリストを作成（親に影響しない）

    try:
        raw = list(os.scandir(path))
    except PermissionError:
        print(f"{prefix}{LAST}{c('[アクセス拒否]', DIM)}")
        return

    entries = [e for e in raw if show_all or not e.name.startswith(".")]

    if active_pats:
        entries = [e for e in entries
                   if not is_ignored(e.name,
                                     os.path.relpath(e.path, root),
                                     e.is_dir(follow_symlinks=False),
                                     active_pats)]

    dirs  = [e for e in entries if     e.is_dir(follow_symlinks=False)]
    files = [e for e in entries if not e.is_dir(follow_symlinks=False)]

    if type_ext:
        files = [f for f in files
                 if os.path.splitext(f.name)[1].lower() == type_ext]

    def esz(e):
        try:   return e.stat(follow_symlinks=True).st_size
        except OSError: return 0

    def emtime(e):
        try:   return e.stat(follow_symlinks=True).st_mtime
        except OSError: return 0

    if by_size:
        dirs.sort(key=lambda e: dir_size(e.path), reverse=True)
        files.sort(key=lambda e: esz(e), reverse=True)
    else:
        dirs.sort(key=lambda e: e.name.casefold())
        files.sort(key=lambda e: e.name.casefold())

    combined = dirs + files

    for i, entry in enumerate(combined):
        is_last = (i == len(combined) - 1)
        branch  = LAST if is_last else FORK
        cont    = BLANK if is_last else PIPE

        if entry.is_dir(follow_symlinks=False):
            sz     = dir_size(entry.path)
            nd, nf = count_entries(entry.path, show_all, active_pats, root, type_ext, use_gitignore)
            stats["dirs"] += 1

            parts = [fmt_count(nd, nf), fmt_size(sz)]
            if show_date:
                try:
                    parts.append(fmt_date(entry.stat(follow_symlinks=False).st_mtime))
                except OSError:
                    pass

            name = c(f"{entry.name}/", BOLD, CYAN)
            meta = c(f"({', '.join(parts)})", DIM)
            print(f"{prefix}{branch}{name} {meta}")
            render(entry.path, prefix + cont, depth + 1, opts, stats, active_pats)

        else:
            sz  = esz(entry)
            sym = " →" if entry.is_symlink() else ""
            stats["files"] += 1

            ext = os.path.splitext(entry.name)[1].lower()
            key = ext if ext else "(no ext)"
            stats["extensions"][key] = stats["extensions"].get(key, 0) + 1

            parts = [fmt_size(sz)]
            if show_date:
                mt = emtime(entry)
                if mt:
                    parts.append(fmt_date(mt))

            name = c(f"{entry.name}{sym}", MAGENTA if entry.is_symlink() else GREEN)
            meta = c(f"({', '.join(parts)})", DIM)
            print(f"{prefix}{branch}{name} {meta}")


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
            "  dirlens                  カレントディレクトリを表示\n"
            "  dirlens ~/Desktop        指定したディレクトリを表示\n"
            "  dirlens -d 2             深さ 2 階層まで表示\n"
            "  dirlens -a               隠しファイル (.xxx) も表示\n"
            "  dirlens -s               サイズの大きい順に表示\n"
            "  dirlens -g               .gitignore のファイルを除外\n"
            "  dirlens --date           最終更新日時を表示\n"
            "  dirlens -t py            .py ファイルのみ表示\n"
            "  dirlens -m               Markdown コードブロックで出力\n"
            "  dirlens --no-color       カラーなしで表示"
            "  dirlens > dirlens.txt    dirlens.txtに書き出す"
        ),
    )
    ap.add_argument("path",              nargs="?", default=".", help="対象ディレクトリ（省略時はカレント）")
    ap.add_argument("-d", "--depth",     type=int,  metavar="N", help="表示する最大の深さ")
    ap.add_argument("-a", "--all",       action="store_true",    help="隠しファイルも表示する")
    ap.add_argument("-s", "--sort-size", action="store_true",    help="サイズが大きい順に並べる")
    ap.add_argument("-g", "--gitignore", action="store_true",    help=".gitignore のファイルを除外する（サブディレクトリも対応）")
    ap.add_argument("--date",            action="store_true",    help="最終更新日時を表示する")
    ap.add_argument("-t", "--type",      metavar="EXT",          help="指定した拡張子のみ表示 (例: py, md)")
    ap.add_argument("-m", "--markdown",  action="store_true",    help="Markdown コードブロックで出力")
    ap.add_argument("--no-color",        action="store_true",    help="カラー表示を無効化する")
    args = ap.parse_args()

    if args.no_color or args.markdown:
        USE_COLOR = False

    target = Path(args.path).resolve()
    if not target.exists():
        print(f"エラー: '{args.path}' が見つかりません", file=sys.stderr)
        sys.exit(1)
    if not target.is_dir():
        print(f"エラー: '{args.path}' はディレクトリではありません", file=sys.stderr)
        sys.exit(1)

    # ルートの .gitignore を起点として読み込む
    active_pats = load_gitignore(str(target)) if args.gitignore else []
    type_ext    = ("." + args.type.lstrip(".")).lower() if args.type else None
    opts        = (args.depth, args.all, args.sort_size, args.date,
                   args.gitignore, str(target), type_ext)

    if args.markdown:
        print("```")

    # ルートを表示
    root_sz          = dir_size(str(target))
    root_nd, root_nf = count_entries(str(target), args.all, active_pats, str(target), type_ext, args.gitignore)
    root_label       = target.name if target.name else str(target)

    parts = [fmt_count(root_nd, root_nf), fmt_size(root_sz)]
    if args.date:
        try:
            parts.append(fmt_date(target.stat().st_mtime))
        except OSError:
            pass

    root_name = c(f"{root_label}/", BOLD, BLUE)
    root_meta = c(f"({', '.join(parts)})", DIM)
    print(f"{root_name} {root_meta}")

    stats = {"files": 0, "dirs": 0, "extensions": {}}
    render(str(target), "", 0, opts, stats, active_pats)

    # ─ サマリー ──────────────────────────────────────────────
    print()
    summary = f"  {stats['dirs']} ディレクトリ,  {stats['files']} ファイル"
    if args.gitignore:
        summary += "  (.gitignore 適用済み)"
    if type_ext:
        summary += f"  (フィルタ: {type_ext})"
    print(c(summary, DIM))

    if stats["extensions"]:
        sorted_exts = sorted(stats["extensions"].items(), key=lambda x: -x[1])
        ext_line = "  " + "  ".join(f"{ext} ×{n}" for ext, n in sorted_exts[:8])
        print(c(ext_line, DIM))

    if args.markdown:
        print("```")


if __name__ == "__main__":
    main()
