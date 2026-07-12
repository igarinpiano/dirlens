# Swift 版と Rust 版スナップショットの意図的な差分（SWIFT-DELTAS）

`swift` ブランチの Swift 実装は、`snapshots/`（Rust 版既定動作から記録）に対して
`run.py verify --bin swift/.build/release/dirlens` を実行すると
**以下の 11 ケースだけが FAIL する**。それが台帳と一致していること自体が検証になる
（`DELTAS.md` が Python 版→Rust 版で行ったのと同じ運用）。

検証の分担:

| モード | 何を保証するか | 結果 |
|---|---|---|
| `live --bin`（Swift 側に `DIRLENS_COMPAT=python`） | 互換層（正規表現・内蔵 gitignore・ヒューリスティックトークン）が dirlens.py と**バイト一致** | 89/89 |
| `verify --bin`（既定動作） | Swift 版の既定動作が Rust 版スナップショットと一致（台帳の 11 件を除く） | 79/90 |
| `tier_check.py` | gitignore 2層の層選択・縮退が正しい | 6/6 |
| `ast_check.py` | 構造走査層が「実際に改善である」こと（文字列内偽シンボル排除等） | 10/10 |
| `swift test` | コア関数の単体検証（BPE 正確値・fnmatch・循環検出 10 万ノード等） | 27/27 |

## 差分一覧（全 11 ケース・差分は注記/メタブロックのみ）

### 1. `--agent` テキスト末尾の精度注記の文言
- 対象: `ml_agent` / `ml_agent_L2` / `rs_agent` / `gi_agent` / `edge_agent`（5 件）
- 内容: 末尾 1 行の「解析方式: …」のみが異なる。**ツリー本体・アウトライン・
  import 解決・トークン数・TODO 等はすべてバイト一致**。
  - Rust: `outline=AST:py,js/ts,rs,go,c(他は正規表現) / imports=AST+マニフェスト解決`
  - Swift: `outline=構文走査:py,js/ts,rs,go,c,swift / imports=構文走査+マニフェスト解決`
    （外部ツール可用時は `AST:py・構文走査:…` / `AST+マニフェスト解決` に変わる）
- 理由: Swift 版はパーサをバイナリに同梱せず、
  「外部ツール（python3 / node+typescript）→ 内蔵の構文走査 → 正規表現」の
  3 層で解析するため、実行環境に応じた正直な注記を出す。

### 2. `--agent --json` の `capabilities` / `analysis` メタブロック
- 対象: `ml_agent_json` / `rs_agent_json` / `gi_agent_json`（3 件）
- 内容:
  - `capabilities.outline.*` の値が `"ast"` → `"scanner"`（外部ツール可用時は `"ast"`）。
    `"swift"` キーを追加（Swift 言語サポートの追加による。フィールド追加 = 後方互換）。
  - `capabilities.external_tools`（`python3` / `node_typescript` の可用性）を追加。
  - `analysis.outline` が `"ast+regex-fallback"` → `"scanner+regex-fallback"`
    （外部ツール可用時 `"ast+scanner+regex-fallback"`）、
    `analysis.imports` が `"ast+manifest"` → `"scanner+manifest"`（同 `"ast+manifest"`）。
- `schema_version` は 1 のまま（追加・値語彙の拡張のみで、既存フィールドの
  改名・削除・型変更はない）。

### 3. `--check` の能力レポート
- 対象: `check` / `check_json` / `gi_check`（3 件）
- 内容: Swift 版の 3 層構成（ast / scanner / regex）と外部ツール可用性を
  反映したレポートに変更。Swift 言語の行と「外部ツール:」行を追加。
  外部ツールが見つからない環境では縮退扱い（終了コード 1）になる点も Rust 版
  （すべて同梱なので常に best）と異なる。

## 差分に該当しないもの（Rust 版スナップショットとバイト一致を維持）

- tree 互換フラグ全般・テキスト/JSON/HTML の構造・サマリ行・HTML テンプレート
- **BPE トークン数（o200k_base）**: 純 Swift 実装＋同梱語彙で tiktoken-rs と同値
  （`*_tokens` / `*_json_T` / `edge_tokens`（5MB 打ち切り比例補正含む）すべて一致）
- **アウトライン / import 解決**: ゴールデンフィクスチャ上では内蔵の構文走査が
  Rust 版 AST 層（ruff/oxc/syn/tree-sitter）と同じ結果を返す
  （`ml_outline` の `DEFAULT_KIND` 非関数判定・`rs_imports` の use 展開・
  Go の import ブロック等を含む）
- gitignore 2層・`-H`・`-K`・`-V`・`-N`・`-F` の全出力

## Swift 版で追加された挙動（スナップショットに影響しないもの）

- **Swift 言語サポート**: `.swift` のアウトライン（func/class/struct/enum/protocol/
  actor/extension/typealias・public/open 判定）、`import` 抽出（external 扱い）、
  `-V` の XCTest 慣習（`FooTests.swift`）、`main.swift` エントリ検出、
  `Package.swift` / `Package.resolved` の設定ファイル検出。
  フィクスチャに .swift ファイルが無いためゴールデンには現れない。
- `is_test_file` に `stem.endswith("tests")` を追加（XCTest 慣習）。
