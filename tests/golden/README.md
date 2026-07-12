# ゴールデンテスト

Rust 版 dirlens の出力を固定スナップショットで検証する。スナップショットは
旧 Python 版（`dirlens.py`・現在は `python` ブランチ）の出力を基に、
spec が命じる改善のみ意図的に更新したもの（台帳: `DELTAS.md`）。

> **Swift 版（`swift` ブランチ）**も同じスナップショット・同じランナーで検証する。
> `--bin swift/.build/release/dirlens` を渡すだけでよい。意図的な差分の台帳は
> `SWIFT-DELTAS.md`（verify で 11 件だけ FAIL するのが正常。live は 89/89 一致）。
>
> ```bash
> swift build -c release --package-path swift
> python3 tests/golden/run.py verify --bin swift/.build/release/dirlens
> python3 tests/golden/run.py live   --bin swift/.build/release/dirlens --py /tmp/dirlens.py
> python3 tests/golden/tier_check.py --bin swift/.build/release/dirlens
> python3 tests/golden/ast_check.py  --bin swift/.build/release/dirlens
> ```

```bash
# 1) 既定動作の照合: スナップショット（AST 層・git 層・BPE・精度注記込み）
python3 tests/golden/run.py verify --bin rust/target/debug/dirlens

# 2) スナップショットの再記録（Rust 既定動作から）。
#    出力が意図的に変わる場合は必ず DELTAS.md を更新すること
python3 tests/golden/run.py record --bin rust/target/debug/dirlens

# 3) 改善が「実際に改善である」ことの敵対的検証
python3 tests/golden/tier_check.py --bin rust/target/debug/dirlens   # gitignore 2層
python3 tests/golden/ast_check.py  --bin rust/target/debug/dirlens   # AST 2段

# 4) 旧 Python 版との互換検証（dirlens.py は python ブランチから取得して指定）
git show python:dirlens.py > /tmp/dirlens.py
#    互換層のバイト一致（Rust 側は DIRLENS_COMPAT=python で実行される）:
python3 tests/golden/run.py live --bin rust/target/debug/dirlens --py /tmp/dirlens.py
#    DELTAS 台帳の照合（台帳どおりの 28 件だけ FAIL するのが正常）:
python3 tests/golden/run.py verify --python --py /tmp/dirlens.py
```

- ケース定義: `cases.py`（90 ケース） / フィクスチャ生成: `fixtures.py`（実行毎に再構築・決定論）
- 意図的な Python 版との差分の台帳: `DELTAS.md`
- 決定論性の設計（mtime のバケット中央固定・git 日時の絶対固定＋正規化・
  GIT_CEILING_DIRECTORIES による外側リポジトリ遮断・PATH shim 等）は
  `run.py` / `fixtures.py` の冒頭コメントを参照
