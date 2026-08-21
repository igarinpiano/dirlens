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

It is a **single Rust binary with no runtime dependencies**, with compatibility for major `tree` flags.

Output is English by default. Use `--lang ja`, set `lang = "ja"` in `~/.config/dirlens/config.toml`, or set `DIRLENS_LANG=ja` for Japanese output.

> The legacy Python implementation (v1.0.x) is available on the [`python`](https://github.com/igarinpiano/dirlens/tree/python) branch. Golden tests verify output compatibility with the Rust implementation.

---

## Install

### npm (recommended on every OS)

```bash
npm install -g dirlens
```

The package automatically selects a native binary for macOS arm64/x64; Linux arm64/x64, musl x64 (Alpine), ppc64, and s390x; and Windows x64/arm64. Windows runs a native `dirlens.exe`.

### Download a binary

Download the archive for your platform from [GitHub Releases](https://github.com/igarinpiano/dirlens/releases), then put the binary on your `PATH`. Release binaries are also provided, where available, for armv7 (32-bit Linux ARM), i686 (32-bit Linux/Windows), and riscv64gc; these targets are not available from npm because Node.js does not publish official builds for them.

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

This downloads a prebuilt GitHub Release binary through the crates.io metadata, without a local compilation.

### Build from source

```bash
git clone https://github.com/igarinpiano/dirlens.git
cd dirlens/rust
cargo build --release
# Binary: target/release/dirlens
```

---

## One project map, three ways to use it

| Mode | Intended user | What it does |
| --- | --- | --- |
| Normal CLI | People | Browse a project interactively or use it as a `tree` replacement. |
| `--ai` | People using AI chat | Copies Markdown context to the clipboard for pasting into a chat. |
| `--agent` | Coding agents | Produces a color-free, clipboard-free project analysis for autonomous exploration. |

## Quick start

```bash
# Use it like tree: inspect the structure first
dirlens

# For an AI chat: copy Markdown context to your clipboard
dirlens --ai

# For a coding agent: generate a project map before exploration
dirlens --agent

# Estimate output cost before analyzing an unfamiliar large repository
dirlens --agent --estimate

# Print MCP registration instructions
dirlens --mcp-setup
```

## Example output

### Basic tree

```text
$ dirlens -L 2
my-project/ (4 dirs, 10 files, 42.1 KB, 2 hours ago)
├── src/ (1 dir, 4 files, 18.4 KB, 10 minutes ago)
│   ├── lib.rs (8.2 KB, 10 minutes ago)
│   ├── main.rs (3.1 KB, 2 hours ago)
│   └── utils/ (2 files, 7.1 KB, 1 hour ago)
├── tests/ (2 files, 9.4 KB, 1 hour ago)
├── Cargo.toml (1.3 KB, 2 hours ago)
└── README.md (13.0 KB, 3 days ago)

Total  4 directories, 10 files
```

### `--ai`: paste-ready AI context

```bash
dirlens --ai
```

`--ai` emits Markdown optimized for pasting into an AI chat and copies it to the clipboard. It includes the project tree and selected metadata. It is for a human-operated workflow; do not use it from autonomous agents because clipboard access is a side effect.

### `--agent`: agent analysis mode

```bash
dirlens --agent
```

`--agent` disables ANSI color and enables the complete analysis set: token counts, Git metadata and status, TODO/FIXME markers, test hints, entry-point candidates, outlines, import relationships, config-file detection, language token totals, and long-function reporting. It also respects `.gitignore` by default.

Use JSON when a program needs to consume the result:

```bash
dirlens --agent --json
```

The JSON output stays valid even when a partial analysis cannot run. Check `errors`, `capabilities`, and per-file fields such as `outline_method` and `tokens_estimated` before treating a heuristic result as definitive.

---

## What `dirlens` adds to a project map

| Layer | Information |
| --- | --- |
| Filesystem | Tree, sizes, modification times, extension totals, hidden files, filtering, sorting, and depth limits. |
| Git | Last commit per file, author/date, working-tree status, recent history, and changes since a ref. |
| Context | Exact BPE token counts, language totals, output estimates, and a token budget. |
| Code | Public API outlines, long functions, local imports, circular-dependency detection, and impact queries. |
| Maintenance | TODO/FIXME inventory, test-coverage hints, entry-point candidates, and known config files. |

The analysis is deliberately lightweight. It is a navigation aid, not a replacement for reading the relevant source or running tests.

## Common commands

```bash
# Agent / AI analysis
dirlens --agent
dirlens --agent --json
dirlens --agent --estimate
dirlens --agent --budget 3000

# Impact of a change: direct and transitive import neighbors
dirlens --focus src/config.rs -G

# Changes since a Git ref
dirlens --since HEAD -G

# Analyze only paths supplied on stdin
git diff --name-only | dirlens --stdin --json

# Individual analyses
dirlens -O src/main.rs       # outline one file
dirlens -A                   # public API across the project
dirlens -M -G                # import graph and cycles
dirlens -V -G                # files without a detected test
dirlens -K -G                # TODO/FIXME markers
dirlens -N -G                # entry-point candidates
dirlens -F -G                # recognized configuration files
dirlens -H -L 1              # recent history / hotspots
dirlens --api-diff v1.0.0    # public API changes from a ref
dirlens --status             # overlay Git status on the tree

# Alternative displays and exports
dirlens --top 10
dirlens -M --mermaid
dirlens -M --dot
dirlens --pack src/a.rs src/b.rs
dirlens --compare ../other-project
dirlens --dupes
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

`-L` only restricts the displayed tree depth; project-wide analysis aggregates still cover the full scan. Directory sizes are raw disk sizes and, unlike the tree entries, include ignored content—do not infer that ignored files were analyzed merely from a directory’s size.

## Options

Run `dirlens --help` for the authoritative, version-specific option list.

### Tree and display

| Option | Description |
| --- | --- |
| `-L N` | Limit displayed tree depth. |
| `-a` | Include hidden files. |
| `-d` | Directories only. |
| `-G` | Respect `.gitignore`. |
| `-I PATTERN` | Exclude matching paths. |
| `-P PATTERN` | Include only matching paths. |
| `-s` | Show sizes. |
| `-D` | Show modification times. |
| `-S` | Sort by size, largest first. |
| `-t` | Sort by modification time. |
| `-r` | Reverse the selected ordering. |
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
| `--mcp` | Run the MCP server. |
| `--mcp-setup` | Print MCP setup instructions. |
| `--lang ja` | Use Japanese output. |

## Analysis methods and limits

`dirlens` favors a best available method with a safe fallback. Use `dirlens --check` to see what is available in your environment.

- Token counts use `o200k_base` BPE. Files larger than the read limit are proportionally estimated and identified as such in JSON.
- Outlines use language-specific AST parsers for Python, JavaScript/TypeScript, Rust, Go, C, Java, Ruby, PHP, C#, Kotlin, and Swift. A syntactically invalid file can fall back to regex extraction, which can miss symbols; JSON reports `outline_method`.
- Import analysis combines AST extraction with manifest resolution, including TypeScript paths, package imports, Go modules, Rust module trees, Java/Kotlin FQCNs, PHP `use`, and Ruby `require_relative`. External packages are not resolved to source.
- Test detection is based on naming conventions, imports from tests, and Rust inline tests. It is not code coverage.
- Entry points and config files are recognized by known names and manifest fields; custom conventions may not be detected.
- TODO/FIXME detection is a word-boundary text match and can include string literals.
- Git history analysis scans the most recent 2,000 commits.

For consequential decisions, inspect the actual source after using the project map to find it.

## Configuration

Environment variables include:

| Variable | Purpose |
| --- | --- |
| `DIRLENS_LANG` | Default language, for example `ja`. |
| `DIRLENS_MAX_FILE_BYTES` | Maximum bytes read per file for analysis. |
| `DIRLENS_MAX_WORKERS` | Override the parallel-worker limit. |
| `DIRLENS_GITIGNORE` | Enable or disable gitignore handling. |
| `DIRLENS_AST` | Enable or disable AST analysis. |
| `DIRLENS_TOKENS` | Enable or disable token counting. |
| `DIRLENS_CACHE` | Enable or disable the persistent token cache. |

The persistent token cache lives at `~/.cache/dirlens/` (or `$XDG_CACHE_HOME/dirlens/`), keyed per
project. Pass `--no-cache` (or set `DIRLENS_CACHE=off`) to skip it for one run, or run
`dirlens --clear-cache` to delete all cached files.

Example configuration:

```toml
# ~/.config/dirlens/config.toml
lang = "ja"
```

## Development

The Rust workspace lives in [`rust/`](rust/).

```bash
cd rust
cargo test
cargo run -- --agent -L 2
```

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
