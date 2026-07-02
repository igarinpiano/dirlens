#!/usr/bin/env python3
"""npm パッケージ群の組み立てスクリプト（リリース CI から呼ぶ）。

入力: --version X.Y.Z --binaries <dir>
  <dir>/ 配下に「<target>/dirlens(.exe)」の形で機種別バイナリが置かれている想定:
    aarch64-apple-darwin/dirlens
    x86_64-apple-darwin/dirlens
    aarch64-unknown-linux-gnu/dirlens
    x86_64-unknown-linux-gnu/dirlens
    x86_64-pc-windows-msvc/dirlens.exe

出力: --out <dir> に本体パッケージ dirlens/ と機種別パッケージ dirlens-bin-*/ を生成する。
公開は所有者が手動で行う（CI は dry-run / artifacts の生成までに留める）。
"""
import argparse
import json
import os
import shutil
import stat

TARGETS = {
    "aarch64-apple-darwin": ("dirlens-bin-darwin-arm64", ["darwin"], ["arm64"], "dirlens"),
    "x86_64-apple-darwin": ("dirlens-bin-darwin-x64", ["darwin"], ["x64"], "dirlens"),
    "aarch64-unknown-linux-gnu": ("dirlens-bin-linux-arm64", ["linux"], ["arm64"], "dirlens"),
    "x86_64-unknown-linux-gnu": ("dirlens-bin-linux-x64", ["linux"], ["x64"], "dirlens"),
    "x86_64-pc-windows-msvc": ("dirlens-bin-win32-x64", ["win32"], ["x64"], "dirlens.exe"),
}

DESCRIPTION = "ファイルサイズ・AI/エージェント解析つきディレクトリツリー表示ツール（tree 互換）"
REPO = "github:igarinpiano/dirlens"


def write_json(path, obj):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)
        f.write("\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--binaries", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    here = os.path.dirname(os.path.abspath(__file__))
    repo_root = os.path.dirname(os.path.dirname(here))
    os.makedirs(args.out, exist_ok=True)

    optional = {}
    for target, (pkg, os_list, cpu_list, exe) in TARGETS.items():
        src = os.path.join(args.binaries, target, exe)
        if not os.path.isfile(src):
            print(f"skip {target}（バイナリなし）")
            continue
        pdir = os.path.join(args.out, pkg)
        os.makedirs(os.path.join(pdir, "bin"), exist_ok=True)
        dst = os.path.join(pdir, "bin", exe)
        shutil.copy2(src, dst)
        os.chmod(dst, os.stat(dst).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        write_json(os.path.join(pdir, "package.json"), {
            "name": pkg,
            "version": args.version,
            "description": f"dirlens の {target} バイナリ",
            "repository": REPO,
            "license": "Apache-2.0",
            "os": os_list,
            "cpu": cpu_list,
            "files": ["bin/"],
        })
        optional[pkg] = args.version

    main_dir = os.path.join(args.out, "dirlens")
    os.makedirs(os.path.join(main_dir, "bin"), exist_ok=True)
    shutil.copy2(os.path.join(here, "launcher.js"),
                 os.path.join(main_dir, "bin", "dirlens.js"))
    for doc in ["README.md", "LICENSE", "NOTICE", "AGENT_RULE.md", "AGENT_RULE_STRICT.md"]:
        src = os.path.join(repo_root, doc)
        if os.path.isfile(src):
            shutil.copy2(src, os.path.join(main_dir, doc))
    write_json(os.path.join(main_dir, "package.json"), {
        "name": "dirlens",
        "version": args.version,
        "description": DESCRIPTION,
        "keywords": ["tree", "directory", "cli", "filesize", "ai", "agent"],
        "repository": REPO,
        "license": "Apache-2.0",
        "bin": {"dirlens": "bin/dirlens.js"},
        "files": ["bin/", "README.md", "AGENT_RULE.md", "AGENT_RULE_STRICT.md",
                  "LICENSE", "NOTICE"],
        "optionalDependencies": optional,
    })
    print(f"assembled: dirlens + {len(optional)} platform packages -> {args.out}")


if __name__ == "__main__":
    main()
