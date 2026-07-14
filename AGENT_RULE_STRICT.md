# dirlens エージェント向けルール（厳格版）

このプロジェクトのコード探索を始める前に、`dirlens --agent` を実行してプロジェクト全体の構造を把握してください。

## 基本ルール

1. **調査の最初の一手として `dirlens --agent` を実行する**

   ファイルツリー・サイズ・最終更新日時に加え、推定トークン数（BPE 正確値）、最終コミット情報、TODO/FIXME、テスト未整備ファイル、エントリーポイント候補、関数/クラスのアウトライン、import依存関係、設定ファイル一覧、言語別トークン内訳、長大関数を一度に取得できる。`ls`/`find`/`grep` を繰り返すより少ない往復で全体像を掴める。

   `--agent` はANSIカラーコードを自動的に無効化する（`--no-color` を兼ねる）ため、エージェント出力やログとしてそのまま扱える。出力はデフォルトで英語（`--lang ja` で日本語）。

2. **構造化データが必要な場合は `dirlens --agent --json` を使う**

   パース可能なJSON形式で同じ情報を取得できる。`project_summary` にプロジェクト全体の集計、`language_breakdown` に言語別内訳、`longest_functions` に長大関数、`errors` に部分的に取得できなかった情報（git不在等）が機械可読で入る。出力は部分的な解析失敗があっても常に valid な JSON。

3. **コンテキストが逼迫しているときは `--budget N` を使う**

   `dirlens --agent --budget 3000` のように指定すると、出力自体が指定トークン数（o200k BPE 実測）に収まるよう深さ・詳細が自動調整される。末尾に実測トークン数が付記される。

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

   - `dirlens -O <path>` — 特定ファイルの関数/クラス一覧（JSON では doc 1行目・行範囲つき）
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

7. **`--ai` はエージェントでは使わない**

   `--ai` はクリップボードへのコピーを伴うため、エージェントの自律実行では不要かつ副作用になりうる。エージェントは常に `--agent` を使う。

8. **`-L` はツリー表示の深さのみを制限する。集計値はプロジェクト全体を反映する**

   `dirlens --agent -L 2` のように深さを指定した場合、ツリーの「見た目」は指定階層までに制限されるが、TODO件数・テスト未整備数・エントリーポイント数などの集計値はプロジェクト全体のスキャン結果を反映する。巨大なリポジトリでざっくり把握したいときは `-L 2` 等を使ってよい。

9. **巨大なリポジトリでは `-G` の併用を確認する**

   `--agent` は自動的に `-G`（.gitignore除外）を有効にするが、個別フラグ（`-V`/`-N`/`-M`/`-K`/`-F`/`--focus` 等）を単体で使う場合は `-G` を明示的に付けないと `node_modules` 等まで走査して遅くなることがある。なお2回目以降の実行はトークン計数キャッシュにより速くなる（`DIRLENS_CACHE=off` で無効化可）。

10. **MCP ホストでは `--mcp` でネイティブツールとして登録できる**

    ```bash
    claude mcp add dirlens -- dirlens --mcp
    ```

    analyze / tree / outline / imports / focus / todos の6ツールが使えるようになり、シェル経由の往復が不要になる。

---

## 出力の正確さについて（重要）

dirlens の解析は「最良の方式 → 縮退」の多層構成であり、実際に使われた方式は `--check` や `--agent --json` の `capabilities` / `analysis` ブロックで機械的に確認できる。以下の制限を理解して使うこと。

| 機能 | 方式と制限事項 |
|---|---|
| トークン数（`-T`） | BPE（o200k_base）による正確値。5MB 超は比例概算。モデルによりトークナイザは異なるため他社モデルでは目安 |
| シンボルアウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。構文エラーのあるファイルは正規表現に縮退し、取得漏れがありうる |
| import依存グラフ（`-M`/`--focus`） | AST 抽出＋マニフェスト解決（tsconfig paths・package.json imports・go.mod・Rustモジュールツリー・Java/Kotlin FQCN・PHP use・Ruby require_relative）。外部パッケージの実体は解決されず「external」扱い。C#/Swift はローカル解決なし |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない** |
| エントリーポイント検出（`-N`） | 既知のファイル名パターン（`main.py`、`index.js`等）と`package.json`の`main`/`bin`フィールドのみで判定 |
| TODO/FIXME抽出（`-K`） | 単語境界つき文字列マッチ。コメント外の文字列内に偶然該当語があっても拾われる場合がある |
| git連携（`-H`/`--status`/`--since`/`--api-diff`） | 直近2000コミットのみ走査（`-H`）。それより古い変更しかないファイルは情報が出ない |
| 長大関数・doc 1行目 | AST の行スパン/docstringに基づく。正規表現縮退時は出力されない |

これらの制限は「完全に間違っている」という意味ではなく、「**最終確認はファイルの中身で行うべき**」という意味である。dirlensの出力は最初の当たりをつけるための地図として使い、コードの詳細な振る舞いや正確性が重要な判断は、必ず該当ファイルを実際に読んで確認すること。

---

## 使い方の早見表

```bash
dirlens --agent                  # 推奨：解析全部入り（テキスト、カラーなし、英語）
dirlens --agent --json           # 推奨：解析全部入り（JSON、パース用）
dirlens --agent -L 2             # 深さを2階層に制限して概要だけ把握
dirlens --agent --budget 3000    # 出力を約3000トークン以内に自動調整

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
