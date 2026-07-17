# dirlens エージェント向けルール（テンプレート）

dirlensは「使えたら速い索引ツール」として扱う。**必須ツールとして扱わない**。実行に失敗しても作業を止めず、標準コマンドにフォールバックして調査を続けること。

---

## 基本ルール

1. **存在確認してから使う**

   ```bash
   command -v dirlens
   ```

   見つかった場合のみ次に進む。見つからない場合は最初から標準コマンド（`ls`/`find`/`git ls-files`等）で調査する。

2. **エージェント向けには `--agent` を使う。`--ai` は使わない**

   ```bash
   dirlens --agent                 # 人間が読むログ・テキスト出力（--agent はカラーなし）
   dirlens --agent --json          # 構造化データとしてパースしたい場合
   ```

   `--agent` はANSIカラーコードを自動で無効化する（`--no-color` を兼ねる）。明示的に `--no-color` を付けても害はない。

   `--ai` はクリップボードへのコピーを行う。サンドボックスやヘッドレス環境では `pbcopy`/`xclip` 等が使えず失敗しやすいうえ、そもそも人間がAIチャットに貼り付けるための機能であり、エージェントの自律実行では不要かつ副作用になりうる。

3. **失敗したら記録してフォールバックする。リトライに固執しない**
   実行した正確なコマンド・exit code・エラーの先頭1〜2行をメモし、すぐに以下へ切り替える：

   ```bash
   pwd
   git status --short
   git ls-files            # または: rg --files
   find . -maxdepth 2 -type f
   du -sh */               # 大まかなサイズ感だけ知りたい場合
   ```

4. **`npx`/`npm exec` は第一候補にしない**
   npmキャッシュへの書き込み権限が無い環境（`~/.npm` がroot所有・読み取り専用等）でEPERM/失敗しやすい。どうしても使う場合は書き込み可能な一時ディレクトリをキャッシュに指定する：

   ```bash
   NPM_CONFIG_CACHE=/tmp/dirlens-npm-cache npx dirlens --agent
   ```

5. **`-L`（深さ制限）はツリー表示を浅くするだけで、解析集計（TODO件数・推定トークン数・言語別内訳・長大関数・テスト未整備数・エントリーポイント数・依存度ランキング等）はプロジェクト全体のスキャン結果を反映する（v1.2.5+）**
   深い階層の概要だけサッと見たいときは `-L 2` 等を使ってよいが、ツリーの「見た目」が浅くなるだけで、サマリーの数値自体は省略されない。なお `Total N directories, M files` と拡張子出現数（`.rs ×18` 等）だけは「表示されたツリー」の集計（`tree` コマンド互換の意味論）。
   v1.2.4 以前は TODO 件数・トークン集計・長大関数が表示深さに引きずられて過少になるバグがあった（テスト未整備数等は全体反映で、集計項目により挙動が食い違っていた）。

   また、単体フラグ（`-V`/`-K`/`-N`/`-F`/`-A`等）の出力は「該当ファイルだけの一覧」ではなく「**全ツリーに該当マーカーを注釈した表示**」である（例: `-V` は全ファイルのうち該当分に `no test` が付く）。出力サイズはプロジェクト規模に比例するため、大きなリポジトリでは `-L`/`--budget`/`-G` と組み合わせること（`--budget`/`--estimate` は単体フラグにもそのまま効く）。フラットな一覧が欲しい場合は `--top N` だけが例外的にフラット出力。

6. **コンテキスト節約用のモードを知っておく（Rust 版 v1.2+）**

   ```bash
   dirlens --agent --estimate                     # 階層別の出力コストを見積もる（--budget の値決めに）
   dirlens --agent --budget 3000                  # 出力を3000トークン以内に自動調整
   dirlens --since HEAD -G                        # 前回コミット以降の変更ファイルだけ
   git diff --name-only | dirlens --stdin --json  # 指定ファイルだけの解析
   dirlens --focus src/app.py -G                  # このファイルの影響範囲（依存元/依存先）
   ```

   出力はデフォルトで英語（`--lang ja` で日本語）。MCP ホストでは
   `claude mcp add dirlens -s user -- dirlens --mcp` でネイティブツールとしても登録できる
   （`-s user` を省略すると local scope になり、登録時のカレントディレクトリのプロジェクトにしか有効にならない）。

7. **MCP ツールと CLI の対応関係を知っておく（v1.2.2+）**

   MCP では 9 ツールが使える: `analyze` / `tree` / `outline` / `imports` / `focus` / `todos` / `since` / `history` / `api_diff`。CLI との主な対応と注意点:

   - `analyze` = `--agent --json`。大きなプロジェクトでは JSON が巨大化するため、`estimate: true` でコスト見積もり → `budget: N` で予算内テキストに切り替えるのが安全（`budget` 指定時は JSON ではなく注釈付きテキストが返る）。`estimate` は実際に使われる出力フォーマット（既定は JSON、`budget` 指定時はテキスト）で見積もる。**見積もりが正確でも、多くの MCP ホストは 1 応答に上限がある（Claude Code の既定は 25,000 トークン）**。使いたい階層の見積もりがホスト上限を超えるなら、上限未満の `budget`（例: 20000）を指定して呼ぶこと。v1.2.6+ は見積もり表の該当行に `⚠ exceeds host cap` マークが付き、末尾に上限値（`MAX_MCP_OUTPUT_TOKENS` があればその値、無ければ Claude Code 既定の 25000）と推奨 `budget` が表示されるので、その場で判断できる。v1.2.9+ は見積もり表に「全階層・テキスト（budget 指定時の形式）」行が加わる — テキストは JSON の数分の一で済むことが多いので、depth を絞る前に budget を検討する。`path` にファイルを渡すと単一ファイルレポート（`outline` の `files` 指定時と同じ形）が返る
   - `outline` は **複数ファイルを配列で一括処理できる**（CLI の `-O` は単一ファイルのみ）。`files` を省略するとプロジェクト全体の公開 API（`-A` 相当）になり、この場合 `depth` は既定で 2 に制限される（全ツリー走査で肥大化しうるため）。全階層を見たい場合は `depth` を明示するか `unlimited_depth: true` を渡す（`depth` を指定した場合はそちらが常に優先）。v1.2.9+ は解決できなかったパスが `errors` 配列に載り（黙って落ちない）、重複は1回だけ処理、`files: []` は空の結果（全体モードはキー省略でのみ発動）、depth 打ち切りディレクトリには `truncated: true` が付く。ネストしたシンボルには `parent`（メソッドならクラス名/impl の型名）が付き同名メソッドを区別できる（v1.2.10+）。`files` はスキャンルート外のパスも指定可（CLI と同じローカル権限で読む・意図した仕様）で、ルート外のファイルは正規化済みの絶対パスで返る（v1.2.12+）
   - `history` は既定で `depth: 1`（ホットスポット一覧は深さに依らず全体を反映）。全ファイルの最終コミットを見たい場合は `outline` 同様 `depth` か `unlimited_depth: true` を指定する
   - `since` = `--since REF -G` ＋ 変更ファイルへのトークン/アウトライン/TODO 注釈。セッション途中の差分確認はこれを使う（ref 省略時は HEAD＝未コミット変更）
   - `imports` と `todos` は CLI の `-M`/`-K` と違い、該当なしファイルを含まないフラットな一覧を返す（`limit` で件数上限を指定可。v1.2.9+ は切り詰め時に `imports` は `total_files`＋`truncated`、`todos` は `truncated` が付く）。`imports` はさらに `format: "mermaid"` / `"dot"` で図の出力も可
   - gitignore 済みディレクトリを `path` に指定すると既定では空のツリーが返る（注記付き・v1.2.10+）。中身は `tree`/`analyze` の `include_ignored: true` で見える。負の `depth` はエラー（0 はサマリのみで有効）
   - MCP サーバーは実装レベルでクリップボード無効（`NoClipboard` 固定）。`capabilities.clipboard: false` は異常ではない
   - MCP に**無い**もの: `--pack` / `--compare` / `--dupes` / `--heat` / `--csv` / stdin パイプ。必要ならシェルで CLI を直接使う。v1.2.1 以前のサーバーはツールが 6 個（since/history/api_diff と budget/estimate/format/top パラメータが無い）なので、無ければバージョンを確認する

---

## コマンドが動かないときの切り分け（実証済みのフォールバック順）

複数のエージェント実行環境で検証した結果、エラーの出方からおおよその原因を切り分けられる。

| 症状 | 考えられる原因 | 次の一手 |
|---|---|---|
| `command not found` / `dirlens: No such file or directory` | グローバルbinがPATHに無い、もしくはエージェントのシェルがユーザーの通常PATHを読み込んでいない | ステップ3のフォールバックへ |
| `env: node:` / `python3:` 系のエラー、無言の exit 1、`Fatal Python error` | 旧バージョン（Python 実装 v1.0.x またはその wrapper）を使っている。現行の Rust 版は単一バイナリでランタイム依存ゼロのため、これらは発生しない | dirlens を最新版に更新する（`npm install -g dirlens` または GitHub Releases のバイナリ）。旧版のまま使う場合の回避策は `DIRLENS_PYTHON` で system python を指すこと |
| `error: unexpected argument '--budget'` 等 | 旧バージョンには v1.2+ の新フラグが無い | `--agent` のみで使うか、最新版への更新を提案する |
| `Operation not permitted` がファイル読み込み全般で出る（`ls`/`cat`/`file` 等、dirlens以外のコマンドでも再現する） | OSレベルのサンドボックスが特定ディレクトリ（mise管理パス等）への読み取りを禁止している | dirlensに限らない制約。ユーザーに権限設定の確認を依頼するか、許可されているディレクトリ内にあるツールのみで作業を続ける |
| `error: permission denied for the current directory`（旧版: `エラー: 現在のディレクトリへのアクセス権限がありません`） | カレントディレクトリ自体がサンドボックス外 | 絶対パスを明示的に指定する: `dirlens /path/to/project` |
| `--ai` 使用時に `copy failed` と出るがツリー本文は表示される | クリップボードコマンド（pbcopy/xclip/wl-copy）が使えない環境 | エージェントは `--ai` を使わず `--agent` を使う（本文の取得自体は失敗していない） |
| 出力の言語が想定と違う | 設定ファイル（`.dirlens.toml` 等）や `DIRLENS_LANG` が言語を上書きしている | `--lang en` を明示するか `--no-config` を付ける |
| npm cache関連のEPERM（`npx`/`npm exec`/`npm pack`等） | `~/.npm` への書き込み権限が無い | `NPM_CONFIG_CACHE` を書き込み可能な一時ディレクトリに向ける |

---

## 推奨コマンド早見表

```bash
# 存在確認 → 最初の一手
command -v dirlens && dirlens --agent

# 構造化データとして処理したい場合
dirlens --agent --json

# 深い階層の概要だけサッと見たい場合（集計は全体反映、ツリー表示だけ浅くなる）
dirlens --agent -L 2

# 個別の情報だけ欲しい場合
dirlens -O src/main.py     # 特定ファイルの関数/クラス一覧
dirlens -A                 # 公開API（exportされたシンボル）のみ
dirlens -M                 # import依存グラフ（影響範囲調査・循環依存検出）
dirlens -V                 # テスト未整備ファイルの一覧
dirlens -K                 # TODO/FIXMEの棚卸し
dirlens -H                 # 直近の変更履歴・ホットスポット
dirlens -F                 # 設定ファイル（.env, tsconfig.json等）の検出

# コンテキスト節約・影響範囲（Rust 版 v1.2+）
dirlens --agent --estimate       # 階層別の出力コストを見積もる
dirlens --agent --budget 3000    # 出力をトークン予算内に自動調整
dirlens --focus src/app.py -G    # 変更の影響範囲（依存元/依存先）
dirlens --since HEAD -G          # 変更ファイルだけのツリー
dirlens --api-diff v1.0.0        # 公開APIの差分（破壊的変更の検出）
```

---

## 出力の正確さについて

dirlens の解析は「最良の方式 → 縮退」の多層構成。実際に使われた方式は `--check` や `--agent --json` の `capabilities` ブロックで機械的に確認できる。以下の制限を理解して使うこと。

| 機能 | 方式と制限事項 |
|---|---|
| トークン数（`-T`） | BPE（o200k_base）による正確値。5MB 超は比例概算（JSON では `tokens_estimated: true` が付く・v1.2.9+）。他社モデルのトークナイザでは目安。通常ファイル以外（FIFO・ソケット・デバイス）は読まずサイズ 0 扱い（v1.2.12+。v1.2.11 以前は FIFO を含むディレクトリで永久にブロックした） |
| シンボルアウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。HTML はインライン `<script>` 内の JS を抽出してアウトラインする（v1.2.5+・src 付き外部スクリプトは対象外）。Python の `public` 判定はスコープ対応（v1.2.9+）: 関数内のローカル定義とそのメンバは非公開、クラスメソッドはクラス自身が公開の場合のみ名前で判定。ネストしたシンボルには外側シンボル名が付く（JSON `parent`・テキストは親付き表示・v1.2.10+）。構文エラーのあるファイルは正規表現に縮退し、取得漏れがありうる（JSON の `outline_method` = "ast"/"regex" で判別可・v1.2.11+） |
| import依存グラフ（`-M`/`--focus`) | AST 抽出＋マニフェスト解決（tsconfig paths・package.json imports・go.mod・Rustモジュールツリー・Java/Kotlin FQCN・PHP use・Ruby require_relative）。Rust はネストした Cargo.toml をクレート境界として検出しクレート単位で解決＝モノレポ/ワークスペース対応（v1.2.7+。v1.2.6 以前はスキャンルート直下の `src/` しか解決できず、サブクレートのファイルを `--focus` すると依存元が過少に出た）。`mod` 宣言のみのエッジは循環依存の検出から除外され、lib.rs/mod.rs⇄子モジュールの往復は循環として報告されない（v1.2.7+）。tsconfig paths / package.json imports / go.mod はスキャンルートのもののみ読むため、JS/TS/Go のネストしたサブプロジェクトは `--focus` に注意書きが付く（path をサブプロジェクトにして再実行を推奨）。外部パッケージの実体は解決されず「external」扱い。C#/Swift はローカル解決なし |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない**。判定対象は `.py/.js/.jsx/.ts/.tsx/.go`（＋AST有効時の `.rs`）のみで、対象外のファイルは JSON で `has_test: null` になる（v1.2.5+。v1.2.4 以前は対象外にも一律 `true` が返り「テスト有り」に見えた）。Rust の `lib.rs`/`main.rs`/`mod.rs` は名前で判定対象から免除される（re-export・配線ファイルの定番名でノイズになるため。ロジック満載の lib.rs もフラグが立たない点に注意） |
| エントリーポイント検出（`-N`） | 既知のファイル名パターンと`package.json`の`main`/`bin`フィールドのみで判定 |
| 設定ファイル検出（`-F`） | 既知のファイル名パターンのみで判定。独自命名の設定ファイルは拾えない |
| TODO/FIXME抽出（`-K`） | 単語境界つき文字列マッチ。コメント外の文字列内に偶然該当語があっても拾われる場合がある |
| git連携（`-H`/`--status`/`--since`/`--api-diff`） | 直近2000コミットのみ走査（`-H`）。それより古い変更しかないファイルは情報が出ない。リポジトリのサブディレクトリをスキャンルートに指定してもパスは正しく突き合わされる（v1.2.8+。v1.2.7 以前はサブディレクトリ指定時にコミット注釈の欠落・同名ファイルの誤った履歴・`--since` の空振り・`--api-diff` の全削除誤報が起きた）。`--api-diff` は untracked ファイルも `(untracked)` 注記付きで含める（v1.2.12+。v1.2.11 以前は git add するまで新規ファイルの公開 API が差分に現れなかった） |
| ディレクトリの `size`/`size_human` | 常にディスク上の生サイズ（`du`相当）で、**`-G`（gitignore除外）の影響を受けない**。子要素一覧・トークン数・解析対象は `-G` で正しく除外されるが、サイズ集計だけは対象外（旧Python版から一貫した仕様）。`node_modules/`や`target/`等が`.gitignore`済みでも合計サイズには含まれるので、サイズだけで「大きい」と早合点しないこと。v1.2.5+ は `--agent` の末尾注記と JSON の `analysis.dir_sizes` に、v1.2.9+ は `--top` / MCP `tree` の `top` の出力にもこの旨が明記される |

これらの制限は「完全に間違っている」という意味ではなく、「**最終確認はファイルの中身で行うべき**」という意味である。dirlensの出力は最初の当たりをつけるための地図として使い、コードの詳細な振る舞いや正確性が重要な判断は、必ず該当ファイルを実際に読んで確認すること。
