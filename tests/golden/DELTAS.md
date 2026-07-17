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

### 6. トークン計数: BPE 2層化（spec §2 機能4 の発展・オーナー指示）
- 対象ケース: `-T` を含むすべて（`*_tokens` / `*_agent*` / `*_json_T` 等）
- 内容: Tier1 = tiktoken-rs（o200k_base・データはバイナリ同梱）による正確値。
  5MB 打ち切り時は Python 版と同じ比例補正で概算に戻る。
  Tier2 = 文字数ヒューリスティック（Python 版と同一式）。
  `DIRLENS_TOKENS=heuristic` または `DIRLENS_COMPAT=python`、
  および `tokens-bpe` feature 無効ビルド（wasm 等）では Tier2 に縮退し
  Python 版とバイト一致する。

### 7. テキスト出力の制御文字サニタイズ（セキュリティ強化・v1.1.1）
- 対象ケース: なし（既存フィクスチャは制御文字を含まないため、全ゴールデン・
  live 89/89 とも影響なし）
- 内容: ファイル名・シンボリックリンク先・TODO スニペット・git コミット件名・
  import パス等の攻撃者が制御しうる文字列に含まれる制御文字（Cc カテゴリ）を
  テキスト出力時に `?` へ置換する（エスケープシーケンス注入・ツリー行偽装の防止）。
  Python 版はサニタイズしないため、**制御文字を含む敵対的な入力に限り**
  出力が dirlens.py と一致しない。セキュリティ目的のため
  `DIRLENS_COMPAT=python` でも**無効化しない**（意図的な例外）。
  JSON は serde のエスケープ、HTML は html_escape で従来から安全。

### 8. `-L` 指定時の解析集計のプロジェクト全体化（v1.2.5）
- 対象ケース: `ml_agent_L2`
- 内容: `-L` で表示深さを絞っても、推定トークン数・拡張子別トークン・TODO 件数/
  サンプル・長大関数の集計は全階層スキャンの値を出す（Python 版は表示された
  分だけを集計しており、`missing_tests_count` 等の事前スキャン系集計と挙動が
  食い違っていた）。ツリー由来の集計（Total の dirs/files・拡張子出現数）は
  「表示されたもの」の集計として従来どおり。`DIRLENS_COMPAT=python` では
  Python 版と同じ表示分のみの集計に縮退する。

### 9. `--json` の `has_test` を判定対象外ファイルで null に（v1.2.5）
- 対象ケース: `ml_json_V` / `ml_agent_json` / `rs_agent_json` / `gi_agent_json`
- 内容: テスト欠落検知の対象外（`.py/.js/.jsx/.ts/.tsx/.go`、enhanced 時の
  `.rs` 以外）のファイルに `has_test: true` を返すと「テスト有り」に見える
  ため、null（判定対象外）を返す。`DIRLENS_COMPAT=python` では常時 bool。

### 10. HTML インライン `<script>` の JS アウトライン（v1.2.5）
- 対象ケース: なし（既存フィクスチャの .html にスクリプトが無いため出力不変。
  単体テスト `analysis::ast::js::tests::html_*` で検証）
- 内容: `.html`/`.htm` の src 属性なし・JS タイプの `<script>` ブロックを
  抽出し、oxc でアウトライン（行番号はファイル全体に合わせてオフセット。
  パース失敗ブロックは正規表現へ縮退）。AST 層のみの対応で、
  `DIRLENS_COMPAT=python` / `DIRLENS_AST=off` では従来どおり null。

### 11. 精度注記・capabilities への追記（v1.2.5）
- 対象ケース: `*_agent` / `*_agent_json` / `check_json`
- 内容: `--agent` 末尾の精度注記に「dir sizes=raw disk (gitignore not applied)」
  と outline 対応言語の「html(embedded js)」を追加。JSON では
  `capabilities.outline.html` と `analysis.dir_sizes` を追加（フィールド追加は
  後方互換・schema_version 据え置き）。

### 12. 位置引数が通常ファイルの場合の単一ファイルレポート（v1.2.5）
- 対象ケース: なし（フィクスチャは全てディレクトリ）
- 内容: `dirlens -O src/main.py` のように位置引数へファイルを渡すと、
  エラーではなく `--stdin` と同じ単一ファイルレポート（トークン・
  アウトライン・TODO）を返す。ドキュメントが以前から謳っていた挙動を
  実装に合わせた（Python 版はエラー）。

### 13. Rust import 解決のクレート境界対応と循環検出の改善（v1.2.7）
- 対象ケース: なし（rust_lang フィクスチャは Cargo.toml がスキャンルートに
  あるため出力不変。単体テスト `analysis::index::tests::{crate_of_nested_cargo,
  mod_key_is_crate_relative, module_map_separates_crates}` で検証）
- 内容: (1) Cargo.toml のあるディレクトリをクレート境界として検出し、
  `crate::`/`self::`/`super::` をクレート単位で解決する（モノレポ/
  ワークスペースのサブクレートに対応。従来はスキャンルート直下の `src/` 前提で、
  サブクレートのファイルを `--focus` すると依存元が過少に出た）。
  (2) `mod x;` 宣言のみのエッジ（use で参照されないもの）を循環依存の検出
  から除外する（lib.rs/mod.rs⇄子モジュールの往復が全て循環として報告される
  ノイズの解消。imports/imported_by/focus のグラフには残る）。
  (3) `--focus` で対象ファイルの上位にネストした tsconfig.json /
  jsconfig.json / package.json / go.mod がある場合、解決が不完全になりうる旨の
  注意書き（JSON では `note` フィールド）を付ける。
  いずれも enhanced 層のみ（`DIRLENS_COMPAT=python` / `DIRLENS_AST=off` では
  従来どおり）。

### 14. git 連携のサブディレクトリ・スキャン対応（v1.2.8）
- 対象ケース: なし（フィクスチャは git リポジトリではないため出力不変。
  単体テスト `analysis::gitlog::tests::{remap_strips_prefix_and_drops_outside,
  remap_noop_at_repo_root}` / `analysis::gitstatus::tests::{scan_relative_prefix,
  since_strips_repo_prefix}` で検証）
- 内容: git の出力パスはリポジトリルート相対だが、ツリー側はスキャンルート
  相対のため、リポジトリのサブディレクトリをスキャンルートに指定すると
  突き合わせがズレていた（Python 版から一貫したバグ）。症状: `-H` で
  コミット注釈がほぼ消え、同名ファイルがルートにあると**誤った履歴が黙って
  付く**・ホットスポットにスキャン対象外のファイルが混入 / `--since` が
  「変更なし」と空を返す / `--api-diff` が現存シンボル全削除（+0/-N）を誤報
  しスキャン対象外のファイルも混入 / `--status` のマークが付かない。
  `GitProvider::repo_prefix()`（`git rev-parse --show-prefix`）を追加し、
  git のパスをスキャンルート相対へ変換・対象外パスは除外するよう修正
  （`git show` へはリポジトリ相対のまま渡す）。`--since`/`--status`/
  `--api-diff` は Rust 版独自機能のため無条件に修正。`-H` のみ
  `DIRLENS_COMPAT=python` で Python 版と同じ変換なしの挙動に縮退する。

### 15. Python 公開 API 判定のスコープ対応（v1.2.9）
- 対象ケース: なし（multi_lang フィクスチャの .py にネスト定義・非公開クラスの
  メソッドが無いため出力不変。単体テスト `analysis::ast::python::tests::
  {local_defs_are_not_public, methods_follow_class_visibility,
  control_blocks_keep_module_scope}` で検証）
- 内容: Python の AST アウトラインで、関数内のローカル def / class とその
  メンバは `public: false`、クラスメソッドは「名前が `_` 始まりでなく、かつ
  属するクラス自身が公開」の場合のみ `public: true` とする（`if`/`try` 等の
  制御ブロック直下はモジュールレベル扱いのまま）。v1.2.8 以前は名前の
  `_` 始まりだけで判定しており、`-A`（公開 API モード）や `--api-diff` に
  関数内のローカル関数まで公開 API として過剰報告されていた。
  AST 層のみの変更で、`DIRLENS_COMPAT=python` / `DIRLENS_AST=off`
  （正規表現層）は従来どおり名前のみで判定する。

### 16. `--json` の情報付加フィールド（v1.2.9）
- 対象ケース: `json_L2` / `edge_json_T`（再記録済み）
- 内容: (1) `-L` の深さ打ち切りで `children` を省略したディレクトリに
  `"truncated": true` を付ける（`item_count` との照合を不要にする）。
  (2) 5MB 打ち切りの比例概算になったファイルのトークン数に
  `"tokens_estimated": true` を付ける（BPE 正確値と概算を区別可能にする）。
  (3) `--stdin` / 単一ファイルレポートの JSON に `errors` 配列を追加し、
  解決できなかった入力（不存在・ディレクトリ）を黙って落とさず
  `{path, error}` で報告する。重複パスは1回だけ処理する。
  (1)(2) は Python 版に無いフィールドのため `DIRLENS_COMPAT=python` では
  出さない（フィールド追加は後方互換・schema_version 据え置き）。
  (3) は Rust 版独自機能のため compat 対象外。

### 17. アウトラインの `parent`（外側シンボル名）と親付き表示（v1.2.10）
- 対象ケース: `ml_outline` / `ml_api` / `ml_agent` / `ml_agent_L2` /
  `ml_agent_json` / `ml_json_O` / `ml_json_A` / `rs_outline` / `rs_api` /
  `rs_agent` / `rs_agent_json`（再記録済み）
- 内容: ネストしたシンボルに直近の外側シンボル名を持たせる
  （クラスメソッド→クラス名、Rust の impl/trait 内 fn→型/トレイト名、
  関数内ローカル定義→関数名。treesitter 言語も直近の外側対象ノード名）。
  JSON では `parent` フィールド（compat では出さない）、テキスト表示・
  `--api-diff`・長大関数一覧では `def AppRunner.run` / `fn Thing::new` の
  親付き名になる。複数 impl の同名メソッド（`fn label` ×2 等）が
  区別できなかった問題の解消。AST 層のみで、正規表現層
  （`DIRLENS_COMPAT=python` / `DIRLENS_AST=off`）は従来どおり親なし。
  単体テスト `analysis::ast::python::tests::parent_is_nearest_enclosing_symbol`。

### 18. スキャンルート自体が gitignore 対象のときの注記（v1.2.10）
- 対象ケース: なし（gitignored フィクスチャのスキャンルートは ignore 対象外）
- 内容: `-G` 有効時にスキャンルート自体が gitignore 対象だと中身が全て
  隠れ「サイズは大きいのに空」という不可解な出力になるため、Tier1
  （`git check-ignore .`）で検出してテキスト末尾に注記、`--agent --json`
  の `errors` に `root_gitignored` を追加する。`DIRLENS_COMPAT=python`
  では出さない。MCP の `tree`/`analyze` には gitignore 除外を外す
  `include_ignored` パラメータを追加（コアの `Args.include_ignored`、
  CLI にフラグは無い＝ -G を付けなければ同じ）。

### 19. `--json` の `symlink` / `outline_method` フィールド（v1.2.11）
- 対象ケース: `json` / `json_J` / `json_L2` / `json_all` / `ml_agent_json` /
  `ml_json_O` / `ml_json_A` / `rs_agent_json` / `gi_agent_json`（再記録済み）
- 内容: (1) シンボリックリンクのファイルノードに
  `"symlink": {"target": …, "broken": bool}` を付ける（テキストの
  「name → target」矢印表示と同じ情報。broken = リンク先が存在しない
  dangling）。JSON だけリンクが普通のファイルに見えて dangling も判別
  できなかった問題の解消。(2) アウトラインを持つファイルに
  `"outline_method": "ast" | "regex"` を付ける（regex = 構文エラー等による
  縮退で取得漏れがありうることを機械可読に。`--stdin`／MCP `outline` の
  ファイルレポートにも付く）。どちらも Python 版に無いフィールドのため
  `DIRLENS_COMPAT=python` では出さない（フィールド追加は後方互換・
  schema_version 据え置き）。

### 20. 非通常ファイルのスキップ・api_diff の untracked・ルート外パス正規化（v1.2.12）
- 対象ケース: なし（フィクスチャに FIFO/untracked/ルート外参照は無い）
- 内容: (1) `StdFs::read_prefix` が通常ファイル以外（FIFO・ソケット・
  デバイス）を開かずに None を返す。FIFO は open(O_RDONLY) が書き手が
  現れるまでブロックするため、`mkfifo x.py` を含むディレクトリで
  全モードが永久ハングしていた（MCP では stdio サーバーごと固まる）。
  None は読めないファイルと同じ経路（tokens 0・outline なし）に乗る。
  Python 版も同様にハングするが「ハングしないこと」は parity 対象に
  しない（プロバイダ層の修正でコアの出力は不変）。単体テスト
  `providers::tests::read_prefix_skips_fifo`。
  (2) `--api-diff` が working tree の untracked ファイルを
  `git status --porcelain` の `??` から補完する（`--since` と対称に）。
  該当ファイルは `path (untracked)`／`path（未追跡）` の注記付きで
  全公開シンボルが追加扱い。untracked ディレクトリ（`dir/` に集約）は
  走査済みツリーで中のファイルへ展開する。Rust 版独自機能のため
  compat 対象外。
  (3) `--stdin`/`--pack`/`--focus` の入力パスがスキャンルート外のとき、
  従来は与えられた文字列をそのまま返していた（`inner/../x` のような
  未正規化表示）が、解決済みの絶対パスを返す。いずれも Rust 版独自
  機能のため compat 対象外。ルート外の読み取り自体は CLI と同じ
  ローカル権限で動くツールとして意図した仕様（MCP `outline` の
  説明文に明記）。

## 差分に該当しないもの（バイト一致を維持）

- tree 互換フラグ全般・テキスト/JSON/HTML の構造・サマリ行
- gitignore は両層が一致するパターンではバイト一致（差分が出るパターンは
  `tier_check.py` が仕様どおりであることを検証）
- 上記 1〜6 はすべて `DIRLENS_COMPAT=python` で無効化でき、その状態では
  全出力が dirlens.py とバイト一致する（live モードが常時検証。
  例外は 7 のサニタイズのみ＝制御文字を含む入力に限る）
