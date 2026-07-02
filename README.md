# dirlens 🌳

ファイルサイズ付きのディレクトリツリーを表示するコマンドラインツール。
**単一バイナリ（Rust 製）・ランタイム依存ゼロ**で、`tree` コマンドと高い互換性を持ち、
AI・コーディングエージェントがプロジェクト構造を把握するための解析機能
（トークン数・git 情報・TODO・アウトライン・import 依存グラフ等）を備えています。

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

### ソースからビルド

```bash
git clone https://github.com/igarinpiano/dirlens
cd dirlens/rust && cargo build --release
# バイナリ: rust/target/release/dirlens
```

> **pip 版について**: `pip install dirlens` は旧 Python 実装（v1.x）を配布しています。
> 新機能・精度改善は Rust 版のみです。

---

## 出力例

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

  合計  5 ディレクトリ,  5 ファイル
  .py ×2  .txt ×1  .zip ×1  .png ×1
```

---

## 特徴

- **単一バイナリ** — Python も Node も不要。ダウンロードして置くだけで動く（macOS / Linux / Windows）
- **tree コマンドとの高い互換性** — **`-a -d -f -g -l -p -u -r -s -t -c -L -D -P -I -n -J --prune` など主要フラグが `tree` と互換**。dirlens 独自機能は `-G`（gitignore）・`-S`（サイズ順）・`-e`（拡張子）・`-C`（クリップボード）で提供
- **カラー表示** — ディレクトリ・ファイル・シンボリックリンクを色で識別
- **自動サイズ変換** — bytes / KB / MB / GB / TB
- **ディレクトリサイズ** — サブディレクトリの合計サイズを自動計算（並列プリフェッチで高速化）
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
- **JSON出力** — `--json` / `-J` で機械可読な構造データを出力（**`schema_version` 付きの安定スキーマ**）
- **HTMLレポート** — `--html` でブラウザで閲覧できる折りたたみツリーを生成
- **クリップボードコピー** — `-C` で出力を自動コピー（ANSIコードを自動除去）
- **AIチャット貼り付けモード** — `--ai` 一発で gitignore除外・日時・Markdown・クリップボードコピーを全適用（人間がコピペする用）
- **エージェント解析モード** — `--agent` 一発で下記のAI/エージェント向け解析機能を全適用（カラーなし・クリップボードは使わない、自律実行しても安全）
- **能力レポート** — `--check` で「この環境でどの解析方式が使えるか」を表示（縮退があると終了コード 1）
- **隠しファイル対応** — `-a` で表示切り替え
- **サイズ順ソート** — `-S` で大きいものから表示

### AI/エージェント向け解析機能（`--agent` でまとめて有効化）

AIチャットやコーディングエージェントがプロジェクト構造を理解する際に、ファイルの中身を
逐一読まなくても済むよう設計された機能です。個別フラグでも使えます。
**解析は「最良の方式 → 縮退」の多層構成**で、実際に使われた方式は `--check` や
`--agent --json` の `capabilities` / `analysis` ブロックで機械的に確認できます。

- **トークン数計数** — `-T` でファイルごとのトークン数を表示。**BPE（o200k_base）による正確値**（5MB 超のファイルは比例概算）。縮退時は文字数ベースの概算
- **git連携** — `-H` で各ファイルの最終コミット情報（メッセージ・相対日時）を表示。直近2000コミットまで走査。変更頻度の高いファイル（ホットスポット）も検出
- **TODO/FIXME抽出** — `-K` で `TODO`/`FIXME`/`HACK`/`XXX` コメントを抽出し、行番号付きで一覧表示
- **テスト欠落検知** — `-V` で対応するテストファイルが見つからないソースファイルをマーク（命名規則ベースのヒューリスティック）
- **エントリーポイント検出** — `-N` で `main.py`・`index.js`・`package.json` の `main`/`bin` フィールドなどから入口ファイルを推測してマーク
- **シンボルアウトライン** — `-O` で関数・クラス名を抽出。**言語別 AST パーサ（Python / JS・TS / Rust / Go / C）による正確な抽出**、パース失敗時は正規表現に縮退。`-A` で公開 API のみに絞り込み
- **import/依存グラフ** — `-M` でファイル間のローカルな import 関係を解析し、`imports×N`（依存先数）・`used-by×N`（被参照数）・循環依存を表示。**tsconfig の `paths`・package.json の `imports`・go.mod・Rust のモジュールツリー（`crate::`/`self::`/`super::`）を読んで解決**
- **設定ファイル検出** — `-F` で `.env`・`tsconfig.json`・`Makefile` 等の設定ファイルを検出して一覧表示

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
dirlens -A                # 公開APIのみのアウトライン
dirlens -M                # ローカルなimport/依存関係を解析
dirlens -F                # 設定ファイル（.env, tsconfig.json等）を検出して一覧表示

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
| **`--ai`**          | —            | **`-G --date -m -C` のショートカット。人間がAIチャットに貼り付ける用** |
| **`--agent`**       | —            | **`-G --date -T -H -K -V -N -O -M -F --no-color` のショートカット。エージェント向け解析（カラーなし・クリップボードは使わない）** |
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
| `--html [FILE]`     | —            | HTML レポートを生成（デフォルト: `dirlens.html`）              |
| `--copy`            | `-C`         | 出力をクリップボードにコピー（ANSIコードを自動除去）            |
| `--no-color`        | `-n`         | カラー表示を無効化                                            |
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
| トークン数（`-T`） | BPE（o200k_base）による正確値。データはバイナリに同梱 | 文字数ベースの概算 | 5MB 超のファイルは第1層でも比例概算。モデルによりトークナイザは異なるため、他社モデルでは目安 |
| アウトライン（`-O`/`-A`） | 言語別 AST パーサ（Python=rustpython / JS・TS=oxc / Rust=syn / Go・C=tree-sitter）。文字列内の偽検出なし | 正規表現による簡易抽出 | 構文エラーのあるファイルは自動で縮退層へ |
| import グラフ（`-M`） | AST 抽出 + マニフェスト解決（tsconfig `paths`/`baseUrl`・package.json `imports`・go.mod・Rust モジュールツリー） | 正規表現 + 相対パス解決 | node_modules 等の外部パッケージ実体は対象外（external 扱い） |
| テスト欠落検知（`-V`） | ファイル命名規則（`test_foo.py` 等）による判定のみ | — | 実際のカバレッジは見ていない。Rust は対象外（インラインテスト慣習のため） |
| エントリーポイント（`-N`） | 既知のファイル名 + `package.json` の `main`/`bin` | — | |
| TODO 抽出（`-K`） | 単語境界つき文字列マッチ | — | コメント外の文字列内の該当語も拾われることがある |
| git 連携（`-H`） | 直近 2000 コミットを走査 | — | それより古い変更しかないファイルは情報が出ない |

### 環境変数（縮退の手動制御）

| 変数 | 効果 |
|---|---|
| `DIRLENS_GITIGNORE=builtin` | gitignore を内蔵マッチャに固定 |
| `DIRLENS_AST=off` | AST 解析を無効化し正規表現層に固定 |
| `DIRLENS_TOKENS=heuristic` | トークン計数を文字数概算に固定 |
| `DIRLENS_COMPAT=python` | 上記すべて＋精度注記/`schema_version` 抑止（旧 Python 版とバイト一致になる検証用モード） |

---

## 仕様・注意事項

- ディレクトリのサイズは **全サブファイルの合計**（隠しファイルを含む、`.gitignore` 対象も含む）
- ディレクトリサイズはルート直下を **並列プリフェッチ** して高速化（透過的な最適化）
- **`+` 表記** — 一部のサブディレクトリが読めなかった場合、サイズは `1.5+ KB`（少なくとも 1.5 KB）、件数は `3+ dirs` のように表示
- **アクセス拒否** — 読めないディレクトリは赤太字で `[アクセス拒否]` を表示してスキップ
- **シンボリックリンク** は `→ リンク先パス` で表示。`-l` でリンク先ディレクトリを展開（循環検出あり）
- **ホームフォルダ（`~/`）やルート（`/`）で実行すると時間がかかる場合があります** — サイズ計算は表示深さに関わらず底まで全再帰するため
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
