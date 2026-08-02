#!/usr/bin/env python3
"""npm パッケージ群の組み立てスクリプト（リリース CI から呼ぶ）。

入力: --version X.Y.Z --binaries <dir>
  <dir>/ 配下に「<target>/dirlens(.exe)」の形で機種別バイナリが置かれている想定
  （reusable-build-matrix.yml がビルドする全ターゲット。TARGETS に無いものは
  npm パッケージ化されない＝ GitHub Releases の生バイナリのみで配布される。
  対象を絞っているのは Node.js が公式バイナリを配っていない os/cpu の組み合わせ
  （armv7 の 32bit linux-arm・i686 系・riscv64gc）に npm パッケージを作っても
  インストールする Node.js ランタイム自体が存在せず無意味なため）。

出力: --out <dir> に本体パッケージ dirlens/ と機種別パッケージ dirlens-bin-*/ を生成する。
公開は所有者が手動で行う（CI は dry-run / artifacts の生成までに留める）。
"""
import argparse
import json
import os
import shutil
import stat

# target -> (npm パッケージ名, os[], cpu[], libc[] または None, 実行ファイル名)
# os/cpu は Node.js の process.platform/process.arch にそのまま対応する値。
# libc は package.json の "libc" フィールド（npm 9+ が対応。musl 版と glibc 版の
# 両方が os/cpu だけでは区別できない linux x64/arm64 でのみ指定する。既定
# （libc 指定なし）は「どの libc でも対象」＝ glibc 版で、Alpine 等 musl ホストは
# launcher.js のランタイム検出で musl 版を優先させる）。
TARGETS = {
    "aarch64-apple-darwin": ("dirlens-bin-darwin-arm64", ["darwin"], ["arm64"], None, "dirlens"),
    "x86_64-apple-darwin": ("dirlens-bin-darwin-x64", ["darwin"], ["x64"], None, "dirlens"),
    "aarch64-unknown-linux-gnu": ("dirlens-bin-linux-arm64", ["linux"], ["arm64"], None, "dirlens"),
    "x86_64-unknown-linux-gnu": ("dirlens-bin-linux-x64", ["linux"], ["x64"], None, "dirlens"),
    "aarch64-unknown-linux-musl": ("dirlens-bin-linux-arm64-musl", ["linux"], ["arm64"], ["musl"], "dirlens"),
    "x86_64-unknown-linux-musl": ("dirlens-bin-linux-x64-musl", ["linux"], ["x64"], ["musl"], "dirlens"),
    "powerpc64le-unknown-linux-gnu": ("dirlens-bin-linux-ppc64", ["linux"], ["ppc64"], None, "dirlens"),
    "s390x-unknown-linux-gnu": ("dirlens-bin-linux-s390x", ["linux"], ["s390x"], None, "dirlens"),
    "x86_64-pc-windows-msvc": ("dirlens-bin-win32-x64", ["win32"], ["x64"], None, "dirlens.exe"),
    "aarch64-pc-windows-msvc": ("dirlens-bin-win32-arm64", ["win32"], ["arm64"], None, "dirlens.exe"),
}

DESCRIPTION = "ファイルサイズ・AI/エージェント解析つきディレクトリツリー表示ツール（tree 互換）"
REPO = "git+https://github.com/igarinpiano/dirlens.git"


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
    for target, (pkg, os_list, cpu_list, libc_list, exe) in TARGETS.items():
        src = os.path.join(args.binaries, target, exe)
        if not os.path.isfile(src):
            print(f"skip {target}（バイナリなし）")
            continue
        pdir = os.path.join(args.out, pkg)
        os.makedirs(os.path.join(pdir, "bin"), exist_ok=True)
        dst = os.path.join(pdir, "bin", exe)
        shutil.copy2(src, dst)
        os.chmod(dst, os.stat(dst).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        pkg_json = {
            "name": pkg,
            "version": args.version,
            "description": f"dirlens の {target} バイナリ",
            "repository": REPO,
            "license": "Apache-2.0",
            "os": os_list,
            "cpu": cpu_list,
            "files": ["bin/"],
        }
        if libc_list:
            pkg_json["libc"] = libc_list
        write_json(os.path.join(pdir, "package.json"), pkg_json)
        optional[pkg] = args.version

    main_dir = os.path.join(args.out, "dirlens")
    os.makedirs(os.path.join(main_dir, "bin"), exist_ok=True)
    shutil.copy2(os.path.join(here, "launcher.js"),
                 os.path.join(main_dir, "bin", "dirlens.js"))
    for doc in ["README.md", "LICENSE", "NOTICE", "AGENT_RULE.md", "AGENT_RULE_STRICT.md",
                "AGENT_RULE_MCP.md"]:
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
                  "AGENT_RULE_MCP.md", "LICENSE", "NOTICE"],
        "optionalDependencies": optional,
    })
    print(f"assembled: dirlens + {len(optional)} platform packages -> {args.out}")


if __name__ == "__main__":
    main()
