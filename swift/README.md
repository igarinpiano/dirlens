# dirlens — Swift 実装（実験的）

Rust 版 dirlens（`rust/`）を **Swift** へ全面移植したもの。CLI 仕様・出力
フォーマット・`--json` スキーマは Rust 版と互換（検証方法は下記）。

## ビルド

```bash
cd swift
swift build -c release          # → .build/release/dirlens
```

- 要件: Swift 6 系ツールチェーン（macOS 13+）。Xcode 本体は不要（Command Line Tools で可）。
- **外部 SwiftPM パッケージへの依存はゼロ**（Foundation のみ）。
- BPE 語彙（`Sources/DirlensCore/Resources/o200k_base.tiktoken`）は SwiftPM リソースとして
  実行ファイルの隣の `dirlens_DirlensCore.bundle` に同梱される。バイナリを配布する場合は
  bundle も一緒に置くこと（見つからない環境では文字数ヒューリスティックへ自動縮退する）。

## テスト

```bash
# ユニットテスト（swift-testing・27 本）
swift test

# Command Line Tools のみの環境（Xcode 無し）では Testing.framework の配置の都合で
# 補助スクリプトを使う:
./scripts/test-clt.sh

# ゴールデンテスト（リポジトリルートで。詳細は tests/golden/README.md）
python3 ../tests/golden/run.py verify --bin .build/release/dirlens   # 79/90（残り 11 件は台帳 SWIFT-DELTAS.md の対象）
python3 ../tests/golden/run.py live   --bin .build/release/dirlens --py /tmp/dirlens.py  # 89/89 バイト一致
python3 ../tests/golden/tier_check.py --bin .build/release/dirlens   # 6/6
python3 ../tests/golden/ast_check.py  --bin .build/release/dirlens   # 10/10
```

## アーキテクチャ

Rust 版の構成（コア/プラットフォーム分離）をそのまま踏襲する:

- **`DirlensCore`**（ライブラリ）: 純粋な解析ロジック。FS / git / クリップボード /
  外部 AST ツールはすべてプロトコル（`FsProvider` / `GitProvider` /
  `ClipboardProvider` / `AstProvider`）経由で受け取り、Foundation の
  FileManager / Process を直接呼ばない。
- **`dirlens`**（実行ファイル）: プロトコルの POSIX 実装（opendir / lstat /
  posix_spawn）、clap 相当の引数パーサ、外部 AST ツールの常駐コプロセス管理。

## 解析の 3 層構成（Rust 版との最大の違い）

Rust 版は言語パーサ（ruff / oxc / syn / tree-sitter）をバイナリに同梱していた。
Swift 版は「外部依存なし」を保ったまま同等以上の精度を出すために 3 層にする:

| 層 | 方式 | 使う条件 |
|---|---|---|
| Tier1 `ast` | 外部ツール: **python3 の stdlib `ast`**（= CPython そのもの）/ **node + 対象プロジェクトの node_modules 内 typescript** | 実行時に見つかったとき（常駐コプロセスで高速化） |
| Tier1.5 `scanner` | **内蔵の構文走査**: 言語別の字句規則でコメント除去＋文字列リテラル空白化した「コード限定ビュー」を作ってから抽出 | 既定（ゼロ依存） |
| Tier2 `regex` | dirlens.py 互換の正規表現 | `DIRLENS_AST=off` / `DIRLENS_COMPAT=python` |

- scanner 層は AST 層の主要な利点（**文字列リテラル内の偽シンボル・偽 import を
  拾わない**、`export const X = (…)` を関数と誤検出しない、Rust の
  `use a::{b, c}` グループ展開）を再現しており、ゴールデンフィクスチャ上では
  Rust 版 AST 層と同じ結果を返す（`ast_check.py` が敵対的に検証）。
- 外部ツールはパース失敗・起動失敗・応答タイムアウトで自動的に scanner へ縮退する。
  `DIRLENS_EXTERNAL=off` で外部ツールだけを無効化できる。
- どの層が使われるかは `--check` / `--agent --json` の `capabilities` で機械可読に分かる。

### トークン計数（-T）

o200k_base の **BPE を純 Swift で実装**し、語彙（tiktoken 由来・MIT）をリソース同梱。
tiktoken-rs と同値を返す（ゴールデンで検証済み。5MB 打ち切り時の比例補正も同じ）。
`DIRLENS_TOKENS=heuristic` で Python 版と同一の文字数ヒューリスティックに固定できる。

### gitignore（-G）2 層

Rust 版と同一: Tier1 = `git check-ignore --stdin -z`（レベルごとに一括投入・厳密）、
git 不在/非 work tree では内蔵マッチャ（fnmatch 近似）へ縮退。
`DIRLENS_GITIGNORE=builtin` で内蔵層を強制。

## Swift 版で追加した機能

- **Swift 言語サポート**（Rust 版・Python 版には無い）:
  - アウトライン: `func` / `class` / `struct` / `enum` / `protocol` / `actor` /
    `extension` / `typealias`（`public` / `open` を公開判定）
  - `import` 抽出（モジュール単位のため external 扱い）
  - `-V`: XCTest 慣習（`FooTests.swift` / `FooTest.swift`）の検出
  - `-N`: `main.swift` / `-F`: `Package.swift`・`Package.resolved`
- `capabilities.external_tools`（外部ツール可用性）の機械可読な報告

## 環境変数（Rust 版と共通＋追加）

| 変数 | 効果 |
|---|---|
| `DIRLENS_COMPAT=python` | Python 版完全互換（regex 層・内蔵 gitignore・ヒューリスティック・注記なし） |
| `DIRLENS_GITIGNORE=builtin\|git` | gitignore 層の強制 |
| `DIRLENS_AST=off` | scanner/外部 AST を無効化（regex 層のみ） |
| `DIRLENS_TOKENS=heuristic` | BPE を使わない |
| `DIRLENS_EXTERNAL=off` | 外部ツール（python3 / node+ts）だけを無効化（scanner は使う）※Swift 版のみ |
| `DIRLENS_BPE=off` | 語彙リソース欠落時の縮退を再現（検証用）※Swift 版のみ |

## 既知の制限・Rust 版との差

- 対応プラットフォームは macOS / Linux（POSIX）。Windows は未対応（Rust 版を使うこと）。
- `--agent` の実測は Rust 版の約 1.7 倍の実行時間（BPE の正規表現分割が主因）。
  Python 版よりは大幅に高速。
- 単一ファイル配布ではなく「実行ファイル + リソース bundle」の 2 点構成
  （bundle が無い場合はトークン計数のみ縮退）。
- 出力の意図的な差分は `tests/golden/SWIFT-DELTAS.md` を参照（11 ケース・
  すべて精度注記/capabilities/--check のみ。ツリー本体はバイト一致）。
