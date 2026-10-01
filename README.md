# dirlens

**English** | [日本語](README_ja.md)

**`dirlens` is a project map that connects filesystem structure, lightweight code intelligence, and AI context.**

Starting with a familiar `tree`-style directory view, `dirlens` layers in sizes, modification times, Git information, token counts, TODOs, test hints, entry-point candidates, symbol outlines, local import relationships, and configuration files. It gives people, AI chats, and coding agents one readable map of a codebase.

```text
filesystem tree
      + project metadata / git
      + lightweight code structure
      + dependency / impact hints
      + token / context awareness
      └── one project map for humans, AI chats, and coding agents
```

`dirlens` can replace `tree`, but it is not merely a larger `tree`. Nor is it a substitute for deep semantic search, an LSP, a symbol graph, or a knowledge graph. It sits between a filesystem-only listing and deep code intelligence, making it clear—at a glance—what is where and what to read next.

It is a **single Rust binary with no runtime dependencies** — no Python, no Node, just download it and run it on macOS/Linux/Windows — with broad compatibility for major `tree` flags (`-a -d -f -g -l -p -u -r -s -t -c -L -D -P -I -n -J --prune`, among others). `dirlens`-specific behavior lives behind its own flags: `-G` (gitignore), `-S` (sort by size), `-e` (extension filter), `-C` (clipboard).

Output is English by default. Use `--lang ja`, set `lang = "ja"` in `~/.config/dirlens/config.toml`, or set `DIRLENS_LANG=ja` for Japanese output.

> The legacy Python implementation (v1.0.x) is available on the [`python`](https://github.com/igarinpiano/dirlens/tree/python) branch. Golden tests verify output compatibility with the Rust implementation.

---

## Install

### npm (recommended on every OS)

```bash
npm install -g dirlens
```

The package automatically selects a native binary for macOS arm64/x64; Linux arm64/x64, musl arm64/x64 (Alpine, etc.), ppc64, and s390x; and Windows x64/arm64. Windows runs a native `dirlens.exe`.

### Download a binary

Download the archive for your platform from [GitHub Releases](https://github.com/igarinpiano/dirlens/releases), then put the binary on your `PATH`. Release binaries are also provided, where available, for armv7 (32-bit Linux ARM), i686 (32-bit Linux/Windows), and riscv64gc. These targets are not published to npm: Node.js has no official builds for i686 Linux or riscv64, and dropped its official armv7 and 32-bit Windows builds after Node 22 LTS.

The glibc Linux binaries (`*-unknown-linux-gnu`) are linked against an old glibc and run on glibc 2.28 or newer (e.g. RHEL/Rocky 8+, Debian 10+, Ubuntu 20.04+). On older or glibc-less systems, use the `*-unknown-linux-musl` (statically linked) build.

```bash
# Example: macOS (Apple Silicon)
tar -xzf dirlens-*-aarch64-apple-darwin.tar.gz
sudo install -m 755 dirlens /usr/local/bin/
```

### crates.io

```bash
cargo install dirlens
```

This compiles the project from source.

### cargo-binstall

```bash
cargo binstall dirlens
```

This downloads a prebuilt GitHub Release binary through the crates.io metadata (`[package.metadata.binstall]`), without a local compilation — faster than `cargo install`.

### Build from source

```bash
git clone https://github.com/igarinpiano/dirlens.git
cd dirlens/rust
cargo build --release
# Binary: target/release/dirlens
```

> **About the PyPI package:** `pip install dirlens` still distributes the legacy Python implementation (v1.0.x). Current development happens on the Rust implementation described in this document.

---

## One project map, three ways to use it

`--ai`, `--agent`, and `--mcp` are not separate products — they are different entry points into the same underlying project map, shaped for whoever receives it.

| Mode | Intended user | What it does |
| --- | --- | --- |
| Normal CLI | People | Browse a project interactively or use it as a `tree` replacement. |
| `--ai` | People, pasting into an AI chat | Copies a dated Markdown snapshot (with `.gitignore` applied) to the clipboard — more structured context than a screenshot or a plain `tree`. |
| `--agent` | Coding agents (Claude Code, Codex, Cursor, ...) | Produces a color-free, clipboard-free project analysis — structure, scale, likely-important spots, dependencies — meant to be read before exploring, not a substitute for the agent itself. |
| `--mcp` | MCP-capable coding agents | Exposes the same analysis as on-demand tools (`analyze`, `outline`, `focus`, `since`, ...), so an agent can re-query just the piece it needs mid-session. |

In short: paste `--ai` into an ordinary AI chat, hand `--agent` to an agent as its first look at a project, and let MCP-aware agents query the map incrementally as they work.

## Quick start

```bash
# Use it like tree: inspect the structure first
dirlens -G -L 2

# For an AI chat: copy Markdown context to your clipboard
dirlens --ai -L 3

# For a coding agent: generate a project map before exploration
dirlens --agent

# Estimate output cost before analyzing an unfamiliar large repository
dirlens --agent --estimate
dirlens --agent --budget 3000

# Print MCP registration instructions
dirlens --mcp-setup
```

`--agent`'s report is a map for choosing what to read next, not a complete semantic model of the code. Confirm anything that matters by reading the actual file.

---

## Example output

### Basic tree

```bash
dirlens
```

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
  .py ×2  .png ×1  .zip ×1  .txt ×1
```

Each directory line shows its direct children (`dirs`/`files`) and its total size. Add `-D` for modification times, `-L N` to limit depth, or `--lang ja` for Japanese output.

### `--ai`: paste-ready AI context

```bash
dirlens --ai
```

`--ai` is shorthand for `-G --date -m -C --status`: it applies `.gitignore`, shows relative modification times, wraps the tree in a Markdown code block, and copies the result straight to the clipboard — ready to paste into a chat the moment the command finishes. It is for a human-operated workflow; do not use it from autonomous agents because clipboard access is a side effect.

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

### `--agent`: agent analysis mode

```bash
dirlens --agent
```

`--agent` disables ANSI color and enables the complete analysis set: token counts, Git metadata and status, TODO/FIXME markers, test hints, entry-point candidates (`*`), config-file detection (`⚙`), symbol outlines, import relationships, language token totals, and long-function reporting. It also respects `.gitignore` by default.

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

Use JSON when a program needs to consume the result:

```bash
dirlens --agent --json
```

The JSON output stays valid even when a partial analysis cannot run (it carries the same `schema_version`). Check `errors`, `capabilities`, and per-file fields such as `outline_method` and `tokens_estimated` before treating a heuristic result as definitive.

---

## What `dirlens` adds to a project map

| Layer | Information |
| --- | --- |
| Filesystem | Tree, sizes, modification times, extension totals, hidden files, filtering, sorting, and depth limits. |
| Git | Last commit per file, author/date, working-tree status, recent history, and changes since a ref. |
| Context | Exact BPE token counts, language totals, output estimates, and a token budget. |
| Code | Public API outlines, long functions, local imports, circular-dependency detection, and impact queries. |
| Maintenance | TODO/FIXME inventory, test-coverage hints, entry-point candidates, and known config files. |

The analysis is deliberately lightweight. It is a navigation aid, not a replacement for reading the relevant source or running tests. Every "smart" feature below runs a best-effort tier first and falls back when the environment doesn't support it — see [Analysis methods and limits](#analysis-methods-and-limits) for which tier ran and what it misses.

### Filesystem, display, and workflow

- **Single binary** — no Python, no Node; download it and run it (macOS / Linux / Windows)
- **Broad `tree` compatibility** — `-a -d -f -g -l -p -u -r -s -t -c -L -D -P -I -n -J --prune` and more behave like `tree`; `dirlens`-only behavior lives behind `-G` (gitignore), `-S` (sort by size), `-e` (extension filter), and `-C` (clipboard)
- **Color output** — directories, files, and symlinks are colored distinctly (see [Color legend](#color-legend))
- **Automatic size units** — bytes / KB / MB / GB / TB
- **Directory sizes** — the total size of a directory's subtree, computed in parallel for speed. This total is always the raw on-disk size and, unlike the tree's own entries, is **not** affected by `-G` (gitignore exclusion) — matching `du` semantics and consistent with the legacy Python implementation. The children listed, the token counts, and everything else analysis-related *are* correctly excluded by `-G`; only `size`/`size_human` is not. Don't infer that ignored files were analyzed just because a directory's size includes them
- **Item counts** — dirs/files directly under each directory
- **Extension totals** — a summary of file types across the whole tree
- **`.gitignore` support (two tiers)** — `-G` excludes matches. With git available, matching uses `git check-ignore` itself (nested rules, `!` negation, `**`, global excludes, `.git/info/exclude` — all supported); without git, it falls back automatically to a builtin matcher
- **Relative modification times** — `--date` / `-D`
- **Extension filter** — `-e py` shows only that extension
- **Pattern filters** — `--exclude` / `-I` and `--include` / `-P` accept wildcards (repeatable)
- **Size filters** — `--min-size` / `--max-size`
- **Prune empty branches** — `--prune` hides branches left empty after filtering (`tree --prune`-compatible)
- **Permissions display** — `-p` for the permission string, `-u` for the owner name (`tree -p`/`-u`-compatible)
- **Symlink expansion** — `-l` follows a symlinked directory's target and shows it with `→` (cycle-safe)
- **Full paths** — `-f` shows each entry's path from the root (`tree -f`-compatible)
- **Reverse sort** — `-r` reverses the active sort order (`tree -r`-compatible)
- **Disk-usage bar** — `--bar` visualizes an entry's share of its parent directory
- **Emoji icons** — `--emoji` adds an icon per extension
- **Markdown output** — `-m` wraps the tree in a code block
- **JSON output** — `--json` / `-J` emits a stable, machine-readable schema (**versioned with `schema_version`**; symlinks carry `symlink: {target, broken}`, outlines carry the tier actually used as `outline_method`)
- **HTML report** — `--html` generates a browsable, collapsible tree you can open in a browser
- **Clipboard copy** — `-C` copies the output automatically (ANSI codes stripped); if the copied content looks like it contains a `.env` file or a secret key, a warning is printed to stderr
- **AI chat paste mode** — `--ai` applies gitignore exclusion, dates, Markdown, and clipboard copy in one shot (for a human pasting into a chat)
- **Agent analysis mode** — `--agent` applies every AI/agent-oriented analysis feature below in one shot (no color, no clipboard — safe for autonomous execution)
- **Capability report** — `--check` shows which analysis tier is actually available in this environment (exits 1 if anything is degraded)
- **Hidden files** — `-a` toggles them on
- **Size sort** — `-S` sorts largest first
- **Interactive TUI** — `-i` launches a tree browser with expand/collapse, filtering, and a detail pane
- **Git status overlay** — `--status` overlays `[M]`/`[??]`/`[A]` marks on the tree
- **Heat coloring** — `--heat age|size|churn` colors filenames along a gradient
- **Largest-files list** — `--top N` shows a flat list without the tree, useful for disk cleanup
- **Duplicate detection** — `--dupes` finds files with identical content and the wasted space
- **Directory comparison** — `--compare DIR` shows additions/deletions/changes between two trees
- **Config files** — `~/.config/dirlens/config.toml` and a project's `.dirlens.toml` define default flags; named presets (`--preset`) are read from the global config only
- **Shell completions / man page** — `--completions <shell>` / `--man`
- **Secret-file warning** — copying via `--ai`/`-C` warns on stderr if the content looks like it contains a `.env` file or a private key
- **Progress spinner** — long scans show a spinner on the terminal (suppressed when not attached to one)

### Code and context layers (all enabled together by `--agent`)

These are the signals an AI chat or coding agent uses to decide what to read and how far to explore — not a way to pin down a file's exact behavior. After narrowing to the relevant files, read them. Each of these is a best-effort tier with a documented fallback; the tier that actually ran is always reported (`--check`, or the `capabilities`/`analysis` block in `--agent --json`).

- **Token counting** — `-T` shows a per-file token count: **exact `o200k_base` BPE**. Files above the per-file read limit are proportionally estimated instead (`tokens_estimated: true` in JSON). The limit defaults to 5 MB and scales up automatically based on host memory (override with `DIRLENS_MAX_FILE_BYTES`; the active value is visible via `--check`'s `capabilities.max_file_bytes`). The fallback tier is a character-count heuristic. The summary also breaks tokens down by language. Repeat runs are faster thanks to a persistent cache (disable with `--no-cache`, or clear it with `--clear-cache`)
- **Git integration** — `-H` shows each file's last commit (message, relative date), scanning up to the most recent 2,000 commits. Also surfaces frequently-changed files (hotspots)
- **TODO/FIXME extraction** — `-K` extracts `TODO`/`FIXME`/`HACK`/`XXX` comments with line numbers
- **Missing-test detection** — `-V` flags source files with no corresponding test file found. Beyond naming conventions, it **follows transitive imports from test files** and understands Rust's inline tests (`#[cfg(test)]`)
- **Entry-point detection** — `-N` flags likely entry files based on known names (`main.py`, `index.js`) and `package.json`'s `main`/`bin` fields
- **Symbol outlines** — `-O` extracts function/class names using per-language AST parsers (Python / JS·TS / Rust / Go / C / Java / Ruby / PHP / C# / Kotlin / Swift), falling back to regex on a parse failure. `-A` narrows the result to the public API only. JSON output includes the first doc-comment line and the line range; the summary also lists the **top 5 longest functions**
- **Import/dependency graph** — `-M` analyzes local import relationships between files and shows `imports×N` (dependency count), `used-by×N` (dependent count), and circular dependencies. **Resolution understands tsconfig `paths`, package.json `imports`, `go.mod`, Rust's module tree (`crate::`/`self::`/`super::`), Java/Kotlin FQCNs, PHP `use`, and Ruby's `require_relative`**
- **Impact queries** — `--focus FILE` shows "what could break if I change this file" as the transitive closure of dependents/dependencies (supports `--json`)
- **Token budget** — `--budget N` automatically trims the text output to fit within N tokens (measured with the same BPE counter, trimming depth → annotations → tree rows in that order; omitted content is noted along with the token count needed to show everything). `--estimate` gives a per-depth cost estimate up front
- **Diff mode** — `--since REF` shows only files changed since a given git ref (includes untracked files; lists deleted files too)
- **stdin file list** — `git diff --name-only | dirlens --stdin` analyzes only the listed files (tokens, outline, TODOs; supports `--json`)
- **Public API diff** — `--api-diff REF` compares public symbols against a git ref, for detecting breaking changes
- **Graph export** — `--mermaid` / `--dot` emit the import graph as diagram source; `--csv` emits a file-metadata table
- **`--pack`** — `--pack FILE...` formats file contents plus token counts into paste-ready Markdown
- **MCP server** — `--mcp` runs an MCP server exposing 9 tools an agent can call natively: `analyze`, `tree`, `outline`, `imports`, `focus`, `todos`, `since`, `history`, `api_diff`. It runs in the same binary/process as the CLI, so `DIRLENS_*` environment variables (`DIRLENS_MAX_FILE_BYTES`/`DIRLENS_MAX_WORKERS`/`DIRLENS_GITIGNORE`/`DIRLENS_AST`/`DIRLENS_TOKENS`/`DIRLENS_COMPAT`/`DIRLENS_CACHE`) and the persistent token cache apply the same way over MCP as they do on the CLI. `analyze`/`tree` support a token `budget`, cost `estimate`, and `top`; `imports`/`todos` return a flat array of only the matching files plus a `limit` (truncation is reported via `truncated`/`total_files`; `imports` also supports `format: mermaid/dot`); `outline` accepts multiple files at once (unresolvable paths are reported in `errors`, duplicates are deduped, nested symbols carry `parent`) and returns the public API when `files` is omitted; `api_diff` includes untracked files annotated `(untracked)`; `tree`/`analyze` accept `include_ignored` to turn off gitignore exclusion. Most MCP hosts cap a single response (Claude Code defaults to 25,000 tokens) — if `estimate` reports a result over that cap, pass a `budget` under it (the estimate table flags the offending row with `⚠ exceeds host cap` and a suggested budget). **Registration is guided by `dirlens --mcp-setup`**, which prints your binary's absolute path pre-filled into a Claude Code one-liner and Claude Desktop/Cursor config JSON, ready to paste
- **Config-file detection** — `-F` detects and lists known configuration files (`.env`, `tsconfig.json`, `Makefile`, etc.)
- **Structured errors** — `--json` always returns valid JSON even when part of the analysis fails, reporting the failure machine-readably in the `errors` array

---

## Common commands

```bash
# ── Agent / AI analysis ──────────────────────────────────────
dirlens --agent          # token counts, git info, TODOs, missing tests, entry points,
                          # outlines, import graph, config files — all at once
                          # (--agent already implies --no-color)
dirlens --agent --json   # same, as JSON (for scripts/agents)
dirlens --check          # which analysis tier is available here (exit 1 if degraded)

# ── AI/agent analysis (individual flags) ─────────────────────
dirlens -T                # per-file token count (exact BPE)
dirlens -H                # last-commit info (needs git)
dirlens -K                # extract TODO/FIXME/HACK/XXX
dirlens -V                # source files with no detected test
dirlens -N                # mark likely entry-point files
dirlens -O                # function/class outline (AST)
dirlens -O src/main.py    # outline of a single file (also shows tokens/TODOs)
dirlens -A                # public-API-only outline
dirlens -M                # local import/dependency analysis
dirlens -F                # detect and list config files (.env, tsconfig.json, ...)

# ── Context efficiency and impact (agent-oriented) ───────────
dirlens --agent --estimate       # estimate output cost per depth (to choose a --budget)
dirlens --agent --budget 3000    # auto-trim output to 3000 tokens
                                  # (omitted content is noted as "... N more entries",
                                  #  along with the token count needed to show it all)
dirlens --focus src/main.py -G   # this file's dependents/dependencies (direct + transitive)
dirlens --since HEAD -G          # only files changed since the last commit
git diff --name-only | dirlens --stdin --json   # analyze only the listed files
dirlens --api-diff v1.0.0        # public API diff (detect breaking changes)
dirlens --mcp-setup              # print MCP registration steps (copy-paste ready)
dirlens --mcp                    # run as an MCP server (stdio)

# ── Human-friendly modes ──────────────────────────────────────
dirlens -i                # interactive TUI browser
dirlens --status          # overlay git status marks on the tree
dirlens --heat age        # gradient color by recency (also: size, churn)
dirlens --top 10          # flat list of the 10 largest files/directories
dirlens --dupes           # find duplicate files (size + content hash)
dirlens --compare ../v2   # diff two directory trees
dirlens --pack src/a.py src/b.py -C   # format file contents for pasting, then copy

# ── Graph and table export ────────────────────────────────────
dirlens -M --mermaid      # import graph as Mermaid
dirlens -M --dot          # import graph as Graphviz DOT
dirlens --csv -T -G       # file metadata as CSV

# ── Language and configuration ────────────────────────────────
dirlens --lang ja         # Japanese output (default is English)
dirlens --preset quick    # apply a named preset from the config file
dirlens --no-config       # ignore config files
dirlens --completions zsh > ~/.zfunc/_dirlens   # generate shell completions
dirlens --man             # generate the man page (roff)

# ── Display control ────────────────────────────────────────────
dirlens                  # current directory
dirlens ~/Desktop        # a specific directory
dirlens -L 2             # limit to 2 levels deep (tree -L-compatible)
dirlens -d               # directories only (tree -d-compatible)
dirlens -a               # include hidden files
dirlens -r               # reverse sort order (tree -r-compatible)
dirlens --filesfirst     # list files before directories
dirlens -f               # full paths from the root (tree -f-compatible)

# ── Filtering ────────────────────────────────────────────────
dirlens -G               # exclude .gitignore matches
dirlens -G --prune       # gitignore exclusion + prune empty branches
dirlens -e py            # only .py files
dirlens -P '*.md'        # only .md files (tree -P-compatible)
dirlens -I '*.log'       # exclude .log (tree -I-compatible)
dirlens --exclude 'dist' --exclude '*.log'   # multiple excludes
dirlens --min-size 1M    # only files 1 MB or larger
dirlens --max-size 100K  # only files 100 KB or smaller

# ── Sorting ────────────────────────────────────────────────────
dirlens -S               # largest first
dirlens -t               # by modification time, newest first (tree -t-compatible)
dirlens -c               # by status-change time (tree -c-compatible)
dirlens -t -r            # by modification time, oldest first

# ── Detail ────────────────────────────────────────────────────
dirlens --date           # relative modification times (tree -D-compatible is -D)
dirlens --bar            # disk-usage bar
dirlens --emoji          # emoji icons
dirlens -p -u -g         # permissions, owner, group (tree-compatible)
dirlens -l               # expand symlinked directories (tree -l-compatible)

# ── Output format ──────────────────────────────────────────────
dirlens -m               # Markdown code block
dirlens --json           # JSON (tree -J-compatible is -J)
dirlens --html           # HTML report (default: dirlens.html)
dirlens -C               # copy to clipboard
dirlens --no-color > dirlens.txt   # write to a text file
```

Use `-G` with individual analysis flags to honor `.gitignore`. `--agent` enables it automatically.

## Output budgets for large repositories

Before sending a large project map to a chat or agent, estimate it:

```bash
dirlens --agent --estimate
```

Then choose a hard output budget:

```bash
dirlens --agent --budget 3000
```

`dirlens` first reduces tree depth, then annotations, then tree rows. It reports omitted entries and the measured token count, so you can decide whether a larger budget is worthwhile.

`-L` only restricts the displayed tree depth; project-wide analysis aggregates still cover the full scan. Directory sizes are raw disk sizes and, unlike the tree entries, include ignored content—do not infer that ignored files were analyzed merely from a directory's size.

## Options

Run `dirlens --help` for the authoritative, version-specific option list.

### Tree and display

| Option | Description |
| --- | --- |
| `-L N` | Limit displayed tree depth. |
| `-a` | Include hidden files. |
| `-d` | Directories only. |
| `-G` | Respect `.gitignore` (two-tier: exact via `git check-ignore` when available, otherwise a builtin approximation). |
| `-e EXT` | Show only files with this extension (e.g. `-e py`). |
| `-I PATTERN` | Exclude matching paths. |
| `-P PATTERN` | Include only matching paths. |
| `--min-size` / `--max-size` | Filter by file size (e.g. `1M`, `500K`). |
| `--prune` | Hide directories left empty by a filter. |
| `-f` | Show full paths from the root. |
| `--filesfirst` | List files before directories. |
| `-s` | Show sizes (always on; kept for `tree` compatibility). |
| `-D` | Show modification times, relative. |
| `-S` | Sort by size, largest first. |
| `-t` | Sort by modification time, newest first. |
| `-c` | Sort by status-change time. |
| `-r` | Reverse the selected ordering. |
| `-p` / `-u` / `-g` | Show permissions / owner / group (fully supported on macOS/Linux; approximated on Windows). |
| `-l` | Expand symlinked directories (cycle-safe). |
| `--bar` | Show a disk-usage bar relative to the parent directory. |
| `--emoji` | Show an emoji icon per extension. |
| `--top N` | Show the largest files/directories as a flat list. |

### Analysis and Git

| Option | Description |
| --- | --- |
| `--agent` | Full agent-oriented analysis, without ANSI color. |
| `--ai` | Clipboard-oriented Markdown context for AI chat. |
| `-T` | Show BPE token counts. |
| `-O PATH` | Show functions and classes in one file. |
| `-A` | Show public API across the project. |
| `-M` | Show local import relationships and cycles. |
| `--focus PATH` | Show dependencies and dependents of a path. |
| `-V` | Mark files without a detected test. |
| `-K` | Mark TODO/FIXME occurrences. |
| `-N` | Mark entry-point candidates. |
| `-F` | Mark recognized configuration files. |
| `-H` | Show recent commits and hotspots. |
| `--since REF` | Limit analysis to paths changed since a Git ref. |
| `--api-diff REF` | Compare the public API against a Git ref. |
| `--status` | Overlay Git status. |

### Formats and integrations

| Option | Description |
| --- | --- |
| `--json` | Emit machine-readable JSON. |
| `--csv` | Emit CSV where supported. |
| `--mermaid` / `--dot` | Export import relationships as a diagram. |
| `-m` | Emit a Markdown code block. |
| `--html [FILE]` | Generate an HTML report (default: `dirlens.html`). |
| `-C` | Copy output to the clipboard (warns on stderr if it looks like it contains secrets). |
| `-n` | Disable color output. |
| `-i` | Launch the interactive TUI browser. |
| `--check` | Print a capability report; exits 1 if anything is degraded. |
| `--heat MODE` | Color filenames by `age`/`size`/`churn`. |
| `--top N` | Flat list of the largest files/directories (see above). |
| `--dupes` | Find duplicate files by size and content hash. |
| `--compare DIR` | Diff this tree against another directory. |
| `--pack FILE...` | Bundle file contents and token counts for pasting. |
| `--mcp` | Run the MCP server. |
| `--mcp-setup` | Print MCP setup instructions. |
| `--lang ja` | Use Japanese output. |

### Configuration and shell integration

| Option | Description |
| --- | --- |
| `--preset NAME` | Apply a named argument set from `[presets]` in the config file. |
| `--no-config` | Ignore all config files (also: `DIRLENS_CONFIG=off`). |
| `--no-cache` | Skip the persistent token-count cache for this run (also: `DIRLENS_CACHE=off`). |
| `--clear-cache` | Delete all persistent token-count cache files and exit. |
| `--completions SHELL` | Print a shell-completion script (bash/zsh/fish/powershell/elvish). |
| `--man` | Print the man page (roff). |
| `--version` | Show the version (no short form — `-V` is taken by `--missing-tests`). |

## Color legend

| Color | Meaning |
| --- | --- |
| Blue (bold) | Root directory |
| Cyan (bold) | Subdirectory |
| Green | File |
| Magenta | Symlink |
| Dim | Size annotation |

## Analysis methods and limits

`dirlens` favors a best available method with a safe, documented fallback for every "smart" feature. Use `dirlens --check` to see what is actually active in your environment (also available programmatically via `--agent --json`'s `capabilities`/`analysis` blocks).

| Feature | Best tier | Fallback | Notes |
| --- | --- | --- | --- |
| `.gitignore` (`-G`) | `git check-ignore` — the real git engine (nested rules, `!` negation, `**`, global excludes, `.git/info/exclude`) | Builtin matcher (an approximation covering basic patterns only) | Falls back when git or a repository isn't available |
| Token counts (`-T`) | Exact `o200k_base` BPE (vocabulary data ships in the binary); results are cached persistently, see [Configuration](#configuration) | Character-count heuristic | Files above the per-file read limit (5 MB by default, scaling up to 15/30/50/80 MB based on host memory; override with `DIRLENS_MAX_FILE_BYTES`) are proportionally estimated even on the best tier, flagged as `tokens_estimated: true` in JSON. The active limit is visible via `--check`'s `capabilities.max_file_bytes`. Tokenizers differ by model, so counts are only a rough guide outside the OpenAI family. Non-regular files (FIFOs, sockets, devices) are treated as size 0 and never read. Files with a binary extension (`.png`, `.zip`, …) are sniffed: if the first 8 KB is NUL-free valid UTF-8 they are analyzed as text, so a disguised text file still gets tokens/TODOs |
| Outlines (`-O`/`-A`) | Per-language AST parsers (Python, JavaScript/TypeScript, Rust, Go, C, Java, Ruby, PHP, C#, Kotlin, Swift). HTML outlines inline `<script>` blocks (external `src` scripts are not followed). No false positives from string literals; captures the first doc line and the line range. Python's public/private judgment is scope-aware — a local `def`/`class` inside a function and its members are private; a class's methods are judged public only if the class itself is public. Nested symbols carry their enclosing symbol's name (`parent` in JSON; `def Class.method` / `fn Type::method` in text) | Regex extraction (no doc line or line range; visibility judged by name only) | A syntax error in a file falls back automatically; JSON's `outline_method` (`"ast"`/`"regex"`) reports which tier actually ran |
| Import graph (`-M`/`--focus`) | AST extraction plus manifest resolution (TypeScript `paths`/`baseUrl`, package.json `imports`, `go.mod`, Rust's module tree, Java/Kotlin FQCNs, PHP `use`/`require`, Ruby `require_relative`). Nested `Cargo.toml` files are detected as crate boundaries, so monorepos/workspaces resolve per crate. Edges from a bare `mod` declaration are excluded from cycle detection | Regex plus relative-path resolution | External packages (e.g. inside `node_modules`) are never resolved to source and are reported as external. C#/Swift have no local resolution. `tsconfig`/`package.json` `imports`/`go.mod` are read only from the scan root, so `--focus` on a file inside a nested JS/Go sub-project carries a caveat note |
| Missing-test detection (`-V`) | Naming conventions plus transitive imports from test files plus Rust inline-test detection | Naming conventions only | This is not code coverage. Only `.py/.js/.jsx/.ts/.tsx/.go` (plus `.rs` when AST is enabled) are judged; other files report `has_test: null` in JSON. Rust's `lib.rs`/`main.rs`/`mod.rs` are exempt by name (they're typically re-export/wiring files) — note this also means a `lib.rs` full of real logic won't be flagged either |
| Entry points (`-N`) | Known filenames plus `package.json`'s `main`/`bin` fields | — | |
| TODO extraction (`-K`) | Word-boundary text match | — | Can match the word inside a string literal, not only inside a comment |
| Git integration (`-H`/`--status`/`--since`/`--api-diff`) | Shells out to `git`; `-H` scans the most recent 2,000 commits. Running from a repository subdirectory is handled correctly — git's repo-root-relative paths are remapped to the scan root. `--api-diff` includes untracked files, annotated `(untracked)` | — | Files whose only changes are older than the 2,000-commit window report no history |

For consequential decisions, inspect the actual source after using the project map to find it.

## Configuration

Environment variables include:

| Variable | Purpose |
| --- | --- |
| `DIRLENS_LANG` | Default language, for example `ja`. |
| `DIRLENS_CONFIG` | Set to `off` to skip loading all config files (also: `--no-config`). |
| `DIRLENS_MAX_FILE_BYTES` | Maximum bytes read per file for analysis. An invalid value (not an integer ≥ 1) prints a warning and falls back to the automatic, memory-based limit. |
| `DIRLENS_MAX_WORKERS` | Override the parallel-worker limit (invalid values print a warning and are ignored). |
| `DIRLENS_GITIGNORE` | Enable or disable gitignore handling. |
| `DIRLENS_AST` | Enable or disable AST analysis. |
| `DIRLENS_TOKENS` | Enable or disable token counting. |
| `DIRLENS_CACHE` | Enable or disable the persistent token cache. |
| `DIRLENS_COMPAT` | Set to `python` to pin every fallback tier plus Japanese output, for byte-for-byte comparison against the legacy Python implementation (used by golden tests). |

The persistent token cache lives at `~/.cache/dirlens/` (or `$XDG_CACHE_HOME/dirlens/`), keyed per
project. Pass `--no-cache` (or set `DIRLENS_CACHE=off`) to skip it for one run, or run
`dirlens --clear-cache` to delete all cached files.

`dirlens` reads a global config file (`~/.config/dirlens/config.toml`, or `$XDG_CONFIG_HOME/dirlens/config.toml`), then a project config file (the nearest `.dirlens.toml` found by walking up from the target directory). Precedence is **CLI flags > project config > global config**.

Example configuration:

```toml
# ~/.config/dirlens/config.toml
lang = "ja"
gitignore = true    # always apply -G
emoji = true        # always apply --emoji
exclude = ["dist"]  # always exclude this pattern

[presets]           # apply with: dirlens --preset quick
quick = ["-L", "2", "-G"]
paste = ["--ai", "-L", "3"]
```

Supported keys: `lang`, `gitignore`, `all`, `date`, `emoji`, `markdown`, `no_color`, `bar`, `prune`, `filesfirst`, `follow`, `full_path`, `depth`, `min_size`, `max_size`, `exclude`, `include`, and `[presets]`.

`[presets]` is honored **only in the global config**. A project's `.dirlens.toml` lives inside the tree being scanned, so presets there are ignored with a warning — otherwise a scanned directory could inject flags (such as `-C`) into its own analysis.

## Notes and caveats

- A directory's size is the sum of **all subfiles**, including hidden files and anything matched by `.gitignore`.
- Computing directory sizes (stat-ing the full subtree for each total) runs across multiple cores for speed on native builds — a transparent optimization. Output is byte-identical to a serial computation, and there's no regression on a single core (`DIRLENS_MAX_WORKERS` adjusts the thread count).
- Heavier analysis (token counting, outlines, TODOs, import analysis) also runs in parallel on native builds, since each of these depends only on a file's own content. Output is byte-identical to running serially (`--agent`/`-T`/`-O`/`-M` are roughly 3x faster on 4 cores).
- **`+` notation** — when part of a subdirectory couldn't be read, sizes show as `1.5+ KB` (at least 1.5 KB) and counts as `3+ dirs`.
- **Permission denied** — unreadable directories are shown in bold red as `[Permission Denied]` and skipped.
- **Symlinks** are shown as `→ target path`; `-l` expands a symlinked directory's target (cycle-safe).
- Running at your home folder (`~/`) or filesystem root (`/`) can be slow — size computation recurses to the bottom regardless of the display depth.
- **`-L` (depth limit) only shallows the displayed tree** — analysis aggregates (TODO count, estimated tokens, per-language breakdown, longest functions, etc.) always reflect the full project. Only `Total N directories, M files` and the extension counts reflect what's actually displayed (matching `tree`'s own semantics).
- The `--json` output schema is treated as a **stable public API**: the top-level `schema_version` only increases on a field rename, removal, or type change (additions are backward-compatible).
- `-p` (permissions), `-u` (owner), and `-g` (group) are fully supported on macOS/Linux; on Windows they show an approximation derived from file attributes and numeric IDs.

## Instruction templates for AI agents

If you want an agent (Claude Code, Cursor, etc.) to use `dirlens --agent` as its go-to project-exploration step, paste one of these templates as-is into a global rule file such as `CLAUDE.md` or `.cursorrules`. Pick the one that matches your setup (the prose above each file's divider is for humans — only the content below the divider needs to be pasted):

- **[`AGENT_RULE.md`](AGENT_RULE.md)** — works even where `dirlens` might be missing or outdated; emphasizes checking for it first and falling back gracefully.
- **[`AGENT_RULE_STRICT.md`](AGENT_RULE_STRICT.md)** — a stricter version that assumes the `dirlens` CLI is always available (skips the existence check, pushes full feature use).
- **[`AGENT_RULE_MCP.md`](AGENT_RULE_MCP.md)** — for environments with both the CLI and the MCP server (`--mcp`) registered; adds guidance on choosing between the two and falling back when one misbehaves.

## Development

```text
rust/
├── crates/dirlens-core/   # analysis core (I/O only via provider traits; native + wasm)
├── crates/dirlens-cli/    # CLI (clap, std providers, parallel size prewarming)
└── crates/dirlens-wasm/   # wasm bindings (analyzes a host-supplied tree)
tests/golden/              # golden tests (snapshot comparison, tiered adversarial checks)
```

```bash
cd rust && cargo build --release && cargo test --workspace
cargo run -- --agent -L 2   # smoke-test a debug build
python3 tests/golden/run.py verify --bin rust/target/release/dirlens   # snapshot check
python3 tests/golden/tier_check.py --bin rust/target/release/dirlens   # gitignore tier check
python3 tests/golden/ast_check.py  --bin rust/target/release/dirlens   # AST tier check
```

See `tests/golden/README.md` / `tests/golden/DELTAS.md` for compatibility verification against the legacy Python implementation (`run.py live`) and the ledger of intentional differences.

## License

Apache-2.0. Use, redistribution, and modification are permitted; retaining the copyright notice and `NOTICE` is required. See [LICENSE](LICENSE) and [NOTICE](NOTICE) for details.

Token counting bundles the [tiktoken](https://github.com/openai/tiktoken) `o200k_base` vocabulary data (MIT License).

Copyright 2026 Igarin
