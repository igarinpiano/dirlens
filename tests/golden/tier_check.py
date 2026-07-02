#!/usr/bin/env python3
"""gitignore 2層の敵対的検証。

内蔵マッチャ（Tier3）と git check-ignore（Tier1）が意図的に異なる結果になる
パターンで、Rust 版の層選択・縮退が正しいことを確認する:

1. スラッシュを含むパターン（docs/*.tmp）
   - git:    .gitignore のあるディレクトリにアンカーされる → deep/docs/b.tmp は残る
   - 内蔵:   fnmatch の "*/docs/*.tmp" マッチで deep/docs/b.tmp も除外される
2. .git/info/exclude
   - git:    尊重される（secret.txt が消える）
   - 内蔵:   .gitignore しか読まないので残る
3. DIRLENS_GITIGNORE=builtin を与えた Rust は Python 版とバイト一致（Tier3 パリティ）
4. 非リポジトリでは Tier1 が自動縮退し、builtin 強制時と同一出力になる

使い方: python3 tier_check.py --bin <rust binary>
"""
import argparse
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(HERE, ".work", "tier")
DIRLENS_PY = os.path.join(REPO, "dirlens.py")

sys.path.insert(0, HERE)
from run import build_env  # noqa: E402


def build_fixture(root, with_git):
    shutil.rmtree(root, ignore_errors=True)
    os.makedirs(root)

    def w(rel, content="x\n"):
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            f.write(content)

    w(".gitignore", "docs/*.tmp\n")
    w("docs/a.tmp")
    w("deep/docs/b.tmp")
    w("deep/docs/c.txt")
    w("keep.txt")
    w("secret.txt")
    if with_git:
        env = dict(os.environ,
                   GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null",
                   HOME=root)
        subprocess.run(["git", "init", "-q", "-b", "main", root],
                       check=True, env=env, stdout=subprocess.DEVNULL)
        with open(os.path.join(root, ".git", "info", "exclude"), "w") as f:
            f.write("secret.txt\n")


def run_tool(cmd, cwd_target, env, extra_env=None):
    e = dict(env)
    if extra_env:
        e.update(extra_env)
    proc = subprocess.run(cmd + [cwd_target, "-G", "-a", "-I", ".git"],
                          capture_output=True, text=True, env=e, timeout=60)
    if proc.returncode != 0:
        raise RuntimeError(f"exit {proc.returncode}: {proc.stderr}")
    return proc.stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--py", help="旧 Python 版 dirlens.py（省略時: あればパリティ検証も行う）")
    args = ap.parse_args()
    rust = [os.path.abspath(args.bin)]
    py_path = args.py or DIRLENS_PY
    py = [sys.executable, py_path] if os.path.isfile(py_path) else None

    env = build_env()
    repo_root = os.path.join(WORK, "repo")
    plain_root = os.path.join(WORK, "plain")
    build_fixture(repo_root, with_git=True)
    build_fixture(plain_root, with_git=False)

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"PASS {name}")
        else:
            failures.append(name)
            print(f"FAIL {name}\n{detail}")

    # 1. git 層はアンカーされたパターンを正しく扱う（deep/docs/b.tmp は残る）
    out_git = run_tool(rust, repo_root, env)
    check("tier1_anchored_pattern_keeps_deep",
          "b.tmp" in out_git and "a.tmp" not in out_git, out_git)

    # 2. git 層は .git/info/exclude を尊重する
    check("tier1_info_exclude", "secret.txt" not in out_git, out_git)

    # 内蔵層は deep/docs/b.tmp も除外し、info/exclude は無視する
    out_builtin = run_tool(rust, repo_root, env, {"DIRLENS_GITIGNORE": "builtin"})
    check("tier3_overapplies_slash_pattern",
          "b.tmp" not in out_builtin and "a.tmp" not in out_builtin, out_builtin)
    check("tier3_ignores_info_exclude", "secret.txt" in out_builtin, out_builtin)

    # 3. builtin 強制の Rust は旧 Python 版とバイト一致（dirlens.py がある場合のみ）
    n_checks = 5
    if py:
        n_checks += 1
        out_py = run_tool(py, repo_root, env)
        check("tier3_python_parity", out_builtin == out_py)
    else:
        print("SKIP tier3_python_parity（dirlens.py なし。--py で指定可）")

    # 4. 非リポジトリでは自動縮退（builtin 強制と同一出力）
    out_plain_auto = run_tool(rust, plain_root, env)
    out_plain_builtin = run_tool(rust, plain_root, env, {"DIRLENS_GITIGNORE": "builtin"})
    same = out_plain_auto == out_plain_builtin
    if py:
        same = same and out_plain_auto == run_tool(py, plain_root, env)
    check("auto_degrades_without_git", same)

    print(f"\n結果: pass={n_checks - len(failures)} fail={len(failures)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
