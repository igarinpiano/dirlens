# dirlens エージェント向けルール（CLI + MCP 併用版）

このファイルは、シェル上の `dirlens` バイナリと、MCP 経由の `dirlens` ツール（`analyze`/`tree`/`outline`/`imports`/`focus`/`todos`/`since`/`history`/`api_diff`）の**両方**が使える環境を前提にした版である。片方しか無い環境では [AGENT_RULE_STRICT.md](AGENT_RULE_STRICT.md)（CLI前提・常時稼働扱い）または [AGENT_RULE.md](AGENT_RULE.md)（存在確認とフォールバック重視のテンプレート）を使うこと。

**基本姿勢**: dirlens は「毎回使ってよい第一候補のツール」として扱う。存在確認や失敗時の様子見に時間を使うより、まず叩いてみる方が速い。ただし出力は無条件に正しいとは限らない — 特に依存グラフ・テスト検知・エントリーポイント検出は既知のヒューリスティックで動いており縮退もある（後述）。**「頻繁に使うが、鵜呑みにはしない」**がこの版の立ち位置。

---

## 1. 存在確認は初回だけでよい

MCP ツール一覧に `analyze`/`tree`/`outline` 等が見えている時点で dirlens が使える環境であることは確定している。TEMPLATE 版のように毎回 `command -v dirlens` を叩く必要はない。ただし以下は要注意:

- MCP ツールが見えているプロジェクトと、これから作業するプロジェクトが違う場合（モノレポの別ディレクトリ、別リポジトリ）、シェルの `dirlens` バイナリ自体は同じでも MCP サーバーの起動時 `cwd`／スキャンルートが古いままのことがある。`analyze`/`tree` の `path` 引数で明示的にスキャン対象を指定するか、疑わしければ一度シェルで `dirlens --agent` を叩いて突き合わせる。
- MCP ツールが今のプロジェクトで見えない（他のプロジェクトでは見えていた）場合、真っ先に疑うのは登録スコープ。`claude mcp add` に `-s user` を付け忘れると local scope になり、登録したディレクトリでしか有効にならない（[AGENT_RULE_STRICT.md](AGENT_RULE_STRICT.md) 参照）。dirlens 自体が壊れていると決めつけてリトライを繰り返さず、シェルの `dirlens --agent` にフォールバックして作業を続ける。

---

## 2. CLI と MCP の使い分け（この版の主眼）

同じ情報を取る手段が2つあるとき、どちらを使うべきかの判断基準。

**MCP ツールを優先する場面（通常はこちら）**

- `analyze`（= `--agent --json`）/ `tree` / `outline` / `imports` / `focus` / `todos` / `since` / `history` / `api_diff` に対応する調査。シェル呼び出しのクォーティング事故が無く、構造化データがそのまま返るため後段のパース処理が要らない。
- 同一セッション内で繰り返し呼ぶ場合、MCP はプロセス起動コストが無い分シェル経由よりわずかに速い。

**シェル CLI に切り替える場面**

- MCP に**無い**機能を使うとき: `--pack` / `--compare` / `--dupes` / `--heat` / `--csv` / `--mermaid`・`--dot` をファイル出力したい場合 / `-i` インタラクティブ / stdin パイプ（`git diff --name-only | dirlens --stdin`）/ `--completions` / `--man` / `--clear-cache`。
- 見積もりの時点で出力がホストの1応答上限（Claude Code 既定 25,000トークン）を超えると分かっている、かつそのボリュームが本当に必要な場合。MCP 経由だと `budget` で削っても上限自体は動かせないが、シェルなら `dirlens --agent --budget N` を直接実行してファイルやページャに流せる（応答サイズの制約を受けるのは「MCP 経由でチャットに返す」場合であって、dirlens 自体の出力サイズではない）。
- セッション途中で dirlens のバイナリを更新した、あるいは MCP サーバー側がエラーを返し続ける等、その回の MCP 呼び出しが明らかに不調なとき。チャット内からは MCP サーバーを再起動できないため、同じツール呼び出しに固執せず残りはシェル CLI で続行する。

**両方を同時に叩かない**

同じ問い合わせを「念のため」CLI と MCP の両方で実行しない。1タスクにつき上の基準でどちらか一方を選ぶ。MCP 呼び出しがエラーになった場合は、まず同じ内容をシェルで一度だけ試してから切り分ける（dirlens 自体のバグより、登録スコープや環境変数の差異が原因であることの方が多い）。

---

## 3. 環境変数・キャッシュは共有されるが、プロセスの env が違うことがある

MCP サーバーは CLI と同じバイナリ・同じプロセス実装で動くため、`DIRLENS_MAX_FILE_BYTES`/`DIRLENS_MAX_WORKERS`/`DIRLENS_GITIGNORE`/`DIRLENS_AST`/`DIRLENS_TOKENS`/`DIRLENS_COMPAT`/`DIRLENS_CACHE` は MCP 経由の呼び出しにもそのまま適用され、永続トークンキャッシュも共有される（v1.2.17+）。

ただし **プロセスが継承する環境変数はシェルと MCP ホストで別**であることに注意する。対話シェルではシェルプロファイル（`.zshrc` 等）で export した `DIRLENS_*` が効くが、MCP ホスト（特に GUI アプリを Finder/Dock から起動した場合）はそれらを継承していないことがある。同じフラグで CLI と MCP の結果が食い違ったら、まず `--check`（シェル側）と `analyze`/`capabilities`（MCP側）を突き合わせて `max_file_bytes` 等の実効値が一致しているか確認する。dirlens のバグを疑うのはその後でよい。

---

## 4. 出力の正確さについて（トランスポートに関係なく共通）

同じバイナリ・同じ解析エンジンなので、以下の制限は CLI 経由でも MCP 経由でも同一に適用される。「MCP から返ってきたから CLI より正確/公式」ということはない。

| 機能 | 方式と制限事項 |
|---|---|
| トークン数（`-T` / `analyze`） | BPE（o200k_base）による正確値。1ファイルあたりの読み込み上限（既定5MB、v1.2.17+ はホストの物理メモリ量に応じて段階的に引き上がる）を超えると比例概算（JSON では `tokens_estimated: true`）。実効上限は `--check`/`capabilities.max_file_bytes` で確認できる。他社モデルのトークナイザでは目安 |
| シンボルアウトライン（`-O`/`-A`/`outline`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。構文エラー時は正規表現に縮退し取得漏れがありうる（`outline_method`: "ast"/"regex" で判別可） |
| import依存グラフ（`-M`/`--focus`/`imports`/`focus`） | AST 抽出＋マニフェスト解決。外部パッケージは実体解決されず「external」扱い。C#/Swift はローカル解決なし。JS/TS/Go のネストしたサブプロジェクトを `--focus`/`focus` すると解決が甘くなる場合があり、その旨の注記（`note`）が付く |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない**。対象外の拡張子は `has_test: null` |
| エントリーポイント検出（`-N`） | 既知のファイル名パターンのみで判定。独自の起動方式は拾えない |
| TODO/FIXME抽出（`-K`/`todos`） | 単語境界つき文字列マッチ。コメント外の文字列内の偶然の一致も拾われうる |
| git連携（`-H`/`--since`/`--api-diff`/`history`/`since`/`api_diff`） | 直近2000コミットのみ走査。それより古い変更しかないファイルは情報が出ない |
| ディレクトリの `size`/`size_human` | 常にディスク上の生サイズで `-G`（gitignore除外）の影響を受けない。`node_modules/` 等が除外設定でもサイズ集計には含まれる |

**最終確認はファイルの中身で行う。** dirlens の出力（CLI・MCP どちらでも）は最初の当たりをつけるための地図であって、コードの詳細な振る舞いや正確性が重要な判断は必ず該当ファイルを実際に読んで確認すること。

---

## 5. MCP 固有の注意点（シェルには無い制約）

- `analyze`/`tree` は大きなプロジェクトで応答が肥大化しやすい。`estimate: true` で見積もり→ホスト上限（既定25,000トークン、`⚠ exceeds host cap` 表示）を超えるなら上限未満の `budget` を指定する。`budget` 指定時は JSON ではなく注釈付きテキストで返る点に注意（構造化データが欲しいならまず budget 無しで見積もりを確認し、収まる depth を探すか、諦めてシェル CLI に切り替える）。
- `outline`（`files` 省略）と `history` は `depth` を省略すると小さい既定値（それぞれ2/1）に制限される。全階層が要るなら `depth` を明示するか `unlimited_depth: true`。
- `imports`/`todos` は該当なしファイルを含まないフラットな配列を返す（CLI の `-M`/`-K` は全ツリー注釈形式なので出力形が異なる）。`limit` で件数上限、切り詰め時は `truncated`/`total_files` が付く。
- gitignore 済みディレクトリを `path` に指定すると既定で空のツリーが返る（`include_ignored: true` で中身を見る）。
- MCP サーバーはクリップボード無効固定（`capabilities.clipboard: false` は正常）。
- MCP に無いもの（4項に記載の一覧と同じ）が必要なときは、そこで初めてシェルに切り替えるのが正しい判断であり、無理に MCP だけで完結させようとしない。

---

## 6. 早見表（同じ操作の CLI ⇄ MCP 対応）

| やりたいこと | シェル CLI | MCP ツール |
|---|---|---|
| 解析全部入り | `dirlens --agent --json` | `analyze` |
| ツリー表示 | `dirlens --agent` / `dirlens -L 2` | `tree`（`budget`/`top`） |
| 関数/クラス一覧 | `dirlens -O <path>`（単一ファイルのみ） | `outline`（複数ファイル配列・`files`省略で`-A`相当） |
| import依存グラフ | `dirlens -M` | `imports`（`format: mermaid/dot`可） |
| 影響範囲クエリ | `dirlens --focus <path> -G` | `focus` |
| TODO棚卸し | `dirlens -K -G` | `todos` |
| 差分だけ取得 | `dirlens --since HEAD -G` | `since` |
| 最近のコミット履歴 | `dirlens -H` | `history` |
| 公開APIの差分 | `dirlens --api-diff <ref>` | `api_diff` |
| `--pack`/`--compare`/`--dupes`/`--heat`/`--csv`/stdin/`-i` | シェルのみ | 非対応 |

登録手順は `dirlens --mcp-setup` を参照（バイナリの絶対パス入りで各ホスト向けの手順を出力する）。
