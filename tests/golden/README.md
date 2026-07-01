# ゴールデンテスト

Rust 版 dirlens が Python 版（`dirlens.py`）と機能的に等価であることをバイト単位で検証する。

```bash
# 1) 互換層の敵対的検証: Python 版とバイト一致（Rust 側は DIRLENS_COMPAT=python で実行される）
python3 tests/golden/run.py live --bin rust/target/debug/dirlens

# 2) 既定動作の固定: スナップショット照合（AST 層・git 層・精度注記込み）
python3 tests/golden/run.py verify --bin rust/target/debug/dirlens

# 3) スナップショットの再記録（--bin で Rust 既定動作から / 無指定で dirlens.py から）
#    Python 版との差分が変わる場合は必ず DELTAS.md を更新すること
python3 tests/golden/run.py record --bin rust/target/debug/dirlens

# 4) ハーネス自体の決定論性チェック / DELTAS 台帳の照合（documented 25 件だけ FAIL する）
python3 tests/golden/run.py verify --python

# 5) 改善が「実際に改善である」ことの敵対的検証
python3 tests/golden/tier_check.py --bin rust/target/debug/dirlens   # gitignore 2層
python3 tests/golden/ast_check.py  --bin rust/target/debug/dirlens   # AST 2段
```

- ケース定義: `cases.py`（90 ケース） / フィクスチャ生成: `fixtures.py`（実行毎に再構築・決定論）
- 意図的な Python 版との差分の台帳: `DELTAS.md`
- 決定論性の設計（mtime のバケット中央固定・git 日時の絶対固定＋正規化・
  GIT_CEILING_DIRECTORIES による外側リポジトリ遮断・PATH shim 等）は
  `run.py` / `fixtures.py` の冒頭コメントを参照
