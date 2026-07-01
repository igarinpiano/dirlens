#!/usr/bin/env python3
"""ゴールデンテストランナー。

モード:
  record            現行 dirlens.py の出力をスナップショットとして固定する
  verify --bin B    Rust バイナリの出力をスナップショットと照合する
  verify --python   dirlens.py 自身をスナップショットと照合する（ハーネスの決定論性検証）
  live   --bin B    dirlens.py と Rust バイナリを直接突き合わせる（敵対的検証）
  list              ケース一覧を表示

共通オプション: --only SUBSTR（ケースIDの部分一致で絞り込み）

決定論性のための環境固定:
  - TZ=UTC（Python の fmt_date は naive datetime 差分のため、DST の無い TZ に固定）
  - LC_ALL=C / LC_MESSAGES=C（git の相対日時等を英語に固定）
  - PATH は git だけを含む shim ディレクトリ（pbcopy 等を排除し -C の失敗を決定論化）
  - HOME / GIT_CONFIG_GLOBAL / GIT_CONFIG_SYSTEM を隔離
  - JSON 中のコミットハッシュは比較前に正規化する
"""
import argparse
import difflib
import os
import re
import shutil
import subprocess
import sys

# Windows のコンソール（cp1252 等）でも日本語メッセージを出せるようにする
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(HERE, ".work")
FIXTURES = os.path.join(WORK, "fixtures")
SNAPSHOTS = os.path.join(HERE, "snapshots")
DIRLENS_PY = os.path.join(REPO, "dirlens.py")

sys.path.insert(0, HERE)
from cases import CASES  # noqa: E402
import fixtures as fixtures_mod  # noqa: E402

_HASH_RE = re.compile(r'"hash": "[0-9a-f]{4,40}"')
# git --date=relative のラベルは実行時刻に依存するため正規化する
# （コミット日時自体は絶対固定。fixtures.py の _git 参照）。
_GITDATE_JSON_RE = re.compile(r'"date": "[^"]*"')
_GITDATE_TEXT_RE = re.compile(r'\(\d+ [a-z]+(?:, \d+ [a-z]+)? ago\)')


def normalize(text):
    if isinstance(text, bytes):
        text = text.decode("utf-8", errors="replace")
    text = text.replace("\r\n", "\n")
    text = _HASH_RE.sub('"hash": "<HASH>"', text)
    text = _GITDATE_JSON_RE.sub('"date": "<GITDATE>"', text)
    return _GITDATE_TEXT_RE.sub('(<GITDATE>)', text)


def build_env():
    home = os.path.join(WORK, "home")
    os.makedirs(home, exist_ok=True)
    # PATH は git だけを含む shim（pbcopy 等を排除し -C を決定論化）。
    # Windows は symlink が使えない場合があるためフル PATH のまま
    # （live 比較は同一環境の Python vs Rust なので決定論性は保たれる）。
    if os.name == "nt":
        path = os.environ.get("PATH", "")
    else:
        shim = os.path.join(WORK, "shim")
        os.makedirs(shim, exist_ok=True)
        git_path = shutil.which("git")
        shim_git = os.path.join(shim, "git")
        if git_path and not os.path.exists(shim_git):
            os.symlink(git_path, shim_git)
        path = shim
    return {
        "PATH": path,
        "HOME": home,
        "USERPROFILE": home,
        "LC_ALL": "C",
        "LANG": "C",
        "LC_MESSAGES": "C",
        "TZ": "UTC",
        "PYTHONIOENCODING": "utf-8",
        "PYTHONHASHSEED": "0",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
        # git リポジトリ探索が .work より上（＝外側の dirlens リポジトリ）へ
        # 遡らないようにする。これが無いと fixture の git 情報が外側リポジトリの
        # 履歴で汚染され、ゴールデンが dirlens のコミットのたびに変わってしまう。
        "GIT_CEILING_DIRECTORIES": WORK,
    }


def requirements_ok(reqs):
    for r in reqs:
        if r == "unixperm":
            if os.name != "posix" or os.geteuid() == 0:
                return "unixperm 要件を満たさない（posix かつ非 root が必要）"
        elif r == "git":
            if not shutil.which("git"):
                return "git が見つからない"
        elif r == "symlink":
            if not fixtures_mod.can_symlink():
                return "symlink が作れない環境"
    return None


def run_case(cmd_prefix, case, env, tag, extra_env=None):
    fixture_path = os.path.join(FIXTURES, case["fixture"])
    outdir = os.path.join(WORK, "out", tag, case["id"])
    shutil.rmtree(outdir, ignore_errors=True)
    os.makedirs(outdir)
    argv = list(cmd_prefix) + [fixture_path] + case["args"]
    run_env = dict(env)
    if extra_env:
        run_env.update(extra_env)
    proc = subprocess.run(argv, capture_output=True, cwd=outdir, env=run_env,
                          timeout=120)
    files = {}
    for name in sorted(os.listdir(outdir)):
        p = os.path.join(outdir, name)
        if os.path.isfile(p):
            with open(p, "rb") as f:
                files[name] = normalize(f.read())
    return {
        "out": normalize(proc.stdout),
        "err": normalize(proc.stderr),
        "code": proc.returncode,
        "files": files,
    }


# ─── スナップショット入出力 ───────────────────────────────────
def snap_dir(case):
    return os.path.join(SNAPSHOTS, case["fixture"])


def write_snapshot(case, res):
    d = snap_dir(case)
    os.makedirs(d, exist_ok=True)
    base = os.path.join(d, case["id"])
    for stale in list_snapshot_files(case):
        os.remove(stale)
    with open(base + ".out", "w", encoding="utf-8") as f:
        f.write(res["out"])
    if res["err"]:
        with open(base + ".err", "w", encoding="utf-8") as f:
            f.write(res["err"])
    if res["code"] != 0:
        with open(base + ".code", "w", encoding="utf-8") as f:
            f.write(str(res["code"]))
    for name, content in res["files"].items():
        with open(f"{base}.file.{name}", "w", encoding="utf-8") as f:
            f.write(content)


def list_snapshot_files(case):
    d = snap_dir(case)
    if not os.path.isdir(d):
        return []
    out = []
    for n in os.listdir(d):
        stem = n.split(".", 1)[0] if "." in n else n
        if n == case["id"] + ".out" or n == case["id"] + ".err" or \
           n == case["id"] + ".code" or n.startswith(case["id"] + ".file."):
            out.append(os.path.join(d, n))
    return out


def read_snapshot(case):
    base = os.path.join(snap_dir(case), case["id"])
    if not os.path.isfile(base + ".out"):
        return None
    res = {"out": "", "err": "", "code": 0, "files": {}}
    with open(base + ".out", encoding="utf-8") as f:
        res["out"] = f.read()
    if os.path.isfile(base + ".err"):
        with open(base + ".err", encoding="utf-8") as f:
            res["err"] = f.read()
    if os.path.isfile(base + ".code"):
        with open(base + ".code", encoding="utf-8") as f:
            res["code"] = int(f.read().strip())
    prefix = case["id"] + ".file."
    for n in os.listdir(snap_dir(case)):
        if n.startswith(prefix):
            with open(os.path.join(snap_dir(case), n), encoding="utf-8") as f:
                res["files"][n[len(prefix):]] = f.read()
    return res


# ─── 比較 ─────────────────────────────────────────────────────
def diff_text(expected, actual, label, max_lines=60):
    lines = list(difflib.unified_diff(
        expected.splitlines(keepends=True), actual.splitlines(keepends=True),
        fromfile=f"expected/{label}", tofile=f"actual/{label}"))
    if len(lines) > max_lines:
        lines = lines[:max_lines] + [f"... (diff truncated, {len(lines)} lines total)\n"]
    return "".join(lines)


def compare(case, expected, actual):
    problems = []
    if expected["code"] != actual["code"]:
        problems.append(f"exit code: expected {expected['code']}, got {actual['code']}")
    if expected["out"] != actual["out"]:
        problems.append(diff_text(expected["out"], actual["out"], "stdout"))
    if expected["err"] != actual["err"]:
        problems.append(diff_text(expected["err"], actual["err"], "stderr"))
    if set(expected["files"]) != set(actual["files"]):
        problems.append(f"生成ファイルが不一致: expected {sorted(expected['files'])}, "
                        f"got {sorted(actual['files'])}")
    else:
        for name in expected["files"]:
            if expected["files"][name] != actual["files"][name]:
                problems.append(diff_text(expected["files"][name],
                                          actual["files"][name], f"file:{name}"))
    return problems


# ─── メイン ───────────────────────────────────────────────────
def select_cases(only, mode, has_bin):
    out = []
    for case in CASES:
        if only and only not in case["id"]:
            continue
        if case["live_only"] and mode != "live":
            continue
        if case.get("rust_only"):
            # dirlens.py に無い機能: live 比較不可、record/verify は Rust 版のみ
            if mode == "live" or not has_bin:
                continue
        out.append(case)
    return out


def main():
    ap = argparse.ArgumentParser(description="dirlens ゴールデンテストランナー")
    ap.add_argument("mode", choices=["record", "verify", "live", "list"])
    ap.add_argument("--bin", help="Rust バイナリのパス")
    ap.add_argument("--python", action="store_true",
                    help="verify で dirlens.py 自身を照合する（決定論性チェック）")
    ap.add_argument("--only", help="ケースIDの部分一致フィルタ")
    ap.add_argument("--skip", action="append", default=[],
                    help="fixture 名 or ケースIDの部分一致で除外（複数可）。"
                         "例: CI の Linux で .git のバイト数が環境依存になる "
                         "gitignored の verify を外す")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    if args.mode == "list":
        for case in CASES:
            flags = " [live_only]" if case["live_only"] else ""
            print(f"{case['fixture']}/{case['id']}: {' '.join(case['args'])}{flags}")
        return 0

    cases = select_cases(args.only, args.mode, bool(args.bin))
    cases = [c for c in cases
             if not any(s in c["id"] or s == c["fixture"] for s in args.skip)]
    if not cases:
        print("該当するケースがありません", file=sys.stderr)
        return 1

    print("フィクスチャを構築中...", flush=True)
    fixtures_mod.build_all(FIXTURES)
    env = build_env()

    py_cmd = [sys.executable, DIRLENS_PY]
    rust_cmd = [os.path.abspath(args.bin)] if args.bin else None

    n_pass = n_fail = n_skip = 0
    failures = []
    for case in cases:
        cid = f"{case['fixture']}/{case['id']}"
        reason = requirements_ok(case["requires"])
        if reason:
            n_skip += 1
            if args.verbose:
                print(f"SKIP {cid}: {reason}")
            continue

        try:
            if args.mode == "record":
                # 既定は dirlens.py から記録。--bin 指定時は Rust 版（既定動作）から記録する
                # （AST 層・精度注記など意図的な差分を取り込む際に使う。差分は
                # tests/golden/DELTAS.md に記録すること）。
                src_cmd = rust_cmd if rust_cmd else py_cmd
                res = run_case(src_cmd, case, env, "rec")
                write_snapshot(case, res)
                n_pass += 1
                if args.verbose:
                    print(f"REC  {cid}")
                continue

            if args.mode == "verify":
                if args.python:
                    actual = run_case(py_cmd, case, env, "py")
                elif rust_cmd:
                    actual = run_case(rust_cmd, case, env, "rs")
                else:
                    print("verify には --bin か --python が必要です", file=sys.stderr)
                    return 2
                expected = read_snapshot(case)
                if expected is None:
                    n_skip += 1
                    print(f"SKIP {cid}: スナップショット未記録")
                    continue
            else:  # live
                if not rust_cmd:
                    print("live には --bin が必要です", file=sys.stderr)
                    return 2
                # live = Python 互換パス（正規表現層・内蔵 gitignore）の敵対的検証。
                # 既定動作（AST 層・git 層・精度注記）は verify（スナップショット）が担う。
                expected = run_case(py_cmd, case, env, "py")
                actual = run_case(rust_cmd, case, env, "rs",
                                  extra_env={"DIRLENS_COMPAT": "python"})

            problems = compare(case, expected, actual)
            if problems:
                n_fail += 1
                failures.append((cid, problems))
                print(f"FAIL {cid}")
            else:
                n_pass += 1
                if args.verbose:
                    print(f"PASS {cid}")
        except subprocess.TimeoutExpired:
            n_fail += 1
            failures.append((cid, ["タイムアウト"]))
            print(f"FAIL {cid} (timeout)")

    print(f"\n結果: pass={n_pass} fail={n_fail} skip={n_skip}")
    for cid, problems in failures:
        print(f"\n════ {cid} ════")
        for p in problems:
            print(p)
    return 1 if n_fail else 0


if __name__ == "__main__":
    sys.exit(main())
