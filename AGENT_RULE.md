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

5. **`-L`（深さ制限）はツリー表示を浅くするだけで、集計（TODO件数・テスト未整備数・エントリーポイント数・依存度ランキング等）はプロジェクト全体のスキャン結果を反映する**
   深い階層の概要だけサッと見たいときは `-L 2` 等を使ってよいが、ツリーの「見た目」が浅くなるだけで、サマリーの数値自体は省略されない。

6. **コンテキスト節約用のモードを知っておく（Rust 版 v1.2+）**

   ```bash
   dirlens --agent --estimate                     # 階層別の出力コストを見積もる（--budget の値決めに）
   dirlens --agent --budget 3000                  # 出力を3000トークン以内に自動調整
   dirlens --since HEAD -G                        # 前回コミット以降の変更ファイルだけ
   git diff --name-only | dirlens --stdin --json  # 指定ファイルだけの解析
   dirlens --focus src/app.py -G                  # このファイルの影響範囲（依存元/依存先）
   ```

   出力はデフォルトで英語（`--lang ja` で日本語）。MCP ホストでは
   `claude mcp add dirlens -- dirlens --mcp` でネイティブツールとしても登録できる。

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
| トークン数（`-T`） | BPE（o200k_base）による正確値。5MB 超は比例概算。他社モデルのトークナイザでは目安 |
| シンボルアウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）。構文エラーのあるファイルは正規表現に縮退し、取得漏れがありうる |
| import依存グラフ（`-M`/`--focus`) | AST 抽出＋マニフェスト解決（tsconfig paths・package.json imports・go.mod・Rustモジュールツリー・Java/Kotlin FQCN・PHP use・Ruby require_relative）。外部パッケージの実体は解決されず「external」扱い。C#/Swift はローカル解決なし |
| テスト欠落検知（`-V`） | 命名規則＋テストファイルからの推移的 import＋Rust インラインテスト検出。**実際のテストカバレッジは見ていない** |
| エントリーポイント検出（`-N`） | 既知のファイル名パターンと`package.json`の`main`/`bin`フィールドのみで判定 |
| 設定ファイル検出（`-F`） | 既知のファイル名パターンのみで判定。独自命名の設定ファイルは拾えない |
| TODO/FIXME抽出（`-K`） | 単語境界つき文字列マッチ。コメント外の文字列内に偶然該当語があっても拾われる場合がある |
| git連携（`-H`/`--status`/`--since`） | 直近2000コミットのみ走査（`-H`）。それより古い変更しかないファイルは情報が出ない |

これらの制限は「完全に間違っている」という意味ではなく、「**最終確認はファイルの中身で行うべき**」という意味である。dirlensの出力は最初の当たりをつけるための地図として使い、コードの詳細な振る舞いや正確性が重要な判断は、必ず該当ファイルを実際に読んで確認すること。
