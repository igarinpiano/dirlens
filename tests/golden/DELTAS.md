# Python 版との意図的な差分（DELTAS）

ゴールデンスナップショット（`snapshots/`）は当初 `dirlens.py` の出力から記録し、
その後 **spec（artifacts/rust-rewrite-spec.md）が命じる改善**に該当する箇所のみ
Rust 版既定動作（`record --bin`）で意図的に更新している。このファイルはその差分の台帳である。

検証の分担:

| モード | 何を保証するか |
|---|---|
| `live --bin`（Rust 側に `DIRLENS_COMPAT=python`） | 移植した互換ロジック（正規表現層・内蔵 gitignore）が dirlens.py と**バイト一致** |
| `verify --bin`（既定動作） | Rust 版の既定動作（AST 層・git 層・精度注記込み）がスナップショットと一致 |
| `tier_check.py` / `ast_check.py` | 改善が「実際に改善である」ことの敵対的検証 |

`verify --python` は以下の差分ぶんだけ失敗する（それが台帳と一致していること自体が検証になる）。

## 差分一覧

### 1. アウトライン: AST 第1段による正規表現偽陽性の解消（spec §2 機能1）
- 対象ケース: `ml_outline` / `ml_api` / `ml_agent` / `ml_agent_json` / `ml_json_O` / `ml_json_A`
- 内容: `export const DEFAULT_KIND = (…)` は関数ではないため、AST 層では
  `func DEFAULT_KIND` を出力しない（正規表現層は `export const X = (` を関数と誤検出する）。
- 環境変数 `DIRLENS_COMPAT=python` または `DIRLENS_AST=off` で正規表現層に固定すれば
  Python 版とバイト一致する。

### 2. `--agent` テキスト末尾の精度注記（spec §2 機能5）
- 対象ケース: `*_agent`
- 内容: 出力末尾に「  解析方式: gitignore=… / outline=… / imports=… / tokens=…」の 1 行を追加。

### 3. `--json` の `schema_version`（spec §8）
- 対象ケース: すべての `--json` 系
- 内容: トップレベル先頭に `"schema_version": 1` を追加。フィールド追加は後方互換、
  改名・削除・型変更時にインクリメントする。

### 4. `--agent --json` の `capabilities` / `analysis` メタブロック（spec §2 機能5）
- 対象ケース: `*_agent_json`
- 内容: トップレベル末尾に `capabilities`（このビルド・環境で使える方式）と
  `analysis`（この実行で実際に使った方式）を追加。

### 5. `--check`（新規機能・spec §2 機能5）
- 対象ケース: `check` / `check_json` / `gi_check`（rust_only）
- 内容: dirlens.py には存在しない能力レポート。縮退があると終了コード 1。

## 差分に該当しないもの（バイト一致を維持）

- tree 互換フラグ全般・テキスト/JSON/HTML の構造・サマリ行・-T の概算値
  （トークン概算式は Python 版と同一。精緻化はゴールデン一致と矛盾するため見送り、
  tiktoken による正確値は将来の opt-in feature とする）
- gitignore は両層が一致するパターンではバイト一致（差分が出るパターンは
  `tier_check.py` が仕様どおりであることを検証）
