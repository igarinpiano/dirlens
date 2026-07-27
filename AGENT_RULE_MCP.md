# dirlens エージェント向けルール（CLI + MCP 併用版）

この版は、シェル上の `dirlens` バイナリと、MCP 経由の `dirlens` ツール（`analyze`/`tree`/`outline`/`imports`/`focus`/`todos`/`since`/`history`/`api_diff`）の**両方**が登録済みの環境向け。片方しか無いなら [AGENT_RULE_STRICT.md](AGENT_RULE_STRICT.md)（CLIが常時使える前提）または [AGENT_RULE.md](AGENT_RULE.md)（存在確認とフォールバック重視のテンプレート）を使ってください。

この版は STRICT 版の内容を全て含んだ上で、「同じ情報を取る手段が2つあるとき、CLIとMCPのどちらを使うべきか」の判断基準を追加したもの。

以下の区切り線から下をそのまま `CLAUDE.md`・`.cursorrules` 等のグローバルルールファイルに貼り付ければ使えます（このファイルの説明文はコピペ不要です）。

---

**↓↓↓ ここから下をそのままペーストしてください ↓↓↓**

dirlensは、コードベースを探索する際に `ls`/`find`/`tree`/`grep` を何度も打つ代わりに使う単一バイナリのCLIツール。ファイルツリー・サイズ・最終更新日時に加え、正確なトークン数（BPE）、gitの最終コミット情報、git status マーク、TODO/FIXME、テスト未整備ファイル、エントリーポイント候補、関数/クラスのアウトライン、import依存関係、設定ファイル一覧、言語別トークン内訳、長大関数を一度の実行でまとめて取得できる。**この環境では同じ機能をシェルの `dirlens` コマンドと、MCPツール（`analyze`等）の2通りで呼び出せる。** dirlensは「毎回使ってよい第一候補のツール」として扱う。存在確認や失敗時の様子見に時間を使うより、まず叩いてみる方が速い。ただし出力は無条件に正しいとは限らない — 依存グラフ・テスト検知・エントリーポイント検出は既知のヒューリスティックで動いており縮退もある（後述）。**「頻繁に使うが、鵜呑みにはしない」**が基本姿勢。

## 基本ルール

1. **調査の最初の一手として `dirlens --agent`（またはMCPの `analyze`）を実行する**

   ファイルツリー・サイズ・最終更新日時に加え、推定トークン数（BPE 正確値）、最終コミット情報、git status マーク（`[M]`/`[??]` 等）、TODO/FIXME、テスト未整備ファイル、エントリーポイント候補、関数/クラスのアウトライン、import依存関係、設定ファイル一覧、言語別トークン内訳、長大関数を一度に取得できる。`ls`/`find`/`grep` を繰り返すより少ない往復で全体像を掴める。

   `--agent` はANSIカラーコードを自動的に無効化する（`--no-color` を兼ねる）ため、エージェント出力やログとしてそのまま扱える。出力はデフォルトで英語（`--lang ja` で日本語）。

2. **構造化データが必要な場合は `dirlens --agent --json`（MCPの `analyze`）を使う**

   パース可能なJSON形式で同じ情報を取得できる。`project_summary` にプロジェクト全体の集計、`language_breakdown` に言語別内訳、`longest_functions` に長大関数、`errors` に部分的に取得できなかった情報（git不在等）が機械可読で入る。シンボリックリンクには `symlink: {target, broken}`（broken = リンク切れ）、アウトラインには取得方式 `outline_method`（"ast" / "regex"。regex は構文エラー等による縮退で取得漏れがありうる）が付く。出力は部分的な解析失敗があっても常に valid な JSON。

3. **コンテキストが逼迫しているときは `--estimate` → `--budget N` を使う**

   ```bash
   dirlens --agent --estimate       # 階層別の出力コスト（-L 1 / -L 2 / … / 全階層）を数行で見積もる
   dirlens --agent --budget 3000    # 出力を指定トークン数（o200k BPE 実測）以内に自動調整
   ```

   `--budget` は深さ→解析注釈→ツリー行の順に削って必ず予算内に収める。収まらなかった分は「… N more entries (omitted by --budget)」と省略され、末尾に実測トークン数と「この階層を全て表示するのに必要なトークン数」が付記されるので、予算を増やすかどうかをそこで判断できる。MCP の `analyze`/`tree` にも同名パラメータがある（10項参照）。

4. **変更の影響範囲を調べるときは `--focus`（MCPの `focus`）を使う**

   ```bash
   dirlens --focus src/cfg.rs -G     # このファイルの依存元/依存先（直接+推移）
   dirlens --focus src/cfg.rs -G --json
   ```

   「このファイルを変更したら何が壊れうるか」が import グラフの推移閉包で一発で分かる。

5. **セッション途中の再確認には `--since` / `--stdin`（MCPの `since`）で差分だけ取る**

   ```bash
   dirlens --since HEAD -G                        # 前回コミット以降に変更されたファイルだけのツリー
   git diff --name-only | dirlens --stdin --json  # 変更ファイルだけのトークン数・アウトライン・TODO
   ```

   全ツリーを再出力するより大幅にトークンを節約できる（`--stdin` は MCP に無いので、これが必要なときはシェルを使う）。

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

   単体フラグ（`-V`/`-K`/`-N`/`-F`/`-A`等）の出力は「該当ファイルだけの一覧」ではなく「**全ツリーに該当マーカーを注釈した表示**」である（例: `-V` は全ファイルのうち該当分に `no test` が付く）。出力サイズはプロジェクト規模に比例するため、大きなリポジトリでは `-L`/`--budget`/`-G` と組み合わせること（`--budget`/`--estimate` は単体フラグにもそのまま効く）。フラット出力は `--top N` のみ。`-M`/`-K` に対応する MCP の `imports`/`todos` は挙動が異なる（10項参照）。

7. **`--ai` はエージェントでは使わない**

   `--ai` はクリップボードへのコピーを伴うため、エージェントの自律実行では不要かつ副作用になりうる。エージェントは常に `--agent` を使う。

8. **`-L` はツリー表示の深さのみを制限する。解析集計値はプロジェクト全体を反映する**

   `dirlens --agent -L 2` のように深さを指定した場合、ツリーの「見た目」は指定階層までに制限されるが、TODO件数・推定トークン数・言語別内訳・長大関数・テスト未整備数・エントリーポイント数などの解析集計値はプロジェクト全体のスキャン結果を反映する。巨大なリポジトリでざっくり把握したいときは `-L 2` 等を使ってよい。例外は `Total N directories, M files` と拡張子出現数（`.rs ×18` 等）で、これらは「表示されたツリー」の集計（`tree` コマンド互換の意味論）。JSON では `-L` で打ち切られたディレクトリに `truncated: true` が付く。

9. **巨大なリポジトリでは `-G` の併用を確認する**

   `--agent` は自動的に `-G`（.gitignore除外）を有効にするが、個別フラグ（`-V`/`-N`/`-M`/`-K`/`-F`/`--focus` 等）を単体で使う場合は `-G` を明示的に付けないと `node_modules` 等まで走査して遅くなることがある。なお2回目以降の実行はトークン計数キャッシュにより速くなる（`DIRLENS_CACHE=off` で無効化可）。

10. **MCP と CLI、どちらを使うか（この版の主眼）**

    同じ情報を取る手段が2つあるとき、以下を判断基準にする。1タスクにつき「念のため両方」は行わず、どちらか一方を選ぶこと。

    **MCP ツールを優先する場面（通常はこちら）** — `analyze`（= `--agent --json`）/ `tree` / `outline` / `imports` / `focus` / `todos` / `since` / `history` / `api_diff` に対応する調査全般。シェル呼び出しのクォーティング事故が無く、構造化データがそのまま返るため後段のパースが要らない。プロセス起動コストも無い。

    **シェル CLI に切り替える場面**
    - MCP に**無い**機能を使うとき: `--pack` / `--compare` / `--dupes` / `--heat` / `--csv` / `--mermaid`・`--dot` のファイル出力 / `-i` インタラクティブ / stdin パイプ（`--stdin`）/ `--completions` / `--man` / `--status` / `--clear-cache`。
    - 見積もりの時点で出力がホストの1応答上限（Claude Code 既定 25,000トークン）を超えると分かっている、かつそのボリュームが本当に必要な場合。MCP 経由だと `budget` で削っても上限自体は動かせないが、シェルなら `dirlens --agent --budget N` を直接実行して読める（応答サイズの制約を受けるのは「MCP 経由でチャットに返す」場合であって、dirlens 自体の出力サイズではない）。
    - セッション途中で MCP 呼び出しがエラーを返し続ける、あるいは明らかに不調なとき。チャット内から MCP サーバーを再起動できないため、同じ呼び出しに固執せず残りはシェル CLI で続行する。MCP 呼び出しがエラーになった場合は、まず同じ内容をシェルで一度だけ試してから切り分ける（dirlens 自体のバグより、登録スコープや環境変数の差異が原因であることの方が多い）。

    **MCP 固有の注意点**
    - `analyze`/`tree` は大きなプロジェクトで応答が肥大化しやすい。`estimate: true` で見積もり→ホスト上限（既定25,000トークン、`⚠ exceeds host cap` 表示）を超えるなら上限未満の `budget` を指定する（`budget` 指定時は JSON ではなく注釈付きテキストで返る）。
    - `outline`（`files` 省略）と `history` は `depth` を省略すると小さい既定値（それぞれ2/1）に制限される。全階層が要るなら `depth` を明示するか `unlimited_depth: true`。
    - `outline` は複数ファイルを配列で一括処理できる（CLI の `-O` は単一ファイルのみ）。`files` を省略するとプロジェクト全体の公開 API（`-A` 相当）になる。
    - `imports`/`todos` は該当なしファイルを含まないフラットな配列を返す（CLI の `-M`/`-K` は全ツリー注釈形式なので出力形が異なる）。`limit` で件数上限、切り詰め時は `truncated`/`total_files` が付く。`imports` は `format: "mermaid"`/`"dot"` にも対応。
    - gitignore 済みディレクトリを `path` に指定すると既定で空のツリーが返る（`include_ignored: true` で中身を見る）。
    - MCP サーバーはクリップボード無効固定（`capabilities.clipboard: false` は正常）。

    **環境変数・キャッシュは共有されるが、プロセスの env が違うことがある**

    MCP サーバーは CLI と同じバイナリ・同じプロセス実装で動くため、`DIRLENS_MAX_FILE_BYTES`/`DIRLENS_MAX_WORKERS`/`DIRLENS_GITIGNORE`/`DIRLENS_AST`/`DIRLENS_TOKENS`/`DIRLENS_COMPAT`/`DIRLENS_CACHE` は MCP 経由の呼び出しにもそのまま適用され、永続トークンキャッシュも共有される。ただし**プロセスが継承する環境変数はシェルと MCP ホストで別**であることに注意する。対話シェルではシェルプロファイル（`.zshrc` 等）で export した `DIRLENS_*` が効くが、MCP ホスト（特に GUI アプリを Finder/Dock から起動した場合）はそれらを継承していないことがある。同じフラグで CLI と MCP の結果が食い違ったら、まず `--check`（シェル側）と `capabilities`（MCP側の `analyze` 応答等）を突き合わせて `max_file_bytes` 等の実効値が一致しているか確認する。dirlens のバグを疑うのはその後でよい。

    **登録スコープの確認**

    `claude mcp add` に `-s user` を付けないと local scope になり、登録したディレクトリでしか有効にならない。MCP ツールが今のプロジェクトで見えない（他のプロジェクトでは見えていた）場合、真っ先に疑うのはこのスコープ設定。dirlens 自体が壊れていると決めつけてリトライを繰り返さず、シェルの `dirlens --agent` にフォールバックして作業を続ける。登録手順は `dirlens --mcp-setup` を参照（バイナリの絶対パス入りで各ホスト向けの手順を出力する）。

---

## 出力の正確さについて（重要・トランスポートに関係なく共通）

dirlens の解析は「最良の方式 → 縮退」の多層構成であり、実際に使われた方式は `--check` や `--agent --json`/`analyze` の `capabilities` / `analysis` ブロックで機械的に確認できる。同じバイナリ・同じ解析エンジンなので、以下の制限は CLI 経由でも MCP 経由でも同一に適用される。「MCP から返ってきたから CLI より正確/公式」ということはない。

| 機能 | 方式と制限事項 |
|---|---|
| トークン数（`-T`） | BPE（o200k_base）による正確値。1ファイルあたりの読み込み上限（既定5MB、ホストの物理メモリ量に応じて段階的に引き上がる）を超えると比例概算（JSON では該当ファイルに `tokens_estimated: true` が付く）。`DIRLENS_MAX_FILE_BYTES` で明示指定も可能。実際に使われた値は `--check`/`capabilities.max_file_bytes` で確認できる。モデルによりトークナイザは異なるため他社モデルでは目安。通常ファイル以外（FIFO・ソケット・デバイス）は読まずサイズ 0 扱い |
| シンボルアウトライン（`-O`/`-A`/`outline`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。HTML はインライン `<script>` 内の JS を抽出してアウトラインする（`src` 付き外部スクリプトは対象外）。Python の `public` 判定はスコープ対応: 関数内のローカル def / class とそのメンバは非公開、クラスメソッドはクラス自身が公開の場合のみ名前で判定。ネストしたシンボルには外側シンボル名が付く（JSON は `parent` フィールド、テキストは `def Class.method` / `fn Type::method` 表示）。構文エラーのあるファイルは正規表現に縮退し、取得漏れがありうる — どちらの層で取得したかは JSON の `outline_method`（"ast"/"regex"）で機械的に判別できる |
| import依存グラフ（`-M`/`--focus`/`imports`/`focus`） | AST 抽出＋マニフェスト解決（tsconfig paths・package.json imports・go.mod・Rustモジュールツリー・Java/Kotlin FQCN・PHP use・Ruby require_relative）。Rust はネストした Cargo.toml をクレート境界として検出しクレート単位で解決＝モノレポ/ワークスペース対応。`mod` 宣言のみのエッジは循環依存の検出から除外され、lib.rs/mod.rs⇄子モジュールの往復は循環として報告されない。tsconfig paths / package.json imports / go.mod はスキャンルートのもののみ読むため、JS/TS/Go のネストしたサブプロジェクトのファイルを `--focus`/`focus` すると注意書き（JSON では `note` フィールド）が付く — その場合は path をサブプロジェクトにして再実行する。外部パッケージの実体は解決されず「external」扱い。C#/Swift はローカル解決なし |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない**。判定対象は `.py/.js/.jsx/.ts/.tsx/.go`（＋AST有効時の `.rs`）のみで、対象外のファイルは JSON で `has_test: null` になる。Rust の `lib.rs`/`main.rs`/`mod.rs` は名前で判定対象から免除される（re-export・配線ファイルの定番名でノイズになるため。ロジック満載の lib.rs もフラグが立たない点に注意） |
| エントリーポイント検出（`-N`） | 既知のファイル名パターン（`main.py`、`index.js`等）と`package.json`の`main`/`bin`フィールドのみで判定 |
| 設定ファイル検出（`-F`） | 既知のファイル名パターンのみで判定。独自命名の設定ファイルは拾えない |
| TODO/FIXME抽出（`-K`/`todos`） | 単語境界つき文字列マッチ。コメント外の文字列内に偶然該当語があっても拾われる場合がある |
| git連携（`-H`/`--status`/`--since`/`--api-diff`/`history`/`since`/`api_diff`） | 直近2000コミットのみ走査（`-H`）。それより古い変更しかないファイルは情報が出ない。リポジトリのサブディレクトリをスキャンルートに指定してもパスは正しく突き合わされる。`--api-diff`/`api_diff` は untracked ファイルも `(untracked)` 注記付きで含める |
| 長大関数・doc 1行目 | AST の行スパン/docstringに基づく。正規表現縮退時は出力されない |
| ディレクトリの `size`/`size_human` | 常にディスク上の生サイズ（`du`相当）で、**`-G`（gitignore除外）の影響を受けない**。子要素一覧・トークン数・解析対象は `-G` で正しく除外されるが、サイズ集計だけは対象外。`node_modules/`や`target/`等が`.gitignore`済みでも合計サイズには含まれるので、サイズだけで「大きい」と早合点しないこと |

これらの制限は「完全に間違っている」という意味ではなく、「**最終確認はファイルの中身で行うべき**」という意味である。dirlensの出力は最初の当たりをつけるための地図として使い、コードの詳細な振る舞いや正確性が重要な判断は、必ず該当ファイルを実際に読んで確認すること。

---

## 早見表（シェル CLI ⇄ MCP ツールの対応）

```bash
dirlens --agent                  # 推奨：解析全部入り（テキスト、カラーなし、英語）      → MCP: analyze
dirlens --agent --json           # 推奨：解析全部入り（JSON、パース用）                  → MCP: analyze
dirlens --agent -L 2             # 深さを2階層に制限して概要だけ把握                     → MCP: tree(depth:2)
dirlens --agent --estimate       # 階層別の出力コストを見積もる                          → MCP: analyze(estimate:true)
dirlens --agent --budget 3000    # 出力を3000トークン以内に自動調整                      → MCP: analyze(budget:3000)

dirlens --focus src/main.py -G   # このファイルの影響範囲（依存元/依存先）               → MCP: focus
dirlens --since HEAD -G          # 前回コミット以降の変更ファイルだけ                     → MCP: since
git diff --name-only | dirlens --stdin --json   # 指定ファイルだけの解析                  → MCP対応なし（シェルのみ）

dirlens -O src/main.py           # このファイルの関数/クラス一覧                         → MCP: outline(files:[...])
dirlens -A                       # プロジェクト全体の公開API一覧                         → MCP: outline(files省略)
dirlens -M                       # import依存グラフ（影響範囲調査）                      → MCP: imports
dirlens -M --mermaid             # importグラフを Mermaid 図として出力                   → MCP: imports(format:"mermaid")
dirlens -V -G                    # テスト未整備ファイルの一覧                            → MCP対応なし（`-V`相当のMCPツールは無い。`analyze`のJSONで代用可）
dirlens -K -G                    # TODO/FIXMEの棚卸し                                   → MCP: todos
dirlens -H -L 1                  # 直近の変更点を素早く把握                              → MCP: history
dirlens -F -G                    # 設定ファイルの一覧                                    → MCP対応なし（`-F`相当のMCPツールは無い。`analyze`のJSONで代用可）
dirlens -N -G                    # エントリーポイント候補                                → MCP対応なし（`-N`相当のMCPツールは無い。`analyze`のJSONで代用可）
dirlens --api-diff v1.0.0        # 公開APIの差分（破壊的変更の検出）                     → MCP: api_diff
dirlens --status                 # git status をツリーに重ねる                          → MCP対応なし（シェルのみ）
dirlens --top 10                 # 大きいファイル/ディレクトリ上位10                     → MCP: tree(top:10) / analyze(top:10)
dirlens --pack src/a.py src/b.py # ファイル内容を貼り付け用に整形                        → MCP対応なし（シェルのみ）
dirlens --compare ../v2          # 2つのディレクトリツリーを比較                         → MCP対応なし（シェルのみ）
dirlens --dupes                  # 重複ファイルの検出                                    → MCP対応なし（シェルのみ）
dirlens --check                  # この環境で使える解析方式の確認                        → MCP: analyzeの capabilities で代替可
```
