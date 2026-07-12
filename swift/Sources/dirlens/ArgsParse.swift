// CLI 引数パーサ（rust/crates/dirlens-cli/src/main.rs の clap 定義の等価実装）。
// 短フラグの結合（-GT 等）・`--opt=value`・`--html` の省略可能値・`--` 対応。

import DirlensCore

let dirlensVersion = "1.1.2"

struct ParsedCli {
    var args = Args()
    // -L は --depth より優先（clap 版と同じマージ規則）
    var levelOpt: Int64? = nil
    var depthOpt: Int64? = nil
    var includeTree: [String] = []
    var excludeTree: [String] = []
    var excludeLong: [String] = []
    var includeLong: [String] = []
    var dateTree = false
    var noColorTree = false
    var jsonTree = false
    var positional: [String] = []
}

enum CliAction {
    case run(Args)
    case help
    case version
    case error(String)
}

private let boolShorts: [Character: WritableKeyPath<Args, Bool>] = [
    "d": \.dirsOnly, "g": \.showGroup, "t": \.sortMtime, "c": \.sortCtime,
    "G": \.gitignore, "S": \.sortSize, "C": \.copy,
    "a": \.all, "f": \.fullPath, "l": \.follow, "p": \.perms, "u": \.user, "r": \.reverse,
    "T": \.tokens, "H": \.git, "K": \.todo, "V": \.tests, "N": \.entry,
    "O": \.outline, "M": \.imports, "A": \.api, "F": \.config,
    "m": \.markdown,
]

private let boolLongs: [String: WritableKeyPath<Args, Bool>] = [
    "gitignore": \.gitignore, "sort-size": \.sortSize, "copy": \.copy,
    "all": \.all, "full-path": \.fullPath, "follow": \.follow, "perms": \.perms,
    "user": \.user, "reverse": \.reverse,
    "tokens": \.tokens, "git": \.git, "todo": \.todo, "missing-tests": \.tests,
    "entry": \.entry, "outline": \.outline, "imports": \.imports, "api": \.api,
    "config": \.config,
    "date": \.date, "markdown": \.markdown, "no-color": \.noColor, "bar": \.bar,
    "emoji": \.emoji, "json": \.json, "prune": \.prune, "filesfirst": \.filesfirst,
    "ai": \.ai, "agent": \.agent, "check": \.check,
]

func parseCli(_ argv: [String]) -> CliAction {
    var p = ParsedCli()
    var i = 0
    var noMoreFlags = false

    func nextValue(_ name: String) -> String? {
        if i + 1 < argv.count {
            i += 1
            return argv[i]
        }
        return nil
    }

    while i < argv.count {
        let arg = argv[i]
        if noMoreFlags || !arg.hasPrefix("-") || arg == "-" {
            p.positional.append(arg)
            i += 1
            continue
        }
        if arg == "--" {
            noMoreFlags = true
            i += 1
            continue
        }
        if arg.hasPrefix("--") {
            var name = String(arg.dropFirst(2))
            var attached: String? = nil
            if let eq = name.firstIndex(of: "=") {
                attached = String(name[name.index(after: eq)...])
                name = String(name[name.startIndex..<eq])
            }
            switch name {
            case "help":
                return .help
            case "version":
                return .version
            case _ where boolLongs[name] != nil:
                p.args[keyPath: boolLongs[name]!] = true
            case "type":
                guard let v = attached ?? nextValue(name) else {
                    return .error("--type には値が必要です")
                }
                p.args.typeExt = v
            case "depth":
                guard let v = attached ?? nextValue(name), let n = Int64(v) else {
                    return .error("--depth には整数値が必要です")
                }
                p.depthOpt = n
            case "min-size":
                guard let v = attached ?? nextValue(name) else {
                    return .error("--min-size には値が必要です")
                }
                p.args.minSize = v
            case "max-size":
                guard let v = attached ?? nextValue(name) else {
                    return .error("--max-size には値が必要です")
                }
                p.args.maxSize = v
            case "exclude":
                guard let v = attached ?? nextValue(name) else {
                    return .error("--exclude には値が必要です")
                }
                p.excludeLong.append(v)
            case "include":
                guard let v = attached ?? nextValue(name) else {
                    return .error("--include には値が必要です")
                }
                p.includeLong.append(v)
            case "html":
                if let v = attached {
                    p.args.html = v
                } else if i + 1 < argv.count, !argv[i + 1].hasPrefix("-") {
                    i += 1
                    p.args.html = argv[i]
                } else {
                    p.args.html = "dirlens.html" // 省略時デフォルト
                }
            default:
                return .error("不明なオプション: --\(name)")
            }
            i += 1
            continue
        }
        // 短フラグ（結合可）
        let cluster = Array(arg.dropFirst())
        var ci = 0
        while ci < cluster.count {
            let ch = cluster[ci]
            switch ch {
            case "h":
                return .help
            case "s":
                break // サイズ表示（常時有効・tree -s 互換の no-op）
            case "n":
                p.noColorTree = true
            case "J":
                p.jsonTree = true
            case "D":
                p.dateTree = true
            case _ where boolShorts[ch] != nil:
                p.args[keyPath: boolShorts[ch]!] = true
            case "e", "L", "P", "I":
                // 値を取る短オプション: 残り文字列 or 次の引数
                var value: String
                if ci + 1 < cluster.count {
                    value = String(cluster[(ci + 1)...])
                    ci = cluster.count
                } else if i + 1 < argv.count {
                    i += 1
                    value = argv[i]
                } else {
                    return .error("-\(ch) には値が必要です")
                }
                switch ch {
                case "e":
                    p.args.typeExt = value
                case "L":
                    guard let n = Int64(value) else {
                        return .error("-L には整数値が必要です")
                    }
                    p.levelOpt = n
                case "P":
                    p.includeTree.append(value)
                case "I":
                    p.excludeTree.append(value)
                default:
                    break
                }
            default:
                return .error("不明なオプション: -\(ch)")
            }
            ci += 1
        }
        i += 1
    }

    if p.positional.count > 1 {
        return .error("引数が多すぎます: \(p.positional.dropFirst().joined(separator: " "))")
    }
    var args = p.args
    args.path = p.positional.first ?? "."

    // ── エイリアスのマージ（argparse / clap 相当） ──────────────
    args.depth = p.depthOpt
    if let level = p.levelOpt {
        args.depth = level // -L は --depth より優先
    }
    if p.dateTree {
        args.date = true
    }
    args.exclude = p.excludeLong + p.excludeTree
    args.include = p.includeLong + p.includeTree
    if p.noColorTree {
        args.noColor = true
    }
    if p.jsonTree {
        args.json = true
    }
    args.mergeAliases()
    return .run(args)
}

let helpText = """
ファイルサイズ付きのディレクトリツリーを表示します

Usage: dirlens [OPTIONS] [path]

Arguments:
  [path]  対象ディレクトリ（省略時はカレント） [default: .]

Options（tree 互換）:
  -d                ディレクトリのみ表示（tree -d 互換）
  -g                グループ名を表示（tree -g 互換）
  -s                サイズ表示（常時有効・tree -s 互換）
  -t                更新日時順にソート（tree -t 互換）
  -c                ステータス変更日時順にソート（tree -c 互換）
  -a, --all         隠しファイルも表示
  -f, --full-path   ルートからのフルパスで表示
  -l, --follow      シンボリックリンク先ディレクトリを展開
  -p, --perms       パーミッション文字列を表示
  -u, --user        所有者のユーザー名を表示
  -r, --reverse     ソート順を逆にする
  -n                カラーなし（tree -n 互換）
  -J                JSON形式で出力（tree -J 互換）
  -L <N>            表示する最大の深さ（tree -L 互換）
  -D                最終更新日時を表示（tree -D 互換）
  -P <PATTERN>      このパターンのみ表示（tree -P 互換）
  -I <PATTERN>      除外パターン（tree -I 互換）

Options（dirlens 独自）:
  -G, --gitignore   .gitignoreのファイルを除外（旧 -g）
  -S, --sort-size   サイズ順にソート（旧 -s）
  -e, --type <EXT>  指定した拡張子のみ表示（旧 -t）
  -C, --copy        クリップボードにコピー（旧 -c）
      --depth <N>   表示する最大の深さ（-L と同じ）
      --date        最終更新日時を相対表示
  -m, --markdown    Markdown コードブロック形式で出力
      --no-color    カラー表示を無効化
      --bar         ディスク占有率バーを表示
      --min-size <SIZE>  指定サイズ以上のファイルのみ表示（例: 1M, 500K）
      --max-size <SIZE>  指定サイズ以下のファイルのみ表示
      --exclude <PATTERN>  除外パターン（複数指定可）
      --include <PATTERN>  このパターンのみ表示（複数指定可）
      --emoji       拡張子に応じた絵文字アイコンを表示
      --json        JSON形式で出力
      --html [FILE] HTMLレポートを生成（デフォルト: dirlens.html）
      --prune       フィルタ後に空になるディレクトリを非表示
      --filesfirst  ファイルをディレクトリより先に表示

Options（AI/エージェント向け解析）:
  -T, --tokens         ファイルごとの推定トークン数を表示（BPE または概算）
  -H, --git            最終コミット情報を表示（要git、直近2000コミットまで走査）
  -K, --todo           TODO/FIXME/HACK/XXXコメントを抽出
  -V, --missing-tests  対応するテストファイルが見つからないソースファイルを表示
  -N, --entry          エントリーポイントらしきファイルを検出してマーク
  -O, --outline        関数・クラスの簡易アウトラインを表示（対応言語限定）
  -M, --imports        ローカルなimport/依存関係を解析して表示（外部パッケージは対象外）。循環依存も併せて検出
  -A, --api            公開API（exportされたシンボル）のみに絞り込む（-O を自動的に有効化）
  -F, --config         設定ファイル（.env, tsconfig.json等）を検出してマーク

Options（ショートカット・その他）:
      --ai       -G --date -m -C のショートカット（人間がAIチャットに貼り付ける用）
      --agent    -G -T -H -K -V -N -O -M -F --no-color のショートカット（エージェント向け解析、カラーなし・クリップボードは使わない）
      --check    能力レポートを表示（gitignore層・言語別解析方式・外部ツール可否）。縮退があると終了コード 1。--json 併用可
      --version  バージョンを表示
  -h, --help     ヘルプを表示

使用例:
  dirlens --ai             AIチャット貼り付け用（人間がコピペする想定）
  dirlens --agent          エージェント向け解析（カラーなし・クリップボードは使わない）
  dirlens -d               ディレクトリのみ表示（tree -d 互換）
  dirlens -L 2             深さ 2 まで表示（tree -L 互換）
  dirlens -G --prune       gitignore除外 + 空枝を剪定
  dirlens -T               ファイルごとの推定トークン数を表示
  dirlens -H               最終コミット情報を表示（要git）
  dirlens -K               TODO/FIXME/HACKを抽出
  dirlens -V               テストが無いソースファイルを表示
  dirlens -N               エントリーポイントらしきファイルをマーク
  dirlens -O               関数・クラスの簡易アウトラインを表示
  dirlens -M               ローカルなimport/依存関係を解析
  dirlens --no-color > dirlens.txt   ファイルに書き出す
"""
