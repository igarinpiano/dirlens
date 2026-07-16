# dirlens エージェント向けルール（厳格版）

このプロジェクトのコード探索を始める前に、`dirlens --agent` を実行してプロジェクト全体の構造を把握してください。

## 基本ルール

1. **調査の最初の一手として `dirlens --agent` を実行する**

   ファイルツリー・サイズ・最終更新日時に加え、推定トークン数（BPE 正確値）、最終コミット情報、git status マーク（`[M]`/`[??]` 等）、TODO/FIXME、テスト未整備ファイル、エントリーポイント候補、関数/クラスのアウトライン、import依存関係、設定ファイル一覧、言語別トークン内訳、長大関数を一度に取得できる。`ls`/`find`/`grep` を繰り返すより少ない往復で全体像を掴める。

   `--agent` はANSIカラーコードを自動的に無効化する（`--no-color` を兼ねる）ため、エージェント出力やログとしてそのまま扱える。出力はデフォルトで英語（`--lang ja` で日本語）。

2. **構造化データが必要な場合は `dirlens --agent --json` を使う**

   パース可能なJSON形式で同じ情報を取得できる。`project_summary` にプロジェクト全体の集計、`language_breakdown` に言語別内訳、`longest_functions` に長大関数、`errors` に部分的に取得できなかった情報（git不在等）が機械可読で入る。出力は部分的な解析失敗があっても常に valid な JSON。

3. **コンテキストが逼迫しているときは `--estimate` → `--budget N` を使う**

   ```bash
   dirlens --agent --estimate       # 階層別の出力コスト（-L 1 / -L 2 / … / 全階層）を数行で見積もる
   dirlens --agent --budget 3000    # 出力を指定トークン数（o200k BPE 実測）以内に自動調整
   ```

   `--budget` は深さ→解析注釈→ツリー行の順に削って必ず予算内に収める。収まらなかった分は「… N more entries (omitted by --budget)」と省略され、末尾に実測トークン数と「この階層を全て表示するのに必要なトークン数」が付記されるので、予算を増やすかどうかをそこで判断できる。

4. **変更の影響範囲を調べるときは `--focus` を使う**

   ```bash
   dirlens --focus src/cfg.rs -G     # このファイルの依存元/依存先（直接+推移）
   dirlens --focus src/cfg.rs -G --json
   ```

   「このファイルを変更したら何が壊れうるか」が import グラフの推移閉包で一発で分かる。

5. **セッション途中の再確認には `--since` / `--stdin` で差分だけ取る**

   ```bash
   dirlens --since HEAD -G                        # 前回コミット以降に変更されたファイルだけのツリー
   git diff --name-only | dirlens --stdin --json  # 変更ファイルだけのトークン数・アウトライン・TODO
   ```

   全ツリーを再出力するより大幅にトークンを節約できる。

6. **個別の情報だけが必要なら単体フラグを使う**

   - `dirlens -O <path>` — 特定ファイルの関数/クラス一覧（JSON では doc 1行目・行範囲つき。単一ファイルのみ。複数ファイルは `--stdin` か MCP の `outline` を使う）
   - `dirlens -A` — プロジェクト全体の公開API（公開シンボルのみ）
   - `dirlens -M` — import依存グラフ（循環依存の検出つき。`--mermaid`/`--dot` で図の出力も可）
   - `dirlens -H` — 最近のコミット履歴・ホットスポット
   - `dirlens -V` — テストが不足しているファイル（テストからの import も追跡・Rust対応）
   - `dirlens -K` — TODO/FIXMEの棚卸し
   - `dirlens -N` — エントリーポイント候補
   - `dirlens -F` — 設定ファイル（.env, tsconfig.json等）の一覧
   - `dirlens --api-diff <ref>` — 公開APIの git ref との差分（破壊的変更の検出）
   - `dirlens --status` — git status をツリーに重ねて表示
   - `dirlens --top 10` — 大きいファイル/ディレクトリ上位だけフラット表示

   単体フラグ（`-V`/`-K`/`-N`/`-F`/`-A`等）の出力は「該当ファイルだけの一覧」ではなく「**全ツリーに該当マーカーを注釈した表示**」である（例: `-V` は全ファイルのうち該当分に `no test` が付く）。出力サイズはプロジェクト規模に比例するため、大きなリポジトリでは `-L`/`--budget`/`-G` と組み合わせること（`--budget`/`--estimate` は単体フラグにもそのまま効く）。フラット出力は `--top N` のみ。

7. **`--ai` はエージェントでは使わない**

   `--ai` はクリップボードへのコピーを伴うため、エージェントの自律実行では不要かつ副作用になりうる。エージェントは常に `--agent` を使う。

8. **`-L` はツリー表示の深さのみを制限する。解析集計値はプロジェクト全体を反映する（v1.2.5+）**

   `dirlens --agent -L 2` のように深さを指定した場合、ツリーの「見た目」は指定階層までに制限されるが、TODO件数・推定トークン数・言語別内訳・長大関数・テスト未整備数・エントリーポイント数などの解析集計値はプロジェクト全体のスキャン結果を反映する。巨大なリポジトリでざっくり把握したいときは `-L 2` 等を使ってよい。例外は `Total N directories, M files` と拡張子出現数（`.rs ×18` 等）で、これらは「表示されたツリー」の集計（`tree` コマンド互換の意味論）。
   v1.2.4 以前は TODO 件数・トークン集計・長大関数が表示深さに引きずられて過少になるバグがあった（`-L` を使う場合はこれらの数値を信用しないこと）。

9. **巨大なリポジトリでは `-G` の併用を確認する**

   `--agent` は自動的に `-G`（.gitignore除外）を有効にするが、個別フラグ（`-V`/`-N`/`-M`/`-K`/`-F`/`--focus` 等）を単体で使う場合は `-G` を明示的に付けないと `node_modules` 等まで走査して遅くなることがある。なお2回目以降の実行はトークン計数キャッシュにより速くなる（`DIRLENS_CACHE=off` で無効化可）。

10. **MCP ホストでは `--mcp` でネイティブツールとして登録できる**

    ```bash
    dirlens --mcp-setup                              # ホスト別の登録手順（絶対パス入り）を表示
    claude mcp add dirlens -s user -- dirlens --mcp  # Claude Code ならこの1行
    ```

    analyze / tree / outline / imports / focus / todos / since / history / api_diff の9ツール（v1.2.2+。v1.2.1以前は前6つのみ）が使えるようになり、シェル経由の往復が不要になる。GUI ホスト（Claude Desktop 等）はシェル PATH を継がないため、`--mcp-setup` が出力する絶対パス入りの設定を使うこと。`-s user` を付けないと local scope になり、登録時のカレントディレクトリのプロジェクトにしか有効にならない点に注意。

    CLI との対応と MCP 固有の注意点:

    - `analyze` = `--agent --json`。大きなプロジェクトでは JSON が巨大化してホスト側の出力上限に当たるため、`estimate: true` でコスト見積もり → `budget: N` で予算内に収める（`budget` 指定時は JSON ではなく注釈付きテキストが返る）。`estimate` は実際に使われる出力フォーマット（`budget` 未指定なら JSON、指定時はテキスト）を実測して見積もる（v1.2.4+。それ以前は常にテキストで測っていたため、JSON 出力時に見積もりが実サイズより大幅に小さく出る既知の問題があった）。**見積もりが正確でも、多くの MCP ホストは 1 応答に上限がある（Claude Code の既定は 25,000 トークン）**。使いたい階層の見積もりが上限を超えるなら、上限未満の `budget`（例: 20000）で呼ぶこと。v1.2.6+ は見積もり表の該当行に `⚠ exceeds host cap` マークが付き、末尾に上限値（`MAX_MCP_OUTPUT_TOKENS` があればその値、無ければ Claude Code 既定の 25000）と推奨 `budget` が表示される。`tree` にも `budget` と `top`（大きいファイルのフラット表示）がある
    - `outline` は **複数ファイルを配列で一括処理できる**（CLI の `-O` は単一ファイルのみ）。`files` を省略するとプロジェクト全体の公開 API（`-A` 相当）になる。相対パスは `path` 基準で解決される
    - `outline`（`files` 省略時）と `history` は、呼び出し側が `depth` を省略すると自動的に小さい既定値（それぞれ 2 / 1）に制限される（全ツリー走査で応答が肥大化するのを防ぐため）。全階層が必要な場合は `depth` を明示するか `unlimited_depth: true` を渡す（`depth` を指定した場合はそちらが常に優先され、`unlimited_depth` は無視される）
    - `history` = `-H`（既定で深さ1のコンパクトなテキスト。ホットスポット一覧は深さに依らず全体を反映）
    - `api_diff` = `--api-diff <ref>`（破壊的変更の検出）
    - `imports` と `todos` は CLI の `-M`/`-K` と異なり、**該当なしファイルを含まないフラットな一覧**を返す（v1.2.4+。それ以前は全ファイルに空配列を注釈した全ツリーを返しており、大きいプロジェクトで最もトークンを消費するツールだった）。`imports` は `{path, imports, imported_by, external_imports}` の配列＋`most_depended_on`/`circular_dependencies`、`todos` は `{path, line, kind, text}` の配列＋`todo_count`。どちらも `limit` で件数上限を指定できる。`imports` はさらに `format: "mermaid"` / `"dot"` で図の出力も可（この場合は全体のグラフをそのまま返すため `limit` は無視される）
    - MCP サーバーは実装レベルでクリップボード無効（`NoClipboard` 固定）のため、`capabilities.clipboard: false` は異常ではない
    - MCP に**無い**もの: `--pack` / `--compare` / `--dupes` / `--heat` / `--csv` / stdin パイプ。必要ならシェルで CLI を直接使う

---

## 出力の正確さについて（重要）

dirlens の解析は「最良の方式 → 縮退」の多層構成であり、実際に使われた方式は `--check` や `--agent --json` の `capabilities` / `analysis` ブロックで機械的に確認できる。以下の制限を理解して使うこと。

| 機能 | 方式と制限事項 |
|---|---|
| トークン数（`-T`） | BPE（o200k_base）による正確値。5MB 超は比例概算。モデルによりトークナイザは異なるため他社モデルでは目安 |
| シンボルアウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。HTML はインライン `<script>` 内の JS を抽出してアウトラインする（v1.2.5+・`src` 付き外部スクリプトは対象外）。構文エラーのあるファイルは正規表現に縮退し、取得漏れがありうる |
| import依存グラフ（`-M`/`--focus`） | AST 抽出＋マニフェスト解決（tsconfig paths・package.json imports・go.mod・Rustモジュールツリー・Java/Kotlin FQCN・PHP use・Ruby require_relative）。Rust はネストした Cargo.toml をクレート境界として検出しクレート単位で解決＝モノレポ/ワークスペース対応（v1.2.7+。v1.2.6 以前はスキャンルート直下の `src/` しか解決できず、サブクレートのファイルを `--focus` すると依存元が過少に出た）。`mod` 宣言のみのエッジは循環依存の検出から除外され、lib.rs/mod.rs⇄子モジュールの往復は循環として報告されない（v1.2.7+）。tsconfig paths / package.json imports / go.mod はスキャンルートのもののみ読むため、JS/TS/Go のネストしたサブプロジェクトのファイルを `--focus` すると注意書き（JSON では `note` フィールド）が付く — その場合は path をサブプロジェクトにして再実行する。外部パッケージの実体は解決されず「external」扱い。C#/Swift はローカル解決なし |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない**。判定対象は `.py/.js/.jsx/.ts/.tsx/.go`（＋AST有効時の `.rs`）のみで、対象外のファイルは JSON で `has_test: null` になる（v1.2.5+。v1.2.4 以前は対象外にも一律 `true` が返り「テスト有り」に見えた） |
| エントリーポイント検出（`-N`） | 既知のファイル名パターン（`main.py`、`index.js`等）と`package.json`の`main`/`bin`フィールドのみで判定 |
| TODO/FIXME抽出（`-K`） | 単語境界つき文字列マッチ。コメント外の文字列内に偶然該当語があっても拾われる場合がある |
| git連携（`-H`/`--status`/`--since`/`--api-diff`） | 直近2000コミットのみ走査（`-H`）。それより古い変更しかないファイルは情報が出ない |
| 長大関数・doc 1行目 | AST の行スパン/docstringに基づく。正規表現縮退時は出力されない |
| ディレクトリの `size`/`size_human` | 常にディスク上の生サイズ（`du`相当）で、**`-G`（gitignore除外）の影響を受けない**。子要素一覧・トークン数・解析対象は `-G` で正しく除外されるが、サイズ集計だけは対象外（旧Python版から一貫した仕様）。`node_modules/`や`target/`等が`.gitignore`済みでも合計サイズには含まれるので、サイズだけで「大きい」と早合点しないこと。v1.2.5+ は `--agent` の末尾注記と JSON の `analysis.dir_sizes` にこの旨が明記される |

これらの制限は「完全に間違っている」という意味ではなく、「**最終確認はファイルの中身で行うべき**」という意味である。dirlensの出力は最初の当たりをつけるための地図として使い、コードの詳細な振る舞いや正確性が重要な判断は、必ず該当ファイルを実際に読んで確認すること。

---

## 使い方の早見表

```bash
dirlens --agent                  # 推奨：解析全部入り（テキスト、カラーなし、英語）
dirlens --agent --json           # 推奨：解析全部入り（JSON、パース用）
dirlens --agent -L 2             # 深さを2階層に制限して概要だけ把握
dirlens --agent --estimate       # 階層別の出力コストを見積もる
dirlens --agent --budget 3000    # 出力を3000トークン以内に自動調整

dirlens --focus src/main.py -G   # このファイルの影響範囲（依存元/依存先）
dirlens --since HEAD -G          # 前回コミット以降の変更ファイルだけ
git diff --name-only | dirlens --stdin --json   # 変更ファイルだけの解析

dirlens -O src/main.py           # このファイルの関数/クラス一覧
dirlens -A                       # プロジェクト全体の公開API一覧
dirlens -M                       # import依存グラフ（影響範囲調査）
dirlens -M --mermaid             # importグラフを Mermaid 図として出力
dirlens -V -G                    # テスト未整備ファイルの一覧
dirlens -K -G                    # TODO/FIXMEの棚卸し
dirlens -H -L 1                  # 直近の変更点を素早く把握
dirlens -F -G                    # 設定ファイルの一覧
dirlens -N -G                    # エントリーポイント候補
dirlens --api-diff v1.0.0        # 公開APIの差分（破壊的変更の検出）
dirlens --top 10                 # 大きいファイル/ディレクトリ上位10
dirlens --check                  # この環境で使える解析方式の確認
```
