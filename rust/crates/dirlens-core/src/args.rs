//! 引数（argparse 相当のエイリアス統合「後」の値）。
//!
//! CLI（clap）と wasm バインディングの両方がこの構造体を組み立ててコアへ渡す。
//! エイリアス統合（-L→depth, -D→date, -P→include, -I→exclude, -n→no_color,
//! -J→json, --ai/--agent のフラグ束展開, -A→-O）は `merge_aliases` が行う。

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub path: String,
    pub depth: Option<i64>,

    // tree 互換
    pub dirs_only: bool,      // -d
    pub show_group: bool,     // -g
    pub sort_mtime: bool,     // -t
    pub sort_ctime: bool,     // -c
    pub all: bool,            // -a
    pub full_path: bool,      // -f
    pub follow: bool,         // -l
    pub perms: bool,          // -p
    pub user: bool,           // -u
    pub reverse: bool,        // -r

    // dirlens 独自
    pub gitignore: bool,      // -G
    pub sort_size: bool,      // -S
    pub type_ext: Option<String>, // -e
    pub copy: bool,           // -C
    pub date: bool,           // --date / -D
    pub markdown: bool,       // -m
    pub no_color: bool,       // --no-color / -n
    pub bar: bool,
    pub min_size: Option<String>,
    pub max_size: Option<String>,
    pub exclude: Vec<String>, // --exclude + -I（この順で連結）
    pub include: Vec<String>, // --include + -P（この順で連結）
    pub emoji: bool,
    pub json: bool,           // --json / -J
    pub html: Option<String>, // --html [FILE]
    pub prune: bool,
    pub filesfirst: bool,
    pub ai: bool,
    pub agent: bool,
    pub check: bool,   // --check（能力レポート）
    /// 出力言語（"en" / "ja"）。None なら英語（デフォルト）。
    /// CLI 側で --lang / 設定ファイル / DIRLENS_LANG を解決して入れる。
    pub lang: Option<String>,

    // AI/エージェント解析
    pub tokens: bool,   // -T
    pub git: bool,      // -H
    pub todo: bool,     // -K
    pub tests: bool,    // -V
    pub entry: bool,    // -N
    pub outline: bool,  // -O
    pub imports: bool,  // -M
    pub api: bool,      // -A
    pub config: bool,   // -F

    // 表示モード・注釈（v1.2 拡張）
    pub top: Option<usize>,         // --top N（大きいファイル/ディレクトリの一覧）
    pub dupes: bool,                // --dupes（重複ファイル検出）
    pub compare: Option<String>,    // --compare DIR（ディレクトリ比較）
    pub status: bool,               // --status（git status オーバーレイ）
    pub heat: Option<String>,       // --heat age|size|churn
    pub since: Option<String>,      // --since REF（変更ファイルのみ表示）
    pub focus: Option<String>,      // --focus FILE（影響範囲クエリ・-M を暗黙有効化）
    pub stdin_files: Option<Vec<String>>, // --stdin（CLI が読み取ったファイルリスト）
    pub budget: Option<i64>,        // --budget N（出力トークン予算）
    pub estimate: bool,             // --estimate（階層別の出力コスト見積もり）
    pub api_diff: Option<String>,   // --api-diff REF（公開APIの差分）
    pub pack: Vec<String>,          // --pack FILE...（貼り付け用ブロック整形）
    pub mermaid: bool,              // --mermaid（import グラフを Mermaid で出力）
    pub dot: bool,                  // --dot（import グラフを Graphviz DOT で出力）
    pub csv: bool,                  // --csv（ファイルメタデータを CSV で出力）
}

impl Args {
    /// --ai / --agent / -A のフラグ束展開（dirlens.py main() のエイリアス統合と同一）。
    /// -L/-D/-P/-I/-n/-J の統合は CLI 側でフィールドに直接反映済みであること。
    pub fn merge_aliases(&mut self) {
        if self.ai {
            self.gitignore = true;
            self.date = true;
            self.markdown = true;
            self.copy = true;
            // 作業中のファイルが分かるよう git status マークも重ねる
            // （compat モードでは CLI 側で無効化される）
            self.status = true;
        }
        if self.agent {
            self.gitignore = true;
            self.date = true;
            self.tokens = true;
            self.git = true;
            self.todo = true;
            self.tests = true;
            self.entry = true;
            self.outline = true;
            self.imports = true;
            self.config = true;
            self.status = true;
            self.no_color = true;
        }
        if self.api {
            self.outline = true;
        }
        // --focus / --mermaid / --dot は import グラフが前提
        if self.focus.is_some() || self.mermaid || self.dot {
            self.imports = true;
        }
        // --stdin はファイル単位の解析が本体（トークン・アウトライン・TODO）
        if self.stdin_files.is_some() {
            self.tokens = true;
            self.outline = true;
            self.todo = true;
        }
    }
}
