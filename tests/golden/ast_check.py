#!/usr/bin/env python3
"""AST 2段（第1段=言語別パーサ / 第2段=正規表現）の敵対的検証。

「AST 層が実際に改善である」ことと「失敗時に正規表現へ縮退する」ことを、
既定動作（AST）と DIRLENS_COMPAT=python（正規表現強制）の出力比較で確認する。

使い方: python3 ast_check.py --bin <rust binary>
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
WORK = os.path.join(HERE, ".work", "ast")

sys.path.insert(0, HERE)
from run import build_env  # noqa: E402


def build_fixture(root):
    shutil.rmtree(root, ignore_errors=True)
    os.makedirs(root)

    def w(rel, content):
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(content)

    # 1. 文字列リテラル内の偽シンボル（正規表現は誤検出する）
    w("strings.py",
      'DOC = """\ndef fake_in_string(x):\n    pass\n"""\n\n'
      "def real_fn():\n    return DOC\n")
    # 2. 構文エラー → AST 失敗 → 正規表現へ縮退してもシンボルは出る
    w("broken.py", "def works_by_regex(:\n    this is not python\n")
    # 3. tsconfig paths エイリアス解決
    w("tsconfig.json",
      '{\n  // JSONC コメントも許容される\n'
      '  "compilerOptions": {\n    "baseUrl": ".",\n'
      '    "paths": {"@lib/*": ["src/lib/*"]}\n  },\n}\n')
    w("src/lib/util.ts", "export function helper(): number { return 1; }\n")
    w("src/a.ts", "import { helper } from '@lib/util';\nexport const go = () => helper();\n")
    # 4. package.json imports（# エイリアス）
    w("package.json",
      '{"name": "x", "imports": {"#helpers": "./src/helpers.js"}}\n')
    w("src/helpers.js", "module.exports = {};\n")
    w("src/b.js", "const h = require('#helpers');\n")
    # 5. Rust super:: 解決
    w("Cargo.toml", '[package]\nname = "x"\nversion = "0.1.0"\n')
    w("src/util.rs", "pub struct Helper;\n")
    w("src/main.rs", "mod util;\nmod worker;\nfn main() {}\n")
    w("src/worker.rs", "use super::util::Helper;\npub fn work() -> Helper { Helper }\n")
    # 6. JS 文字列内の偽 import（正規表現は phantom-pkg を拾う）
    w("src/c.js", "const s = \"import x from 'phantom-pkg'\";\nconsole.log(s);\n")


def run_json(bin_path, root, env, compat=False):
    e = dict(env)
    if compat:
        e["DIRLENS_COMPAT"] = "python"
    proc = subprocess.run([bin_path, root, "-O", "-M", "--json"],
                          capture_output=True, text=True, env=e, timeout=60)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr)
    return json.loads(proc.stdout)


def index_files(tree, acc=None):
    if acc is None:
        acc = {}
    for ch in tree.get("children", []):
        if ch["type"] == "file":
            acc[ch["path"]] = ch
        else:
            index_files(ch, acc)
    return acc


def outline_names(f):
    return [o["name"] for o in (f.get("outline") or [])]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    args = ap.parse_args()
    bin_path = os.path.abspath(args.bin)

    env = build_env()
    root = os.path.join(WORK, "fixture")
    build_fixture(root)

    ast = index_files(run_json(bin_path, root, env, compat=False))
    rex = index_files(run_json(bin_path, root, env, compat=True))

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"PASS {name}")
        else:
            failures.append(name)
            print(f"FAIL {name}\n  {detail}")

    # 1. 文字列内の偽シンボル: AST は拾わない / 正規表現は拾う
    check("ast_ignores_string_literal_defs",
          "fake_in_string" not in outline_names(ast["strings.py"])
          and "real_fn" in outline_names(ast["strings.py"]),
          str(ast["strings.py"].get("outline")))
    check("regex_tier_has_string_fp",
          "fake_in_string" in outline_names(rex["strings.py"]),
          str(rex["strings.py"].get("outline")))

    # 2. 構文エラー → 正規表現へ縮退（既定動作でもシンボルが出る）
    check("parse_error_falls_back_to_regex",
          "works_by_regex" in outline_names(ast["broken.py"]),
          str(ast["broken.py"].get("outline")))

    # 3. tsconfig paths: @lib/util がローカル解決される
    check("tsconfig_paths_resolved",
          ast["src/a.ts"]["imports"] == ["src/lib/util.ts"],
          str(ast["src/a.ts"]))
    check("tsconfig_paths_external_in_compat",
          rex["src/a.ts"]["imports"] == []
          and "@lib/util" in rex["src/a.ts"]["external_imports"],
          str(rex["src/a.ts"]))

    # 4. package.json imports: #helpers がローカル解決される
    check("package_imports_resolved",
          ast["src/b.js"]["imports"] == ["src/helpers.js"],
          str(ast["src/b.js"]))

    # 5. Rust super:: 解決
    check("rust_super_resolved",
          "src/util.rs" in ast["src/worker.rs"]["imports"],
          str(ast["src/worker.rs"]))
    check("rust_super_external_in_compat",
          "src/util.rs" not in rex["src/worker.rs"]["imports"],
          str(rex["src/worker.rs"]))

    # 6. JS 文字列内の偽 import: AST は拾わない
    check("ast_ignores_string_literal_imports",
          "phantom-pkg" not in ast["src/c.js"]["external_imports"],
          str(ast["src/c.js"]))
    check("regex_tier_has_import_fp",
          "phantom-pkg" in rex["src/c.js"]["external_imports"],
          str(rex["src/c.js"]))

    print(f"\n結果: pass={10 - len(failures)} fail={len(failures)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
