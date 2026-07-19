# dirlens 🌳

ファイルサイズ付きのディレクトリツリーを表示するコマンドラインツール。
**単一バイナリ（Rust 製）・ランタイム依存ゼロ**で、`tree` コマンドと高い互換性を持ち、
AI・コーディングエージェントがプロジェクト構造を把握するための解析機能
（トークン数・git 情報・TODO・アウトライン・import 依存グラフ・影響範囲クエリ・
トークン予算・MCP サーバー等）を備えています。人間向けにもインタラクティブ TUI・
git status オーバーレイ・ヒートカラー・重複検出・ディレクトリ比較などを搭載。

**出力はデフォルトで英語**です。日本語にするには `--lang ja`、または設定ファイル
（`~/.config/dirlens/config.toml` に `lang = "ja"`）か環境変数 `DIRLENS_LANG=ja` を使います。

> 旧 Python 実装（v1.0.x）は `python` ブランチにあります。Rust 版はゴールデンテストで
> Python 版と出力互換であることを検証したうえで、解析精度を強化しています。

---

## インストール

### npm（全 OS 推奨）

```bash
npm install -g dirlens
```

機種別のネイティブバイナリ（macOS arm64/x64・Linux arm64/x64・Windows x64）が
自動で選択されます。Windows でもネイティブな `dirlens.exe` が動作します。

### バイナリ直接ダウンロード

[GitHub Releases](https://github.com/igarinpiano/dirlens/releases) から
お使いのプラットフォームのアーカイブを取得し、PATH の通った場所へ置くだけです。

```bash
# 例: macOS (Apple Silicon)
tar -xzf dirlens-*-aarch64-apple-darwin.tar.gz
sudo install -m 755 dirlens /usr/local/bin/
```

### crates.io

```bash
cargo install dirlens
```

ソースからコンパイルされます。

### cargo-binstall

```bash
cargo binstall dirlens
```

crates.io のメタデータから GitHub Releases のビルド済みバイナリを取得します
（`[package.metadata.binstall]` 対応済み）。コンパイル不要で `cargo install` より高速です。

### ソースからビルド

```bash
git clone https://github.com/igarinpiano/dirlens
cd dirlens/rust && cargo build --release
# バイナリ: rust/target/release/dirlens
```

> **pip 版について**: `pip install dirlens` は旧 Python 実装（v1.0.x）を配布しています。
> 新機能・精度改善は Rust 版のみです。

---

## 出力例

### 基本のツリー表示

```text
Desktop/ (2 dirs, 2 files, 3.74 MB)
├── EmptyDir/ (0 dirs, 0 files, 0 bytes)
├── Project/ (2 dirs, 1 file, 712 KB)
│   ├── assets/ (1 dir, 0 files, 512 KB)
│   │   └── images/ (0 dirs, 1 file, 512 KB)
│   │       └── logo.png (512 KB)
│   ├── src/ (0 dirs, 1 file, 80 KB)
│   │   └── util.py (80 KB)
│   └── main.py (120 KB)
├── archive.zip (3 MB)
└── readme.txt (50 KB)

  Total  5 directories,  5 files
  .py ×2  .txt ×1  .zip ×1  .png ×1
```

（出力は既定で英語。`--lang ja` で「合計 5 ディレクトリ, 5 ファイル」のような日本語表示になります）

### `--ai` — AI チャット貼り付けモード

gitignore 除外（`-G`）・最終更新日時（`--date`）・Markdown コードブロック（`-m`）・クリップボードコピー（`-C`）を一発で適用します。実行した瞬間にコピーされているので、そのままチャットに貼り付けられます:

````text
```
Project/ (2 dirs, 2 files, 29.31 KB, 1 week ago)
├── assets/ (0 dirs, 1 file, 7 bytes, 1 week ago)
│   └── logo.png (7 bytes, 1 week ago)
├── src/ (0 dirs, 1 file, 478 bytes, 1 week ago)
│   └── util.py (478 bytes, 1 week ago)
├── main.py (219 bytes, 1 week ago)
└── pyproject.toml (45 bytes, 1 week ago)

  Total  2 directories,  4 files  (.gitignore applied)
  .py ×2  .png ×1  .toml ×1
```
✓ copied to clipboard
````

### `--agent` — エージェント解析モード

推定トークン数・最終コミット・TODO・テスト未整備・エントリーポイント（`*`）・設定ファイル（`⚙`）・関数/クラスのアウトライン・import 依存関係を一括で有効化します（カラーなし・クリップボード不使用、エージェントの自律実行でも安全）:

```text
Project/ (2 dirs, 2 files, 29.31 KB, 1 week ago)
├── assets/ (0 dirs, 1 file, 7 bytes, 1 week ago)
│   └── logo.png (7 bytes, 1 week ago, "feat: initial release" (10 days ago))
├── src/ (0 dirs, 1 file, 478 bytes, 1 week ago)
│   └── util.py (478 bytes, 1 week ago, ~115 tok, 21 lines, "feat: initial release" (10 days ago), TODO×1, no test, def load_config, def export_all, class Cache, def __init__, used-by×1)
├── * main.py (219 bytes, 1 week ago, ~53 tok, 11 lines, "feat: initial release" (10 days ago), no test, def main, imports×1)
└── ⚙ pyproject.toml (45 bytes, 1 week ago, ~17 tok, 3 lines, "feat: initial release" (10 days ago), config)

  Total  2 directories,  4 files  (.gitignore applied)
  .py ×2  .png ×1  .toml ×1
  Estimated tokens: ~185 tok
  Tokens by file type:
    .py ×2  ~168 tok  32 lines
    .toml ×1  ~17 tok  3 lines
  TODO/FIXME items: 1
    src/util.py:12 [TODO] # TODO: parallelize exports for large projects
  Files without tests: 2 files
  Entry point candidates: 1 found
  Config files: 1 found
  Most depended-on files (imported by many):
    src/util.py  (used by 1)
  Suggested reading order (entry points → most depended-on):
    1. main.py
    2. src/util.py
  Analysis methods: gitignore=git check-ignore (exact) / outline=AST:py,js/ts,html(embedded js),rs,go,c,java,rb,php,cs,kt,swift (regex otherwise) / imports=AST+manifest resolution / tokens=BPE(o200k) / dir sizes=raw disk (gitignore not applied)
```

`--agent --json` で同じ情報を機械可読な JSON（`schema_version` 付き）でも取得できます。

---

## 特徴

- **単一バイナリ** — Python も Node も不要。ダウンロードして置くだけで動く（macOS / Linux / Windows）
- **tree コマンドとの高い互換性** — **`-a -d -f -g -l -p -u -r -s -t -c -L -D -P -I -n -J --prune` など主要フラグが `tree` と互換**。dirlens 独自機能は `-G`（gitignore）・`-S`（サイズ順）・`-e`（拡張子）・`-C`（クリップボード）で提供
- **カラー表示** — ディレクトリ・ファイル・シンボリックリンクを色で識別
- **自動サイズ変換** — bytes / KB / MB / GB / TB
- **ディレクトリサイズ** — サブディレクトリの合計サイズを自動計算（並列プリフェッチで高速化）。**この合計は常にディスク上の生サイズであり `-G`（gitignore除外）の影響を受けない**（`du` と同様。一覧に表示される子要素・トークン数・解析対象は `-G` で正しく除外されるが、size/size_human だけは対象外。旧Python版から一貫した仕様）。v1.2.5+ は `--agent` の末尾注記と `--agent --json` の `analysis.dir_sizes` に、v1.2.9+ は `--top` / MCP `tree` の `top` の出力にもこの旨を明記
- **アイテム数表示** — 各ディレクトリの直下にある dirs / files 数を表示
- **拡張子統計** — ツリー全体のファイル種別を集計してサマリーに表示
- **`.gitignore` 対応（2層）** — `-G` で除外。**git がある環境では `git check-ignore` による厳密判定**（ネスト・否定 `!`・`**`・グローバル除外・`.git/info/exclude` すべて対応）、git 不在時は内蔵マッチャに自動縮退
- **最終更新日時** — `--date` / `-D` で相対表示
- **拡張子フィルタ** — `-e py` など指定した拡張子のみ表示
- **パターンフィルタ** — `--exclude` / `-I`、`--include` / `-P` でワイルドカード指定（複数可）
- **サイズフィルタ** — `--min-size` / `--max-size` で容量による絞り込み
- **空ディレクトリの剪定** — `--prune` でフィルタ後に空になる枝を非表示（tree --prune 互換）
- **パーミッション表示** — `-p` でパーミッション文字列、`-u` で所有者名（tree -p/-u 互換）
- **シンボリックリンク展開** — `-l` でリンク先を追跡、リンク先パスを `→` で表示（循環検出あり）
- **フルパス表示** — `-f` でルートからのパスを表示（tree -f 互換）
- **逆順ソート** — `-r` でソート順を逆転（tree -r 互換）
- **ディスク占有率バー** — `--bar` で親ディレクトリに対する占有率を視覚表示
- **絵文字アイコン** — `--emoji` で拡張子に応じた絵文字を付与
- **Markdown出力** — `-m` でコードブロック形式に出力
- **JSON出力** — `--json` / `-J` で機械可読な構造データを出力（**`schema_version` 付きの安定スキーマ**。シンボリックリンクは `symlink: {target, broken}`、アウトラインは取得方式 `outline_method` 付き・v1.2.11+）
- **HTMLレポート** — `--html` でブラウザで閲覧できる折りたたみツリーを生成
- **クリップボードコピー** — `-C` で出力を自動コピー（ANSIコードを自動除去）
- **AIチャット貼り付けモード** — `--ai` 一発で gitignore除外・日時・Markdown・クリップボードコピーを全適用（人間がコピペする用）
- **エージェント解析モード** — `--agent` 一発で下記のAI/エージェント向け解析機能を全適用（カラーなし・クリップボードは使わない、自律実行しても安全）
- **能力レポート** — `--check` で「この環境でどの解析方式が使えるか」を表示（縮退があると終了コード 1）
- **隠しファイル対応** — `-a` で表示切り替え
- **サイズ順ソート** — `-S` で大きいものから表示
- **インタラクティブ TUI** — `-i` でツリーブラウザを起動（開閉・フィルタ・詳細ペイン付き）
- **git status オーバーレイ** — `--status` で `[M]`/`[??]`/`[A]` マークをツリーに重ねる
- **ヒートカラー** — `--heat age|size|churn` でファイル名をグラデーション着色
- **大きいファイル一覧** — `--top N` でツリーなしのフラット表示（ディスク掃除向け）
- **重複ファイル検出** — `--dupes` で同一内容のファイル群と無駄容量を検出
- **ディレクトリ比較** — `--compare DIR` で2つのツリーの追加/削除/変更を表示
- **設定ファイル** — `~/.config/dirlens/config.toml` とプロジェクトの `.dirlens.toml` で
  デフォルトフラグや名前付きプリセット（`--preset`）を定義
- **シェル補完 / man** — `--completions <shell>` / `--man` で生成
- **機密ファイル警告** — `--ai`/`-C` のコピー内容に `.env`・秘密鍵らしきファイルが
  含まれる場合に stderr へ警告
- **進捗スピナー** — 時間のかかるスキャンでは端末にスピナーを表示（非端末では出ない）

### AI/エージェント向け解析機能（`--agent` でまとめて有効化）

AIチャットやコーディングエージェントがプロジェクト構造を理解する際に、ファイルの中身を
逐一読まなくても済むよう設計された機能です。個別フラグでも使えます。
**解析は「最良の方式 → 縮退」の多層構成**で、実際に使われた方式は `--check` や
`--agent --json` の `capabilities` / `analysis` ブロックで機械的に確認できます。

- **トークン数計数** — `-T` でファイルごとのトークン数を表示。**BPE（o200k_base）による正確値**（5MB 超のファイルは比例概算 — JSON では該当ファイルに `tokens_estimated: true` が付く・v1.2.9+）。縮退時は文字数ベースの概算。サマリーには**言語別トークン内訳**も表示。2回目以降は永続キャッシュにより高速（`--no-cache` で無効化）
- **git連携** — `-H` で各ファイルの最終コミット情報（メッセージ・相対日時）を表示。直近2000コミットまで走査。変更頻度の高いファイル（ホットスポット）も検出
- **TODO/FIXME抽出** — `-K` で `TODO`/`FIXME`/`HACK`/`XXX` コメントを抽出し、行番号付きで一覧表示
- **テスト欠落検知** — `-V` で対応するテストファイルが見つからないソースファイルをマーク。命名規則に加え、**テストファイルからの推移的 import を追跡**し、Rust のインラインテスト（`#[cfg(test)]`）にも対応
- **エントリーポイント検出** — `-N` で `main.py`・`index.js`・`package.json` の `main`/`bin` フィールドなどから入口ファイルを推測してマーク
- **シンボルアウトライン** — `-O` で関数・クラス名を抽出。**言語別 AST パーサ（Python / JS・TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift）による正確な抽出**、パース失敗時は正規表現に縮退。`-A` で公開 API のみに絞り込み。JSON 出力では doc コメント1行目と行範囲も付与し、サマリーには**長大関数トップ5**を表示
- **import/依存グラフ** — `-M` でファイル間のローカルな import 関係を解析し、`imports×N`（依存先数）・`used-by×N`（被参照数）・循環依存を表示。**tsconfig の `paths`・package.json の `imports`・go.mod・Rust のモジュールツリー（`crate::`/`self::`/`super::`）・Java/Kotlin の FQCN・PHP の use・Ruby の require_relative を読んで解決**
- **影響範囲クエリ** — `--focus FILE` で「このファイルを変更したら何が壊れうるか」を依存元/依存先の推移閉包で表示（`--json` 対応）
- **トークン予算** — `--budget N` でテキスト出力自体を指定トークン数以内に自動調整（自前の BPE で実測しながら深さ→詳細→ツリー行の順で削減。収まらない分は省略し「全表示に必要なトークン数」を注記）。`--estimate` で階層別コストの事前見積もりも可能
- **差分モード** — `--since REF` で指定 git ref 以降に変更されたファイルだけのツリーを表示（未追跡ファイル含む・削除ファイルは一覧表示）
- **stdin ファイルリスト** — `git diff --name-only | dirlens --stdin` のように、指定ファイルだけを解析（トークン・アウトライン・TODO。`--json` 対応）
- **公開API差分** — `--api-diff REF` で公開シンボルの追加/削除を git ref と比較（破壊的変更の検出）
- **グラフのエクスポート** — `--mermaid` / `--dot` で import グラフを図のソースとして出力、`--csv` でファイルメタデータ表を出力
- **--pack** — `--pack FILE...` で指定ファイルの中身＋トークン数を貼り付け用 Markdown に整形
- **MCP サーバー** — `--mcp` で MCP サーバーとして起動し、analyze / tree / outline / imports / focus / todos / since / history / api_diff の9ツールをエージェントがネイティブに呼べる（analyze/tree はトークン予算 `budget`・コスト見積もり `estimate`・上位表示 `top`、imports/todos は該当なしファイルを含まないフラットな一覧＋件数上限 `limit`（切り詰め時は `truncated`/`total_files` 付き・v1.2.9+。imports はさらに `format: mermaid/dot`）、outline は複数ファイル一括（解決不能パスは `errors` に報告・重複は dedupe・v1.2.9+。ネストしたシンボルに `parent` 付き・v1.2.10+。スキャンルート外のファイルも指定可で正規化済み絶対パスで返る・v1.2.12+）と `files` 省略時の公開API出力に対応、api_diff は untracked ファイルも `(untracked)` 注記付きで含む（v1.2.12+）、tree/analyze は `include_ignored` で gitignore 除外を外せる・v1.2.10+）。多くの MCP ホストは 1 応答に上限がある（Claude Code 既定 25,000 トークン）ため、`estimate` の結果が上限を超える場合は上限未満の `budget` を指定する（v1.2.6+ は見積もり表の該当行に `⚠ exceeds host cap` マークと上限値・推奨 `budget` が表示される。上限は `MAX_MCP_OUTPUT_TOKENS` があればその値、無ければ Claude Code 既定の 25000 を仮定。v1.2.9+ は見積もり表に「全階層・テキスト（budget 指定時の形式）」行も併記され、テキストなら収まる場合はヒントが出る）。**設定は `dirlens --mcp-setup` が案内**（バイナリの絶対パス入りで Claude Code のワンライナー・Claude Desktop / Cursor の設定 JSON を出力するので、コピペで完了）
- **設定ファイル検出** — `-F` で `.env`・`tsconfig.json`・`Makefile` 等の設定ファイルを検出して一覧表示
- **構造化エラー** — `--json` は部分的な解析失敗があっても常に valid な JSON を返し、`errors` 配列で機械可読に報告

---

## 使い方

```bash
# ── AI チャットへの貼り付け（人間がコピペする用）──────────────
dirlens --ai             # gitignore除外 + 日時 + Markdown + クリップボードコピー
dirlens --ai -L 3        # 深さ指定と組み合わせ可

# ── エージェント向け解析（自律実行向け・カラーなし・クリップボードは使わない）──
dirlens --agent          # トークン数・git情報・TODO・テスト欠落・エントリーポイント・
                          # アウトライン・import依存グラフ・設定ファイル一覧を一括表示
                          # （--agent は --no-color を兼ねるので単体でOK）
dirlens --agent --json   # 同上をJSON形式で（スクリプト/エージェント連携向け）
dirlens --check          # この環境で使える解析方式を確認（縮退があると終了コード 1）

# ── AI/エージェント向け解析（個別フラグ）────────────────────────
dirlens -T                # ファイルごとのトークン数（BPE 正確値）
dirlens -H                # 最終コミット情報（要git）
dirlens -K                # TODO/FIXME/HACK/XXXを抽出
dirlens -V                # テストが無いソースファイルを表示
dirlens -N                # エントリーポイントらしきファイルをマーク
dirlens -O                # 関数・クラスのアウトライン（AST）
dirlens -O src/main.py    # 単一ファイルのアウトライン（トークン数・TODO も併記。v1.2.5+）
dirlens -A                # 公開APIのみのアウトライン
dirlens -M                # ローカルなimport/依存関係を解析
dirlens -F                # 設定ファイル（.env, tsconfig.json等）を検出して一覧表示

# ── コンテキスト効率・影響範囲（エージェント向け）────────────────
dirlens --agent --estimate       # 階層別の出力コストを見積もる（--budget の値決めに）
dirlens --agent --budget 3000    # 出力を3000トークン以内に自動調整
                                  # （収まらない分は「… N more entries」と省略し、
                                  #   全表示に必要なトークン数を注記する）
dirlens --focus src/main.py -G   # このファイルの依存元/依存先（直接+推移）
dirlens --since HEAD -G          # 前回コミット以降に変更されたファイルだけ
git diff --name-only | dirlens --stdin --json   # 指定ファイルだけを解析
dirlens --api-diff v1.0.0        # 公開APIの差分（破壊的変更の検出）
dirlens --mcp-setup              # MCP の設定手順を表示（コピペで登録完了）
dirlens --mcp                    # MCP サーバーとして起動（stdio）

# ── 人間向けの便利モード ──────────────────────────────────────
dirlens -i                # インタラクティブ TUI ブラウザ
dirlens --status          # git status のマークをツリーに重ねる
dirlens --heat age        # 更新の新しさでグラデーション着色（size/churn も可）
dirlens --top 10          # 大きいファイル/ディレクトリ上位10をフラット表示
dirlens --dupes           # 重複ファイルの検出（サイズ+内容ハッシュ）
dirlens --compare ../v2   # 2つのディレクトリツリーを比較
dirlens --pack src/a.py src/b.py -C   # ファイル内容を貼り付け用に整形してコピー

# ── グラフ・表のエクスポート ──────────────────────────────────
dirlens -M --mermaid      # importグラフを Mermaid 形式で出力
dirlens -M --dot          # importグラフを Graphviz DOT 形式で出力
dirlens --csv -T -G       # ファイルメタデータを CSV で出力

# ── 言語・設定 ────────────────────────────────────────────────
dirlens --lang ja         # 日本語出力（デフォルトは英語）
dirlens --preset quick    # 設定ファイルの [presets] を適用
dirlens --no-config       # 設定ファイルを無視
dirlens --completions zsh > ~/.zfunc/_dirlens   # シェル補完の生成
dirlens --man             # man ページ（roff）の生成

# ── 表示制御 ──────────────────────────────────────────────────
dirlens                  # カレントディレクトリ
dirlens ~/Desktop        # 指定ディレクトリ
dirlens -L 2             # 深さ2階層まで（tree -L 互換）
dirlens -d               # ディレクトリのみ表示（tree -d 互換）
dirlens -a               # 隠しファイルも表示
dirlens -r               # 逆順ソート（tree -r 互換）
dirlens --filesfirst     # ファイルをディレクトリより先に表示
dirlens -f               # ルートからのフルパスで表示（tree -f 互換）

# ── フィルタリング ────────────────────────────────────────────
dirlens -G               # .gitignore のファイルを除外
dirlens -G --prune       # gitignore除外 + 空になった枝を剪定
dirlens -e py            # .py のみ表示
dirlens -P '*.md'        # .md のみ表示（tree -P 互換）
dirlens -I '*.log'       # .log を除外（tree -I 互換）
dirlens --exclude 'dist' --exclude '*.log'   # 複数除外
dirlens --min-size 1M    # 1MB 以上のファイルのみ
dirlens --max-size 100K  # 100KB 以下のファイルのみ

# ── ソート ────────────────────────────────────────────────────
dirlens -S               # サイズの大きい順に表示
dirlens -t               # 更新日時順にソート（新しい順・tree -t 互換）
dirlens -c               # ステータス変更日時順にソート（tree -c 互換）
dirlens -t -r            # 更新日時順・古い順

# ── 詳細情報 ──────────────────────────────────────────────────
dirlens --date           # 最終更新日時を相対表示（tree -D 互換は -D）
dirlens --bar            # ディスク占有率バーを表示
dirlens --emoji          # 絵文字アイコンを表示
dirlens -p -u -g         # パーミッション・所有者・グループを表示（tree 互換）
dirlens -l               # シンボリックリンク先を展開（tree -l 互換）

# ── 出力形式 ──────────────────────────────────────────────────
dirlens -m               # Markdown コードブロック形式で出力
dirlens --json           # JSON 形式（tree -J 互換は -J）
dirlens --html           # HTML レポートを生成（デフォルト: dirlens.html）
dirlens -C               # クリップボードにコピー
dirlens --no-color > dirlens.txt   # テキストファイルに書き出す
```

---

## オプション一覧

| オプション           | 省略形        | 説明                                                        |
|---------------------|--------------|-------------------------------------------------------------|
| `path`              | —            | 対象ディレクトリ（省略時はカレント）                           |
| **`--ai`**          | —            | **`-G --date -m -C --status` のショートカット。人間がAIチャットに貼り付ける用** |
| **`--agent`**       | —            | **`-G --date -T -H -K -V -N -O -M -F --status --no-color` のショートカット。エージェント向け解析（カラーなし・クリップボードは使わない）** |
| **`--check`**       | —            | **能力レポートを表示。縮退があると終了コード 1（`--json` 併用可）** |
| `--depth N`         | `-L N`       | 表示する最大の深さ                                            |
| `--all`             | `-a`         | 隠しファイル・ディレクトリも表示                               |
| `-d`                | —            | ディレクトリのみ表示                                          |
| `--sort-size`       | `-S`         | サイズが大きい順に並べる                                       |
| `-s`                | —            | サイズ表示（tree互換・常時表示されているため実質no-op）        |
| `-t`                | —            | 更新日時順にソート（新しい順）                                 |
| `-c`                | —            | ステータス変更日時順にソート                                   |
| `--reverse`         | `-r`         | ソート順を逆にする                                            |
| `--filesfirst`      | —            | ファイルをディレクトリより先に表示                              |
| `--gitignore`       | `-G`         | `.gitignore` に記載されたファイルを除外（2層・下記参照）        |
| `--prune`           | —            | フィルタ後に空になるディレクトリを非表示                       |
| `--date`            | `-D`         | 最終更新日時を相対表示                                        |
| `--perms`           | `-p`         | パーミッション文字列を表示                                     |
| `--user`            | `-u`         | 所有者のユーザー名を表示                                      |
| `-g`                | —            | グループ名を表示                                              |
| `--follow`          | `-l`         | シンボリックリンク先ディレクトリを展開（循環検出あり）           |
| `--full-path`       | `-f`         | ルートからのフルパスで表示                                     |
| `--type EXT`        | `-e EXT`     | 指定した拡張子のファイルのみ表示（例: `-e py`）                |
| `--include PATTERN` | `-P PATTERN` | このパターンのみ表示（複数指定可）                              |
| `--exclude PATTERN` | `-I PATTERN` | 除外パターン（複数指定可）                                     |
| `--min-size SIZE`   | —            | 指定サイズ以上のファイルのみ表示（例: `1M`, `500K`）           |
| `--max-size SIZE`   | —            | 指定サイズ以下のファイルのみ表示                               |
| `--bar`             | —            | ディスク占有率バーを表示                                       |
| `--emoji`           | —            | 拡張子に応じた絵文字アイコンを表示                             |
| `--tokens`          | `-T`         | ファイルごとのトークン数を表示（BPE 正確値・縮退時は概算）      |
| `--git`             | `-H`         | 最終コミット情報を表示（要git、直近2000コミットまで走査）        |
| `--todo`            | `-K`         | TODO/FIXME/HACK/XXXコメントを抽出                             |
| `--missing-tests`   | `-V`         | 対応するテストファイルが見つからないソースファイルを表示          |
| `--entry`           | `-N`         | エントリーポイントらしきファイルを検出してマーク                 |
| `--outline`         | `-O`         | 関数・クラスのアウトラインを表示（AST・失敗時は正規表現）        |
| `--imports`         | `-M`         | ローカルなimport/依存関係を解析して表示（外部パッケージは対象外） |
| `--api`             | `-A`         | 公開API（exportされたシンボル）のみに絞り込む（`-O` を自動有効化） |
| `--config`          | `-F`         | 設定ファイル（.env, tsconfig.json, Makefile等）を検出して一覧表示 |
| `--markdown`        | `-m`         | Markdown コードブロック形式で出力（カラー自動無効）             |
| `--json`            | `-J`         | JSON 形式で標準出力に出力（`schema_version` 付き）             |
| `--html [FILE]`     | —            | HTML レポートを生成（デフォルト: `dirlens.html`。検索・全展開/折りたたみ・サイズバー・ライト/ダークテーマ・大きいファイル一覧つき） |
| `--copy`            | `-C`         | 出力をクリップボードにコピー（ANSIコードを自動除去）            |
| `--no-color`        | `-n`         | カラー表示を無効化                                            |
| **`--interactive`** | **`-i`**     | **インタラクティブ TUI ブラウザを起動**                        |
| `--lang LANG`       | —            | 出力言語（`en`/`ja`。既定: 英語）                              |
| `--budget N`        | —            | テキスト出力を N トークン以内に自動調整（深さ→詳細→ツリー行の順で削減。省略時は必要トークン数を注記） |
| `--estimate`        | —            | 階層別の出力トークンコストを見積もる（`--budget` の値決めに）     |
| `--focus FILE`      | —            | 影響範囲クエリ（依存元/依存先の推移閉包。`-M` を暗黙有効化）     |
| `--since REF`       | —            | 指定 git ref 以降に変更されたファイルのみ表示（未追跡含む）      |
| `--stdin`           | —            | stdin のファイルリスト（1行1ファイル）だけを解析                |
| `--status`          | —            | git status のマーク（`[M]`/`[??]`/`[A]`）をツリーに重ねる      |
| `--heat MODE`       | —            | ファイル名を `age`/`size`/`churn` でグラデーション着色          |
| `--top N`           | —            | 大きいファイル/ディレクトリ上位 N をフラット表示（ツリーなし）   |
| `--dupes`           | —            | 重複ファイルを検出（サイズ + 内容ハッシュの2段）                |
| `--compare DIR`     | —            | 対象ツリーと DIR を比較（追加/削除/変更・サイズ差分）           |
| `--api-diff REF`    | —            | 公開APIシンボルを git ref と比較（追加/削除を列挙）             |
| `--pack FILE...`    | —            | ファイル内容＋トークン数を貼り付け用 Markdown に整形（複数可）   |
| `--mermaid` / `--dot` | —          | import グラフを Mermaid / Graphviz DOT で出力（`-M` を暗黙有効化） |
| `--csv`             | —            | ファイルメタデータを CSV で出力                                |
| `--mcp`             | —            | MCP サーバーとして stdio で起動                                |
| `--mcp-setup [HOST]` | —           | MCP の登録手順を絶対パス入りで表示（claude-code / claude-desktop / cursor） |
| `--preset NAME`     | —            | 設定ファイルの `[presets]` で定義した引数セットを適用           |
| `--no-config`       | —            | 設定ファイルを読まない（`DIRLENS_CONFIG=off` と同じ）          |
| `--no-cache`        | —            | トークン計数の永続キャッシュを使わない（`DIRLENS_CACHE=off` と同じ） |
| `--completions SHELL` | —          | シェル補完スクリプトを生成（bash/zsh/fish/powershell/elvish）   |
| `--man`             | —            | man ページ（roff）を生成                                       |
| `--version`         | —            | バージョンを表示（`-V` は `--missing-tests` のため使用不可）   |

---

## カラーの意味

| 表示色         | 意味                   |
|---------------|------------------------|
| 青（太字）     | ルートディレクトリ       |
| シアン（太字） | サブディレクトリ         |
| 緑            | ファイル                |
| マゼンタ       | シンボリックリンク       |
| 暗色（dim）   | サイズ表示              |

---

## 解析方式と精度について

dirlens の解析は**「最良の方式を試し、使えない環境では自動的に縮退する」多層構成**です。
いまどの方式が使われているかは `dirlens --check` で確認できます
（`--agent --json` の `capabilities` / `analysis` ブロックでも機械的に取得可能）。

| 機能 | 第1層（最良） | 縮退層 | 備考 |
|---|---|---|---|
| `.gitignore`（`-G`） | `git check-ignore`（本物の git エンジン。ネスト・`!` 否定・`**`・グローバル除外・`.git/info/exclude` 完全対応） | 内蔵マッチャ（基本パターンのみの近似） | git 不在・非リポジトリで縮退 |
| トークン数（`-T`） | BPE（o200k_base）による正確値。データはバイナリに同梱。結果は `~/.cache/dirlens/` に永続キャッシュ | 文字数ベースの概算 | 5MB 超のファイルは第1層でも比例概算（JSON では `tokens_estimated: true` が付く・v1.2.9+）。モデルによりトークナイザは異なるため、他社モデルでは目安。通常ファイル以外（FIFO・ソケット・デバイス）は読まずサイズ 0 扱い（v1.2.12+。v1.2.11 以前は FIFO を含むディレクトリで永久にブロックした） |
| アウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python=rustpython / JS・TS=oxc / Rust=syn / Go・C・Java・Ruby・PHP・C#・Kotlin・Swift=tree-sitter）。HTML はインライン `<script>` 内の JS を抽出してアウトライン（v1.2.5+・`src` 付き外部スクリプトは対象外）。文字列内の偽検出なし。doc 1行目・行範囲も取得。Python の `public` 判定はスコープ対応（v1.2.9+）: 関数内のローカル定義とそのメンバは非公開、クラスメソッドはクラス自身が公開の場合のみ名前で判定。ネストしたシンボルには外側シンボル名が付く（JSON は `parent`、テキストは `def Class.method` / `fn Type::method` 表示・v1.2.10+）。JSON の `outline_method`（"ast"/"regex"）でどちらの層かを判別できる（v1.2.11+） | 正規表現による簡易抽出（Python/JS・TS/Go/Rust のみ。doc・行範囲なし・公開判定は名前のみ） | 構文エラーのあるファイルは自動で縮退層へ |
| import グラフ（`-M`/`--focus`） | AST/構文抽出 + マニフェスト解決（tsconfig `paths`/`baseUrl`・package.json `imports`・go.mod・Rust モジュールツリー・Java/Kotlin FQCN・PHP `use`/`require`・Ruby `require_relative`）。Rust はネストした Cargo.toml をクレート境界として検出しクレート単位で解決＝モノレポ/ワークスペース対応（v1.2.7+）。`mod` 宣言のみのエッジは循環依存の検出から除外（v1.2.7+） | 正規表現 + 相対パス解決 | node_modules 等の外部パッケージ実体は対象外（external 扱い）。C#/Swift はローカル解決なし。tsconfig/package.json imports/go.mod はスキャンルートのもののみ読むため、ネストした JS/Go サブプロジェクトの `--focus` には注意書きが付く |
| テスト欠落検知（`-V`） | 命名規則 + テストファイルからの推移的 import 追跡 + Rust インラインテスト検出 | 命名規則のみ | 実際のカバレッジは見ていない。判定対象は `.py/.js/.jsx/.ts/.tsx/.go`（＋AST有効時の `.rs`）のみで、対象外のファイルは `--json` で `has_test: null`（v1.2.5+）。Rust の `lib.rs`/`main.rs`/`mod.rs` は名前で判定対象から免除される（re-export・配線ファイルの定番名でノイズになるため。ロジック満載の lib.rs もフラグが立たない点に注意） |
| エントリーポイント（`-N`） | 既知のファイル名 + `package.json` の `main`/`bin` | — | |
| TODO 抽出（`-K`） | 単語境界つき文字列マッチ | — | コメント外の文字列内の該当語も拾われることがある |
| git 連携（`-H`/`--status`/`--since`/`--api-diff`） | git コマンド実行（`-H` は直近 2000 コミットを走査）。リポジトリのサブディレクトリをスキャンルートに指定した場合も、git のパス（リポジトリルート相対）をスキャンルート相対へ変換して正しく突き合わせる（v1.2.8+）。`--api-diff` は untracked ファイルも `(untracked)` 注記付きで含める（v1.2.12+） | — | それより古い変更しかないファイルは情報が出ない。v1.2.7 以前はサブディレクトリ指定時にコミット注釈の欠落・同名ファイルの誤った履歴・`--since` の空振り・`--api-diff` の全削除誤報が起きた。v1.2.11 以前の `--api-diff` は git add するまで新規ファイルの公開 API が差分に現れなかった |

### 環境変数

| 変数 | 効果 |
|---|---|
| `DIRLENS_LANG=ja` | 出力言語を日本語にする（`--lang` が優先） |
| `DIRLENS_CONFIG=off` | 設定ファイルを一切読まない（`--no-config` と同じ） |
| `DIRLENS_CACHE=off` | トークン計数の永続キャッシュを無効化（`--no-cache` と同じ） |
| `DIRLENS_GITIGNORE=builtin` | gitignore を内蔵マッチャに固定 |
| `DIRLENS_AST=off` | AST 解析を無効化し正規表現層に固定 |
| `DIRLENS_TOKENS=heuristic` | トークン計数を文字数概算に固定 |
| `DIRLENS_COMPAT=python` | 上記の縮退すべて＋日本語出力＋精度注記/`schema_version` 抑止（旧 Python 版とバイト一致になる検証用モード） |

### 設定ファイル

グローバル `~/.config/dirlens/config.toml`（`$XDG_CONFIG_HOME` 対応）と、
対象ディレクトリから上方向に探索した最初の `.dirlens.toml`（プロジェクト設定）を読み込みます。
優先順は **CLI フラグ > プロジェクト設定 > グローバル設定**。

```toml
# ~/.config/dirlens/config.toml の例
lang = "ja"          # 出力言語
gitignore = true     # 常に -G
emoji = true         # 常に --emoji
exclude = ["dist"]   # 常に除外するパターン

[presets]            # dirlens --preset quick で適用
quick = ["-L", "2", "-G"]
paste = ["--ai", "-L", "3"]
```

対応キー: `lang` / `gitignore` / `all` / `date` / `emoji` / `markdown` / `no_color` /
`bar` / `prune` / `filesfirst` / `follow` / `full_path` / `depth` / `min_size` /
`max_size` / `exclude` / `include` / `[presets]`

---

## 仕様・注意事項

- ディレクトリのサイズは **全サブファイルの合計**（隠しファイルを含む、`.gitignore` 対象も含む）
- ディレクトリサイズはルート直下を **並列プリフェッチ** して高速化（透過的な最適化）
- **`+` 表記** — 一部のサブディレクトリが読めなかった場合、サイズは `1.5+ KB`（少なくとも 1.5 KB）、件数は `3+ dirs` のように表示
- **アクセス拒否** — 読めないディレクトリは赤太字で `[アクセス拒否]` を表示してスキップ
- **シンボリックリンク** は `→ リンク先パス` で表示。`-l` でリンク先ディレクトリを展開（循環検出あり）
- **ホームフォルダ（`~/`）やルート（`/`）で実行すると時間がかかる場合があります** — サイズ計算は表示深さに関わらず底まで全再帰するため
- **`-L`（深さ制限）はツリー表示を浅くするだけ** — TODO件数・推定トークン数・言語別内訳・長大関数などの解析集計はプロジェクト全体を反映する（v1.2.5+。`Total N directories, M files` と拡張子出現数だけは `tree` 互換の「表示分」集計）
- `--json` の出力スキーマは**安定した公開 API** として運用します。トップレベルの `schema_version` はフィールドの改名・削除・型変更時のみ上がります（追加は後方互換）
- `-p`（パーミッション）・`-u`（ユーザー名）・`-g`（グループ名）は macOS / Linux で完全対応（Windows では属性からの近似値と ID 番号を表示）

### AIエージェントへの指示テンプレート

エージェント（Claude Code・Cursor等）にプロジェクト探索の手順として `dirlens --agent` を
使わせたい場合、`AGENT_RULE.md` のテンプレートを `CLAUDE.md`・`.cursorrules` 等の
グローバルルールファイルにそのまま貼り付けて使えます。

---

## 開発

```text
rust/
├── crates/dirlens-core/   # 解析コア（I/O 抽象トレイト経由・native / wasm 両対応）
├── crates/dirlens-cli/    # CLI（clap・std プロバイダ・並列プリフェッチ）
└── crates/dirlens-wasm/   # wasm バインディング（ホスト供給ツリーを解析）
tests/golden/              # ゴールデンテスト（スナップショット照合・層別の敵対的検証）
```

```bash
cd rust && cargo build --release && cargo test --workspace
python3 tests/golden/run.py verify --bin rust/target/release/dirlens   # スナップショット照合
python3 tests/golden/tier_check.py --bin rust/target/release/dirlens   # gitignore 2層の検証
python3 tests/golden/ast_check.py  --bin rust/target/release/dirlens   # AST 2段の検証
```

旧 Python 実装との互換性検証（`run.py live`）や意図的な差分の台帳は
`tests/golden/README.md` / `tests/golden/DELTAS.md` を参照してください。

---

## ライセンス

[Apache License 2.0](LICENSE) の下で公開しています。利用・再配布・改変が可能ですが、著作権表示と `NOTICE` の保持が必要です。詳細は `LICENSE` を参照してください。

トークン計数には [tiktoken](https://github.com/openai/tiktoken) の o200k_base 語彙データ（MIT License）を同梱しています。

Copyright 2026 Igarin
