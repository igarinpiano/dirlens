# dirlens 🌳

ファイルサイズ付きのディレクトリツリーを表示するコマンドラインツール。  
**Python 3.8+ のみで動作**（追加ライブラリ不要）。

---

## 出力例

```
Desktop/ (3.74 MB)
├── EmptyDir/ (0 bytes)
├── Project/ (712 KB)
│   ├── assets/ (512 KB)
│   │   └── images/ (512 KB)
│   │       └── logo.png (512 KB)
│   ├── src/ (80 KB)
│   │   └── util.py (80 KB)
│   └── main.py (120 KB)
├── archive.zip (3 MB)
└── readme.txt (50 KB)

  5 ディレクトリ,  5 ファイル
```

---

## 特徴

- **クロスプラットフォーム** — macOS / Linux / Windows
- **カラー表示** — ディレクトリ・ファイル・シンボリックリンクを色で識別
- **自動サイズ変換** — bytes / KB / MB / GB / TB
- **ディレクトリサイズ** — サブディレクトリの合計サイズを自動計算
- **隠しファイル対応** — `-a` で表示切り替え
- **サイズ順ソート** — `-s` で大きいものから表示

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

```bash
# 実行権限を付与
chmod +x dirlens

# /usr/local/bin にインストール（どこからでも呼べるようになる）
sudo cp dirlens /usr/local/bin/

# ── または sudo なしでユーザーローカルにインストール ──
mkdir -p ~/.local/bin
cp dirlens ~/.local/bin/

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

1. `dirlens` を **`dirlens.py`** に改名して任意のフォルダへ置く  
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

## 使い方

```bash
# カレントディレクトリを表示
dirlens

# 特定のディレクトリを表示
dirlens ~/Desktop

# 深さ 2 階層まで表示（大きなディレクトリに便利）
dirlens -d 2

# 隠しファイル・ディレクトリ (.xxx) も表示
dirlens -a

# サイズの大きい順に並べる
dirlens -s

# カラーなし（パイプ・ファイル書き出し向け）
dirlens --no-color

# 組み合わせ例：Desktop を深さ 3・サイズ順で表示
dirlens ~/Desktop -d 3 -s

# テキストファイルに書き出す
dirlens --no-color > tree.txt
```

---

## オプション一覧

| オプション        | 省略形 | 説明                                     |
|------------------|--------|------------------------------------------|
| `path`           | —      | 対象ディレクトリ（省略時はカレント）         |
| `--depth N`      | `-d N` | 表示する最大の深さ                         |
| `--all`          | `-a`   | 隠しファイル・ディレクトリも表示            |
| `--sort-size`    | `-s`   | サイズが大きい順に並べる                    |
| `--no-color`     | —      | カラー表示を無効化（リダイレクト時に推奨）   |

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

- ディレクトリのサイズは **全サブファイルの合計**（隠しファイルを含む）
- **シンボリックリンク先のディレクトリ**は展開せず `→` マークで表示
- 権限がないディレクトリは `[アクセス拒否]` と表示してスキップ
- 非常に深いディレクトリ（1万階層以上）は `-d` で深さを制限してください
