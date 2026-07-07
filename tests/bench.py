#!/usr/bin/env python3
"""dirlens の Python 版 / Rust 版 速度比較ベンチマーク。

使い方:
    python3 tests/bench.py [オプション] [-- dirlens に渡すフラグ...]

例:
    python3 tests/bench.py                          # カレントディレクトリを --agent で計測
    python3 tests/bench.py --dir ~/project          # 対象ディレクトリを指定
    python3 tests/bench.py --runs 10 -- --agent -G  # フラグを自由に指定
    python3 tests/bench.py -- -M                    # import 依存グラフだけ計測
    python3 tests/bench.py --mode compat            # 同一アルゴリズム比較のみ

比較対象は 3 系列（--mode で選択。既定は both = 全部）:
  Python      : python ブランチの dirlens.py（最終版・バグ修正込み）を python3 で直接実行
                （npm 経由だと Node ランチャーの起動時間が混ざるため使わない）
  Rust(既定)  : rust/target/release/dirlens そのまま。
                BPE 正確トークン・AST パース・git check-ignore 厳密判定など
                Python 版には無い重い解析を含むため、「機能込みの実測」
  Rust(同条件): DIRLENS_COMPAT=python を付けて Python 版と同一アルゴリズムに
                揃えた実行。「純粋な言語・実装の速度差」

計測は time.perf_counter_ns()（ナノ秒精度）で、系列を交互に実行して
ファイルシステムキャッシュの偏りを避ける。出力は /dev/null に捨てて
端末描画のコストを除外する。
"""
import argparse
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


def fmt_s(ns: float) -> str:
    """ナノ秒を秒に変換して小数9桁で表示する。"""
    return f"{ns / 1e9:.9f}"


def get_python_dirlens() -> Path:
    """python ブランチから dirlens.py を取り出して一時ファイルに置く。"""
    for ref in ("python", "origin/python"):
        r = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "show", f"{ref}:dirlens.py"],
            capture_output=True,
        )
        if r.returncode == 0 and r.stdout:
            f = tempfile.NamedTemporaryFile(
                mode="wb", suffix="_dirlens.py", delete=False
            )
            f.write(r.stdout)
            f.close()
            return Path(f.name)
    sys.exit(
        "エラー: python ブランチから dirlens.py を取得できませんでした。\n"
        "  git fetch origin python:python を実行してから再試行してください。"
    )


def get_rust_dirlens(override: str | None) -> Path:
    if override:
        p = Path(override)
        if not p.is_file():
            sys.exit(f"エラー: --bin で指定されたバイナリがありません: {p}")
        return p
    release = REPO_ROOT / "rust" / "target" / "release" / "dirlens"
    if release.is_file():
        return release
    found = shutil.which("dirlens")
    if found:
        print(
            "注意: release ビルドが無いため PATH 上の dirlens を使います。\n"
            "      npm 版の場合 Node ランチャーの起動時間（数十ms）が上乗せされます。\n"
            "      正確な比較には: cd rust && cargo build --release\n",
            file=sys.stderr,
        )
        return Path(found)
    sys.exit(
        "エラー: Rust 版バイナリが見つかりません。\n"
        "  cd rust && cargo build --release を実行してください。"
    )


def run_once(cmd: list[str], env: dict[str, str]) -> int:
    """1 回実行して所要時間をナノ秒で返す。失敗したら中断する。"""
    t0 = time.perf_counter_ns()
    r = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    elapsed = time.perf_counter_ns() - t0
    if r.returncode != 0:
        sys.exit(
            f"エラー: コマンドが失敗しました (exit {r.returncode}): {' '.join(cmd)}\n"
            f"{r.stderr.decode('utf-8', 'replace')}"
        )
    return elapsed


def main() -> None:
    ap = argparse.ArgumentParser(
        description="dirlens Python/Rust 速度比較", add_help=True
    )
    ap.add_argument("--dir", default=".", help="計測対象ディレクトリ（既定: カレント）")
    ap.add_argument("--runs", type=int, default=5, help="計測回数（既定: 5）")
    ap.add_argument("--warmup", type=int, default=1, help="ウォームアップ回数（既定: 1）")
    ap.add_argument("--bin", default=None, help="Rust 版バイナリのパスを明示指定")
    ap.add_argument(
        "--mode",
        choices=["both", "default", "compat"],
        default="both",
        help="Rust 側の計測系列: both=既定+同条件（既定）/ default=既定のみ / compat=同条件のみ",
    )
    ap.add_argument(
        "flags",
        nargs=argparse.REMAINDER,
        help="-- 以降は dirlens にそのまま渡すフラグ（省略時: --agent）",
    )
    args = ap.parse_args()

    flags = [f for f in args.flags if f != "--"] or ["--agent"]
    target = Path(args.dir).resolve()
    if not target.is_dir():
        sys.exit(f"エラー: 対象ディレクトリがありません: {target}")

    py_script = get_python_dirlens()
    rust_bin = get_rust_dirlens(args.bin)

    base_env = dict(os.environ)
    base_env.pop("DIRLENS_COMPAT", None)
    compat_env = dict(base_env, DIRLENS_COMPAT="python")

    # 系列: (表示名, コマンド, 環境)
    series: list[tuple[str, list[str], dict[str, str]]] = [
        ("Python", [sys.executable, str(py_script), str(target), *flags], base_env),
    ]
    if args.mode in ("both", "default"):
        series.append(("Rust(既定)", [str(rust_bin), str(target), *flags], base_env))
    if args.mode in ("both", "compat"):
        series.append(("Rust(同条件)", [str(rust_bin), str(target), *flags], compat_env))

    rust_ver = subprocess.run(
        [str(rust_bin), "--version"], capture_output=True, text=True
    ).stdout.strip() or "不明"

    print("dirlens 速度比較: Python 版 vs Rust 版")
    print(f"  Python : dirlens.py（python ブランチ）+ {sys.version.split()[0]}")
    print(f"  Rust   : {rust_bin}（{rust_ver}）")
    print(f"  対象   : {target}")
    print(f"  フラグ : {' '.join(flags)}")
    print(f"  計測   : {args.runs} 回（ウォームアップ {args.warmup} 回）・単位は秒")
    if args.mode != "compat":
        print("  ※ Rust(既定) は BPE トークン・AST・git 厳密判定込み（Python 版より多くの解析を行う）")
    if args.mode != "default":
        print("  ※ Rust(同条件) は DIRLENS_COMPAT=python で Python 版と同一アルゴリズムに揃えた値")
    print()

    for _ in range(args.warmup):
        for _, cmd, env in series:
            run_once(cmd, env)

    times: dict[str, list[int]] = {name: [] for name, _, _ in series}
    header = f"  {'#':>3}  " + "  ".join(f"{name + ' [s]':>16}" for name, _, _ in series)
    print(header)
    for i in range(1, args.runs + 1):
        row = [f"  {i:>3}"]
        for name, cmd, env in series:
            ns = run_once(cmd, env)
            times[name].append(ns)
            row.append(f"{fmt_s(ns):>16}")
        print("  ".join(row))

    print()
    print(f"  {'':6}  " + "  ".join(f"{name + ' [s]':>16}" for name, _, _ in series))
    for label, fn in (
        ("最小", min),
        ("中央値", statistics.median),
        ("平均", statistics.fmean),
        ("最大", max),
    ):
        row = [f"  {label:<6}"]
        for name, _, _ in series:
            row.append(f"{fmt_s(fn(times[name])):>16}")
        print("  ".join(row))

    print()
    py_med = statistics.median(times["Python"])
    if "Rust(同条件)" in times:
        r = py_med / statistics.median(times["Rust(同条件)"])
        print(f"  同一アルゴリズム比較: Rust 版は Python 版の {r:.2f} 倍高速（中央値比）")
    if "Rust(既定)" in times:
        r = py_med / statistics.median(times["Rust(既定)"])
        if r >= 1:
            print(f"  機能込みの既定動作 : Rust 版は Python 版の {r:.2f} 倍高速（中央値比）")
        else:
            print(
                f"  機能込みの既定動作 : Rust 版は Python 版の {1 / r:.2f} 倍低速（中央値比）"
                "— BPE/AST 等の追加解析ぶん"
            )

    py_script.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
