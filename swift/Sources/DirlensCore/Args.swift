// 引数（エイリアス統合「後」の値）。rust/crates/dirlens-core/src/args.rs の等価移植。
//
// CLI がこの構造体を組み立ててコアへ渡す。
// エイリアス統合（-L→depth, -D→date, -P→include, -I→exclude, -n→no_color,
// -J→json, --ai/--agent のフラグ束展開, -A→-O）は mergeAliases が行う。

public struct Args {
    public var path: String = "."
    public var depth: Int64? = nil

    // tree 互換
    public var dirsOnly = false      // -d
    public var showGroup = false     // -g
    public var sortMtime = false     // -t
    public var sortCtime = false     // -c
    public var all = false           // -a
    public var fullPath = false      // -f
    public var follow = false        // -l
    public var perms = false         // -p
    public var user = false          // -u
    public var reverse = false       // -r

    // dirlens 独自
    public var gitignore = false     // -G
    public var sortSize = false      // -S
    public var typeExt: String? = nil // -e
    public var copy = false          // -C
    public var date = false          // --date / -D
    public var markdown = false      // -m
    public var noColor = false       // --no-color / -n
    public var bar = false
    public var minSize: String? = nil
    public var maxSize: String? = nil
    public var exclude: [String] = [] // --exclude + -I（この順で連結）
    public var include: [String] = [] // --include + -P（この順で連結）
    public var emoji = false
    public var json = false          // --json / -J
    public var html: String? = nil   // --html [FILE]
    public var prune = false
    public var filesfirst = false
    public var ai = false
    public var agent = false
    public var check = false         // --check（能力レポート）

    // AI/エージェント解析
    public var tokens = false        // -T
    public var git = false           // -H
    public var todo = false          // -K
    public var tests = false         // -V
    public var entry = false         // -N
    public var outline = false       // -O
    public var imports = false       // -M
    public var api = false           // -A
    public var config = false        // -F

    public init() {}

    /// --ai / --agent / -A のフラグ束展開（dirlens.py main() のエイリアス統合と同一）。
    public mutating func mergeAliases() {
        if ai {
            gitignore = true
            date = true
            markdown = true
            copy = true
        }
        if agent {
            gitignore = true
            date = true
            tokens = true
            git = true
            todo = true
            tests = true
            entry = true
            outline = true
            imports = true
            config = true
            noColor = true
        }
        if api {
            outline = true
        }
    }
}
