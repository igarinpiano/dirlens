"""ゴールデンテストのケース定義。

各ケース: {
  "id":        一意なケース名（snapshots/<fixture>/<id>.out 等になる）,
  "fixture":   fixtures.py の BUILDERS キー,
  "args":      dirlens に渡す引数（対象パスの後ろに付く）,
  "live_only": True なら snapshot 記録・照合をせず live モード（Python vs Rust 直接比較）のみ,
  "requires":  実行環境の要件リスト（"unixperm": 非rootのPOSIX / "git": git 必須）,
}
"""


def _c(id, fixture, args, live_only=False, requires=(), rust_only=False):
    """rust_only: dirlens.py に存在しない機能（--check 等）。live 比較の対象外で、
    スナップショットは record --bin（Rust 版）でのみ記録される。"""
    return {"id": id, "fixture": fixture, "args": list(args),
            "live_only": live_only, "requires": list(requires),
            "rust_only": rust_only}


CASES = [
    # ── basic: tree 互換 ─────────────────────────────────────
    _c("base",           "basic", []),
    _c("all",            "basic", ["-a"]),
    _c("dirs_only",      "basic", ["-d"]),
    _c("L1",             "basic", ["-L", "1"]),
    _c("L2",             "basic", ["-L", "2"]),
    _c("depth2",         "basic", ["--depth", "2"]),
    _c("L_over_depth",   "basic", ["--depth", "3", "-L", "1"]),
    _c("full_path",      "basic", ["-f"]),
    _c("reverse",        "basic", ["-r"]),
    _c("mtime_sort",     "basic", ["-t"]),
    _c("mtime_sort_rev", "basic", ["-t", "-r"]),
    _c("ctime_sort",     "basic", ["-c"], live_only=True),
    _c("size_sort",      "basic", ["-S"]),
    _c("size_sort_rev",  "basic", ["-S", "-r"]),
    _c("files_first",    "basic", ["--filesfirst"]),
    _c("type_py",        "basic", ["-e", "py"]),
    _c("type_dot_md",    "basic", ["-e", ".md"]),
    _c("include_md",     "basic", ["-P", "*.md"]),
    _c("include_multi",  "basic", ["-P", "*.md", "--include", "*.txt"]),
    _c("exclude_log",    "basic", ["-I", "*.log"]),
    _c("exclude_dir",    "basic", ["--exclude", "sub", "--exclude", "*.bin"]),
    _c("min_size",       "basic", ["--min-size", "1K"]),
    _c("max_size",       "basic", ["--max-size", "2000"]),
    _c("prune_py",       "basic", ["-e", "py", "--prune"]),
    _c("bar",            "basic", ["--bar"]),
    _c("emoji",          "basic", ["--emoji"]),
    _c("markdown",       "basic", ["-m"]),
    _c("date",           "basic", ["--date"]),
    _c("date_tree",      "basic", ["-D"]),
    _c("no_color",       "basic", ["-n"]),
    _c("size_flag_noop", "basic", ["-s"]),
    _c("json",           "basic", ["--json"]),
    _c("json_J",         "basic", ["-J"]),
    _c("json_L2",        "basic", ["--json", "-L", "2"]),
    _c("json_all",       "basic", ["--json", "-a", "-D"]),
    _c("html",           "basic", ["--html"]),
    _c("html_named",     "basic", ["--html", "out.html"]),
    _c("follow",         "basic", ["-l"]),
    _c("follow_L3",      "basic", ["-l", "-L", "3"]),
    _c("combo_display",  "basic", ["-a", "-S", "-D", "--bar"]),
    _c("perms",          "basic", ["-p"]),
    _c("user_group",     "basic", ["-p", "-u", "-g"], live_only=True),
    _c("copy",           "basic", ["-C"]),

    # ── multi_lang: AI/エージェント解析 ──────────────────────
    _c("ml_base",        "multi_lang", []),
    _c("ml_tokens",      "multi_lang", ["-T"]),
    _c("ml_todo",        "multi_lang", ["-K"]),
    _c("ml_tests",       "multi_lang", ["-V"]),
    _c("ml_entry",       "multi_lang", ["-N"]),
    _c("ml_outline",     "multi_lang", ["-O"]),
    _c("ml_api",         "multi_lang", ["-A"]),
    _c("ml_imports",     "multi_lang", ["-M"]),
    _c("ml_config",      "multi_lang", ["-F"]),
    _c("ml_combo_tk",    "multi_lang", ["-T", "-K"]),
    _c("ml_combo_nm",    "multi_lang", ["-N", "-M"]),
    _c("ml_agent",       "multi_lang", ["--agent"], requires=["git"]),
    _c("ml_agent_L2",    "multi_lang", ["--agent", "-L", "2"], requires=["git"]),
    _c("ml_agent_json",  "multi_lang", ["--agent", "--json"], requires=["git"]),
    _c("ml_json_T",      "multi_lang", ["--json", "-T"]),
    _c("ml_json_K",      "multi_lang", ["--json", "-K"]),
    _c("ml_json_V",      "multi_lang", ["--json", "-V"]),
    _c("ml_json_N",      "multi_lang", ["--json", "-N"]),
    _c("ml_json_O",      "multi_lang", ["--json", "-O"]),
    _c("ml_json_A",      "multi_lang", ["--json", "-A"]),
    _c("ml_json_M",      "multi_lang", ["--json", "-M"]),
    _c("ml_json_F",      "multi_lang", ["--json", "-F"]),
    _c("ml_ai",          "multi_lang", ["--ai"]),
    _c("ml_html_agentish", "multi_lang", ["--html", "-K", "-V", "-N"]),

    # ── rust_lang ────────────────────────────────────────────
    _c("rs_outline",     "rust_lang", ["-O"]),
    _c("rs_api",         "rust_lang", ["-A"]),
    _c("rs_imports",     "rust_lang", ["-M"]),
    _c("rs_tests",       "rust_lang", ["-V"]),
    _c("rs_agent",       "rust_lang", ["--agent"], requires=["git"]),
    _c("rs_agent_json",  "rust_lang", ["--agent", "--json"], requires=["git"]),

    # ── gitignored: -G / -H ──────────────────────────────────
    _c("gi_noG",         "gitignored", [], requires=["git"]),
    _c("gi_G",           "gitignored", ["-G"], requires=["git"]),
    _c("gi_G_all",       "gitignored", ["-G", "-a"], requires=["git"]),
    _c("gi_G_prune",     "gitignored", ["-G", "--prune"], requires=["git"]),
    _c("gi_G_json",      "gitignored", ["-G", "--json"], requires=["git"]),
    _c("gi_H",           "gitignored", ["-H"], requires=["git"]),
    _c("gi_H_json",      "gitignored", ["-H", "--json"], requires=["git"]),
    _c("gi_agent",       "gitignored", ["--agent"], requires=["git"]),
    _c("gi_agent_json",  "gitignored", ["--agent", "--json"], requires=["git"]),

    # ── --check（Rust 版のみの新機能） ────────────────────────
    _c("check",         "basic", ["--check"], requires=["git"], rust_only=True),
    _c("check_json",    "basic", ["--check", "--json"], requires=["git"], rust_only=True),
    _c("gi_check",      "gitignored", ["--check"], requires=["git"], rust_only=True),

    # ── edge: エッジケース ───────────────────────────────────
    _c("edge_base",      "edge", [], requires=["unixperm"]),
    _c("edge_tokens",    "edge", ["-T"], requires=["unixperm"]),
    _c("edge_json_T",    "edge", ["-T", "--json"], requires=["unixperm"]),
    _c("edge_todo",      "edge", ["-K"], requires=["unixperm"]),
    _c("edge_L2",        "edge", ["-L", "2"], requires=["unixperm"]),
    _c("edge_html",      "edge", ["--html"], requires=["unixperm"]),
    _c("edge_agent",     "edge", ["--agent"], requires=["unixperm", "git"]),
]
