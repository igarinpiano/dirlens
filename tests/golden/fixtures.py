#!/usr/bin/env python3
"""ゴールデンテスト用フィクスチャ生成。

実行のたびに同一内容のツリーを <work>/fixtures/ 配下に再構築する。

決定論性の設計:
- mtime は「現在時刻 - バケット中央のオフセット」で設定する。fmt_date() の相対表示
  （「3ヶ月前」等）が実行時刻に依存せず安定する（バケット境界から十分離れた値を選ぶ）。
- git コミットの author/committer date も同様に相対オフセットで与える
  （--date=relative の出力が安定する）。コミットハッシュは実行毎に変わるため、
  ランナー側で JSON 中の "hash" フィールドを正規化する。
- ファイルは chmod 0644 / ディレクトリは 0755 に明示的に揃える（umask 非依存、-p 用）。
- <work>/.git（空ディレクトリ）をバリアとして置き、フィクスチャ（gitignored 以外）が
  外側の dirlens リポジトリの git 履歴を拾わないようにする。
"""
import os
import shutil
import stat
import subprocess
import sys
import time

NOW = time.time()

# fmt_date() のバケット中央オフセット（境界からのスラックを十分確保）
AGE_90MIN = 90 * 60          # 「1時間前」（スラック ±30分）
AGE_30H = 30 * 3600          # 「昨日」
AGE_4D = 4 * 86400           # 「4日前」
AGE_10D = 10 * 86400         # 「1週間前」
AGE_100D = 100 * 86400       # 「3ヶ月前」
AGE_400D = 400 * 86400       # 「1年前」
DEFAULT_AGE = AGE_100D


def _set_mtime(path, age):
    t = NOW - age
    os.utime(path, (t, t), follow_symlinks=True)


def w(root, rel, data, age=DEFAULT_AGE):
    """テキスト/バイナリファイルを書き込み、mtime とパーミッションを固定する。"""
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if isinstance(data, str):
        data = data.encode("utf-8")
    with open(path, "wb") as f:
        f.write(data)
    os.chmod(path, 0o644)
    _set_mtime(path, age)
    return path


def can_symlink():
    """symlink が作れるか（Windows は開発者モード無効だと不可）。"""
    global _CAN_SYMLINK
    if _CAN_SYMLINK is None:
        probe = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                             ".work", ".symlink_probe")
        os.makedirs(os.path.dirname(probe), exist_ok=True)
        try:
            if os.path.lexists(probe):
                os.remove(probe)
            os.symlink("target", probe)
            os.remove(probe)
            _CAN_SYMLINK = True
        except OSError:
            _CAN_SYMLINK = False
    return _CAN_SYMLINK


_CAN_SYMLINK = None


def link(root, rel, target):
    if not can_symlink():
        return
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    os.symlink(target, path)


def finalize_dirs(root, age=DEFAULT_AGE):
    """全ディレクトリの mtime/パーミッションを最後にまとめて固定する（bottom-up）。"""
    for dirpath, dirnames, _ in os.walk(root, topdown=False):
        for d in dirnames:
            p = os.path.join(dirpath, d)
            if os.path.islink(p):
                continue
            os.chmod(p, 0o755)
            _set_mtime(p, age)
    os.chmod(root, 0o755)
    _set_mtime(root, age)


# ─── basic: tree 互換機能の検証用 ─────────────────────────────
def build_basic(root):
    w(root, "README.md", "# basic fixture\n" + "line of text\n" * 12, AGE_100D)
    w(root, "empty.txt", "", AGE_90MIN)
    w(root, "single.b", "x", AGE_30H)
    w(root, "app.log", "log entry\n" * 20, AGE_10D)
    w(root, "notes.txt", "note\n" * 100, AGE_4D)
    # casefold ソート検証: alpha < Beta < Zeta
    w(root, "Zeta.txt", "z" * 1536, AGE_400D)          # 1.5 KB
    w(root, "alpha.txt", "a" * 1024, AGE_100D)         # 1 KB
    w(root, "Beta.md", "b" * 1587, AGE_100D)           # 1.55 KB
    w(root, "big.bin", bytes(range(256)) * 1200, AGE_100D)  # 300 KB
    w(root, ".hidden_file", "secret\n", AGE_100D)
    w(root, ".hiddendir/inner.txt", "hidden inner\n", AGE_100D)
    w(root, "sub/child.py", "def hello():\n    return 'hi'\n" * 8, AGE_4D)
    w(root, "sub/grandchild/deep.txt", "deep content\n" * 30, AGE_100D)
    w(root, "sub/grandchild/deeper/leaf.md", "leaf\n", AGE_100D)
    os.makedirs(os.path.join(root, "emptydir"), exist_ok=True)
    w(root, "links/real.txt", "real file target\n", AGE_100D)
    link(root, "links/to_file", "../README.md")
    link(root, "links/to_dir", "../sub")
    link(root, "links/broken", "./nonexistent")
    # -l の循環リンク検出用
    os.makedirs(os.path.join(root, "loop/inner"), exist_ok=True)
    w(root, "loop/inner/file.txt", "in loop\n", AGE_100D)
    link(root, "loop/inner/back", "../../loop")
    finalize_dirs(root)


# ─── multi_lang: AI/エージェント解析機能の検証用 ──────────────
def build_multi_lang(root):
    w(root, "package.json",
      '{\n  "name": "demo",\n  "version": "1.0.0",\n'
      '  "main": "src/index.js",\n  "bin": {"demo": "src/cli.js"}\n}\n')
    w(root, "tsconfig.json",
      '{\n  "compilerOptions": {\n    "strict": true\n  }\n}\n')
    w(root, ".env.example", "API_KEY=xxx\n")
    w(root, "pyproject.toml", '[project]\nname = "demo"\nversion = "1.0.0"\n')
    w(root, "Makefile", "all:\n\techo build\n")
    w(root, "Dockerfile", "FROM python:3.12\nCOPY . /app\n")
    w(root, "main.py",
      "#!/usr/bin/env python3\n"
      '"""demo entrypoint"""\n'
      "import os\n"
      "import pkg.utils\n"
      "from pkg.helpers import public_fn\n"
      "\n"
      "# TODO: コマンドライン引数のパースを追加する\n"
      "def main():\n"
      "    print(public_fn())\n"
      "\n"
      "def _internal():\n"
      "    pass\n"
      "\n"
      "class AppRunner:\n"
      "    def run(self):\n"
      "        pass\n"
      "\n"
      "if __name__ == '__main__':\n"
      "    main()\n")
    w(root, "pkg/__init__.py", "from . import helpers\n")
    w(root, "pkg/helpers.py",
      "import json\n"
      "\n"
      "def public_fn():\n"
      "    # FIXME: エラーハンドリングが未実装\n"
      "    return 1\n"
      "\n"
      "def _private_fn():\n"
      "    return 2\n"
      "\n"
      "class Helper:\n"
      "    async def fetch(self):\n"
      "        pass\n"
      "\n"
      "class _HiddenHelper:\n"
      "    pass\n")
    w(root, "pkg/utils.py",
      "from .helpers import public_fn\n"
      "from . import helpers\n"
      "import sys\n"
      "\n"
      "def util_fn(x):\n"
      "    return public_fn() + x  # HACK: 暫定実装。この行はとても長いコメントで八十文字の切り詰め動作を検証するためのものです\n")
    w(root, "pkg/circle_a.py",
      "from .circle_b import beta\n\ndef alpha():\n    return beta\n")
    w(root, "pkg/circle_b.py",
      "from .circle_a import alpha\n\ndef beta():\n    return alpha\n")
    w(root, "tests/test_utils.py",
      "from pkg.utils import util_fn\n\ndef test_util_fn():\n    assert util_fn(1) == 2\n")
    w(root, "src/index.js",
      "import { helper } from './lib/util';\n"
      "import React from 'react';\n"
      "const data = require('./lib/data');\n"
      "\n"
      "export default class App {\n"
      "  render() { return null; }\n"
      "}\n"
      "\n"
      "export const bootstrap = async () => {\n"
      "  // TODO add error boundary\n"
      "  return import('./cli');\n"
      "};\n")
    w(root, "src/cli.js",
      "#!/usr/bin/env node\n"
      "const { helper } = require('./lib/util');\n"
      "\n"
      "// XXX: exit code handling\n"
      "function run() {\n"
      "  helper();\n"
      "}\n"
      "run();\n")
    w(root, "src/lib/util.ts",
      "import { Kind } from './types';\n"
      "import lodash from 'lodash';\n"
      "\n"
      "export function helper(): Kind {\n"
      "  return { name: 'x' };\n"
      "}\n"
      "\n"
      "function internalOnly() {}\n"
      "\n"
      "export const arrowExported = (a: number) => a + 1;\n"
      "const arrowLocal = (b: number) => b - 1;\n")
    w(root, "src/lib/types.ts",
      "export class Kind {\n  name = '';\n}\n\nexport const DEFAULT_KIND = (\n  new Kind()\n);\n")
    w(root, "src/lib/data.js", "module.exports = { rows: [] };\n")
    w(root, "src/app.ts",
      "import { helper } from './lib/util';\n\nexport function appMain() {\n  helper();\n}\n")
    w(root, "src/app.test.ts",
      "import { appMain } from './app';\n\ntest('app', () => appMain());\n")
    w(root, "go/go.mod", "module example.com/demo\n\ngo 1.22\n")
    w(root, "go/main.go",
      "package main\n\n"
      'import (\n\t"fmt"\n\n\t"example.com/demo/internal/util"\n)\n\n'
      "type Config struct {\n\tName string\n}\n\n"
      "type Runner interface {\n\tRun() error\n}\n\n"
      "func main() {\n\tfmt.Println(util.Public())\n}\n\n"
      "func helperLocal() {}\n")
    w(root, "go/internal/util/util.go",
      "package util\n\n"
      'import "strings"\n\n'
      "// TODO: add caching\n"
      "func Public() string {\n\treturn strings.ToUpper(\"ok\")\n}\n\n"
      "func private() {}\n")
    w(root, "docs/日本語メモ.md",
      "# 日本語のドキュメント\n\nこれはトークン数概算のためのテキストです。"
      "日本語の文字は一文字あたりのトークン数が多めに見積もられます。\n" * 5)
    finalize_dirs(root)


# ─── rust_lang: Rust の outline / import 解決検証用 ───────────
def build_rust_lang(root):
    w(root, "Cargo.toml", '[package]\nname = "demo"\nversion = "0.1.0"\n')
    w(root, "src/main.rs",
      "mod util;\nmod nested;\n\n"
      "use crate::util::Thing;\nuse std::fmt;\n\n"
      "struct Config {\n    name: String,\n}\n\n"
      "pub fn run() -> Thing {\n    Thing::new()\n}\n\n"
      "fn main() {\n    // TODO: implement CLI parsing\n    run();\n}\n")
    w(root, "src/util.rs",
      "pub struct Thing;\n\n"
      "impl Thing {\n    pub fn new() -> Self { Thing }\n}\n\n"
      "pub trait Runner {\n    fn go(&self);\n}\n\n"
      "enum Mode {\n    Fast,\n    Slow,\n}\n\n"
      "pub(crate) async fn helper() {}\n\nfn private_helper() {}\n")
    w(root, "src/nested/mod.rs", "pub mod inner;\n")
    w(root, "src/nested/inner.rs",
      "use crate::util::Thing;\n\npub fn deep_fn() -> Thing {\n    Thing\n}\n")
    finalize_dirs(root)


# ─── gitignored: -G / -H の検証用（実 git リポジトリ） ─────────
def _git_env(root):
    env = dict(os.environ)
    env.update({
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
        "HOME": root,
        "USERPROFILE": root,
        "GIT_AUTHOR_NAME": "Tester",
        "GIT_AUTHOR_EMAIL": "tester@example.com",
        "GIT_COMMITTER_NAME": "Tester",
        "GIT_COMMITTER_EMAIL": "tester@example.com",
    })
    return env


def _git(root, *args, when=None):
    # コミット日時は「絶対時刻」で固定する。相対時刻にすると .git 配下のオブジェクトの
    # バイト列（zlib圧縮長）が実行ごとに揺れ、ルート合計サイズが非決定論になる。
    # 代わりに --date=relative の出力ラベルはランナー側で正規化する（run.py 参照）。
    env = _git_env(root)
    if when is not None:
        d = f"@{when} +0000"
        env["GIT_AUTHOR_DATE"] = d
        env["GIT_COMMITTER_DATE"] = d
    subprocess.run(["git", "-C", root, *args], check=True, env=env,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def build_gitignored(root):
    # 内蔵マッチャ（Tier3）と git check-ignore（Tier1）の両方で同じ結果になる
    # パターンだけを使う（両層の差分は別途の敵対ケースで検証する）。
    w(root, ".gitignore",
      "*.log\n"
      "!important.log\n"
      "build/\n"
      "temp*\n")
    w(root, "README.md", "# gitignored fixture\n", AGE_10D)
    w(root, "app.log", "ignored log\n")
    w(root, "important.log", "kept by negation\n")
    w(root, "build/out.txt", "build output\n")
    w(root, "temp1.txt", "temporary\n")
    w(root, "temporary_notes.md", "temp prefix\n")
    # 注意: コミットごとに mtime を変える（サイズも mtime も同一だと git の
    # stat キャッシュが変更を検知せず「nothing to commit」になる）。
    w(root, "src/main.py",
      "def main():\n    return 0\n", AGE_10D)
    w(root, "src/deep.log", "nested log ignored\n")
    w(root, "nested/.gitignore", "local.txt\n/anchored.txt\n")
    w(root, "nested/local.txt", "ignored by nested gitignore\n")
    w(root, "nested/anchored.txt", "ignored by anchored pattern\n")
    w(root, "nested/kept.txt", "visible\n")
    w(root, "nested/sub/local.txt", "ignored recursively\n")
    w(root, "nested/sub/other.txt", "visible\n")
    finalize_dirs(root)

    # 空テンプレートで init する（hooks/*.sample は git バージョン依存のバイト数を持ち、
    # ルート合計サイズを機械依存にしてしまうため）。
    template = os.path.join(os.path.dirname(root), ".git_template_empty")
    os.makedirs(template, exist_ok=True)
    _git(root, "init", "-q", "-b", "main", "--template", template)
    _git(root, "add", ".gitignore", "README.md", "src/main.py",
         "important.log", "nested/kept.txt")
    _git(root, "commit", "-q", "-m",
         "feat: initial import of project skeleton and configs",
         when=1735689600)  # 2025-01-01T00:00:00Z
    w(root, "src/main.py", "def main():\n    return 1\n", AGE_4D)
    _git(root, "add", "src/main.py")
    _git(root, "commit", "-q", "-m",
         "refactor: reorganize module layout for clarity",
         when=1738368000)  # 2025-02-01T00:00:00Z
    w(root, "src/main.py", "def main():\n    return 2\n", AGE_90MIN)
    w(root, "README.md", "# gitignored fixture v2\n", AGE_90MIN)
    _git(root, "add", "src/main.py", "README.md")
    _git(root, "commit", "-q", "-m",
         "日本語のコミットメッセージで切り詰め動作を確認する",
         when=1740960000)  # 2025-03-03T00:00:00Z
    # コミットで .git 配下が更新されるので mtime を再固定する
    finalize_dirs(root)


# ─── edge: エッジケース検証用 ─────────────────────────────────
def build_edge(root):
    # 5MB 打ち切り + スケール補正（6MB → 5MB 読み込み → ×6/5 補正）
    line = b"0123456789abcdef" * 4 + b"\n"      # 65 bytes
    w(root, "big6mb.txt", line * 92308)          # 5,999,, ~6MB
    w(root, "cjk.md",
      "日本語のテキストです。トークン数の概算では非ASCII文字は1.5文字で1トークンと数えます。\n" * 40)
    w(root, "binary_in_txt.txt", b"\x00\x01\x02BINARY DATA" + b"\xff" * 100)
    w(root, "image.png", b"\x89PNG\r\n\x1a\n" + b"\x00" * 500)
    w(root, "crlf.txt", b"line one\r\nline two\r\n")
    w(root, "only_newline.txt", "\n")
    w(root, "invalid_utf8.txt", b"caf\xc3\xa9 \xff\xfe end\n")
    w(root, "space name.txt", "has space\n")
    w(root, "日本語ファイル.md", "# 日本語名\n")
    if os.name != "nt":  # < > は Windows のファイル名では不正
        w(root, "a&b<c>.html", "<p>escape me</p>\n")
    w(root, "'quote'.txt", "quoted\n")
    w(root, "[bracket].txt", "brackets in name\n")
    w(root, "chain/a/b/c/d/e/f/leaf.txt", "deep chain\n")
    w(root, "denied/secret.txt", "cannot read\n")
    w(root, "todo_many.py",
      "".join(f"# TODO: item number {i} needs work\n" for i in range(12)))
    finalize_dirs(root)
    os.chmod(os.path.join(root, "denied"), 0o000)


BUILDERS = {
    "basic": build_basic,
    "multi_lang": build_multi_lang,
    "rust_lang": build_rust_lang,
    "gitignored": build_gitignored,
    "edge": build_edge,
}


def _force_rmtree(path):
    """chmod 000 のディレクトリを含んでいても削除できるようにする。"""
    if not os.path.exists(path):
        return
    for dirpath, dirnames, _ in os.walk(path):
        for d in dirnames:
            p = os.path.join(dirpath, d)
            try:
                os.chmod(p, 0o755)
            except OSError:
                pass
    shutil.rmtree(path)


def build_all(dest):
    os.makedirs(dest, exist_ok=True)
    # バリア: フィクスチャが外側の dirlens リポジトリの git 履歴を拾わないようにする。
    # 空の .git ディレクトリは「壊れたリポジトリ」として git をエラーにする。
    barrier = os.path.join(os.path.dirname(dest), ".git")
    os.makedirs(barrier, exist_ok=True)
    for name, builder in BUILDERS.items():
        root = os.path.join(dest, name)
        _force_rmtree(root)
        os.makedirs(root)
        builder(root)
    return dest


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else \
        os.path.join(os.path.dirname(os.path.abspath(__file__)), ".work", "fixtures")
    build_all(out)
    print(f"fixtures built at {out}")
