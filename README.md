# dirlens 🌳

ファイルサイズ付きのディレクトリツリーを表示するコマンドラインツール。  
**Python 3.8+ のみで動作**（追加ライブラリ不要）。

---

## インストール

### npm（推奨・全プラットフォーム共通）

Node.js が入っていれば、どの OS でも同じコマンドでインストールできます。

```bash
npm install -g dirlens
```

インストール確認：

```bash
dirlens --help
```

アンインストール：

```bash
npm uninstall -g dirlens
```

---

### macOS / Linux（スクリプト直接インストール）

GitHubリポジトリから `dirlens.py` をダウンロードして使用します。

```bash
# /usr/local/bin にインストール（どこからでも呼べるようになる）
sudo install -m 755 dirlens.py /usr/local/bin/dirlens

# ── または sudo なしでユーザーローカルにインストール ──
mkdir -p ~/.local/bin
cp dirlens.py ~/.local/bin/dirlens
chmod +x ~/.local/bin/dirlens

# ~/.zshrc（zsh）または ~/.bashrc（bash）に以下を追記：
export PATH="$HOME/.local/bin:$PATH"
# 追記後に反映：
source ~/.zshrc   # または source ~/.bashrc
```

インストール確認：

```bash
dirlens --help
```

---

### Windows（スクリプト直接インストール）

1. `dirlens.py` と `dirlens.bat` を任意のフォルダへ置く  
   （例: `C:\Users\ユーザー名\bin\`）

2. 同じフォルダに **`dirlens.bat`** を置く（同梱のものを使用）:

   ```batch
   @echo off
   python "%~dp0dirlens.py" %*
   ```

3. そのフォルダを **システム環境変数 PATH** に追加：
   - スタートメニュー →「環境変数を編集」→ PATH に追記
   - または PowerShell（管理者）:

     ```powershell
     [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";C:\Users\ユーザー名\bin", "User")
     ```

4. 新しいターミナルを開いて確認：

   ```cmd
   dirlens --help
   ```

> **メモ**: Windows Terminal や VS Code のターミナルではカラー表示されます。  
> 旧来のコマンドプロンプト（cmd.exe）ではカラーが出ない場合があります。

---

## 出力例

```
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

- **クロスプラットフォーム** — macOS / Linux / Windows
- **カラー表示** — ディレクトリ・ファイル・シンボリックリンクを色で識別
- **自動サイズ変換** — bytes / KB / MB / GB / TB
- **ディレクトリサイズ** — サブディレクトリの合計サイズを自動計算
- **アイテム数表示** — 各ディレクトリの直下にある dirs / files 数を表示
- **拡張子統計** — ツリー全体のファイル種別を集計してサマリーに表示
- **`.gitignore` 対応** — `-g` で `node_modules/` などを自動除外（サブディレクトリも対応）
- **最終更新日時** — `--date` で各ファイル・ディレクトリの更新日時を相対表示
- **拡張子フィルタ** — `-t py` など指定した拡張子のみ表示
- **パターンフィルタ** — `--exclude` / `--include` でワイルドカード指定（複数可）
- **サイズフィルタ** — `--min-size` / `--max-size` で容量による絞り込み
- **ディスク占有率バー** — `--bar` で親ディレクトリに対する占有率を視覚表示
- **絵文字アイコン** — `--emoji` で拡張子に応じた絵文字を付与
- **Markdown出力** — `-m` でコードブロック形式に出力、AIチャットへそのままペースト可
- **JSON出力** — `--json` で機械可読な構造データを出力、スクリプト連携に
- **HTMLレポート** — `--html` でブラウザで閲覧できる折りたたみツリーを生成
- **クリップボードコピー** — `-c` で出力を自動コピー、AIチャットへの貼り付けが一発
- **隠しファイル対応** — `-a` で表示切り替え（アイテム数・統計にも反映）
- **サイズ順ソート** — `-s` で大きいものから表示

---

## 使い方

```bash
# カレントディレクトリを表示
dirlens

# 特定のディレクトリを表示
dirlens ~/Desktop

# 深さ 2 階層まで表示
dirlens -d 2

# 隠しファイル・ディレクトリ (.xxx) も表示
dirlens -a

# サイズの大きい順に並べる
dirlens -s

# .gitignore のファイルを除外（node_modules など）
dirlens -g

# 最終更新日時を相対表示（例: 3日前、2時間前）
dirlens --date

# ディスク占有率バーを表示
dirlens --bar

# 拡張子に応じた絵文字アイコンを表示
dirlens --emoji

# 指定した拡張子のファイルのみ表示
dirlens -t py

# パターンで除外（複数指定可）
dirlens --exclude '*.log' --exclude 'dist'

# パターンで抽出（複数指定可）
dirlens --include 'test_*'

# サイズで絞り込み
dirlens --min-size 1M        # 1MB 以上のみ
dirlens --max-size 100K      # 100KB 以下のみ

# Markdown コードブロック形式で出力
dirlens -m

# JSON 形式で出力（スクリプト連携）
dirlens --json

# HTML レポートを生成（デフォルト: dirlens.html）
dirlens --html
dirlens --html report.html   # ファイル名を指定

# 出力をクリップボードにコピー
dirlens -c

# カラーなし（パイプ・ファイル書き出し向け）
dirlens --no-color

# ── AI チャットへの貼り付け（推奨の組み合わせ）────────────────
# gitignore 除外 → Markdown → クリップボードコピー（そのまま貼れる）
dirlens -g -m -c

# テキストファイルに書き出す
dirlens --no-color > dirlens.txt
```

---

## オプション一覧

| オプション           | 省略形   | 説明                                                       |
|---------------------|----------|------------------------------------------------------------|
| `path`              | —        | 対象ディレクトリ（省略時はカレント）                          |
| `--depth N`         | `-d N`   | 表示する最大の深さ                                           |
| `--all`             | `-a`     | 隠しファイル・ディレクトリも表示                              |
| `--sort-size`       | `-s`     | サイズが大きい順に並べる                                      |
| `--gitignore`       | `-g`     | `.gitignore` に記載されたファイルを除外（サブディレクトリも対応）|
| `--date`            | —        | 最終更新日時を相対表示（例: 3日前）                           |
| `--type EXT`        | `-t EXT` | 指定した拡張子のファイルのみ表示（例: `-t py`）               |
| `--bar`             | —        | 親ディレクトリに対するディスク占有率バーを表示                  |
| `--emoji`           | —        | 拡張子に応じた絵文字アイコンを表示                            |
| `--exclude PATTERN` | —        | 除外パターン（複数指定可、例: `--exclude '*.log'`）           |
| `--include PATTERN` | —        | このパターンのみ表示（複数指定可）                             |
| `--min-size SIZE`   | —        | 指定サイズ以上のファイルのみ表示（例: `1M`, `500K`）          |
| `--max-size SIZE`   | —        | 指定サイズ以下のファイルのみ表示（例: `10M`）                 |
| `--markdown`        | `-m`     | Markdown コードブロック形式で出力（カラー自動無効）            |
| `--json`            | —        | JSON 形式で標準出力に出力                                    |
| `--html [FILE]`     | —        | HTML レポートを生成（デフォルト: `dirlens.html`）             |
| `--copy`            | `-c`     | 出力をクリップボードにコピー                                  |
| `--no-color`        | —        | カラー表示を無効化（リダイレクト時に推奨）                     |

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

## 仕様・注意事項

- ディレクトリのサイズは **全サブファイルの合計**（隠しファイルを含む、`.gitignore` 対象も含む）
- **シンボリックリンク先のディレクトリ**は展開せず `→` マークで表示
- 権限がないディレクトリは `[アクセス拒否]` と表示してスキップ
- 非常に深いディレクトリ（1万階層以上）は `-d` で深さを制限してください
- **ホームフォルダ（`~/`）やルート（`/`）で実行すると固まる場合があります** — サイズ計算は `-d` の表示制限に関わらず底まで全再帰するため、`~/Library` や iCloud Drive など大容量・ネットワークマウントのディレクトリで時間がかかります。プロジェクトフォルダなど範囲を絞って実行してください
- **`-g` の否定パターン（`!` から始まる行）は現時点で非対応**です
