# cq — Usage Guide

Semantic code query tool. 71 languages. Three precision tiers: tree-sitter, stack graphs, LSP. Works on broken code. Languages auto-install on first use.

---

## Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Commands](#commands)
4. [Output Modes](#output-modes)
5. [Global Flags](#global-flags)
6. [MCP Server Setup](#mcp-server-setup)
7. [Daemon Mode](#daemon-mode)
8. [Grammar Management](#grammar-management)
9. [Configuration](#configuration)
10. [Scoping](#scoping)
11. [Exit Codes](#exit-codes)

---

## Installation

**npm** (no Rust toolchain needed):
```
npx -y @codequery/cli
```

**pip**:
```
uvx codequery-cli
```

**Cargo**:
```
cargo install codequery-cli
```

**GitHub release binaries** (Linux, macOS, Windows):
```
https://github.com/jmfirth/codequery/releases
```

**From source**:
```
git clone https://github.com/jmfirth/codequery
cd codequery
cargo build --release
# binary at target/release/cq
```

---

## Quick Start

```bash
# Orient in a project
cq tree --depth 2

# List all symbols in a file
cq outline src/main.rs

# Find where a symbol is defined
cq def handle_request

# Extract the full source of a function
cq body Router::add_route
```

All commands run from the project root (auto-detected from `.git`, `Cargo.toml`, `package.json`, `go.mod`, etc.) or from any subdirectory. No configuration required.

---

## Commands

### Navigation and Lookup

#### `def <symbol>`

Find where a symbol is defined across the project.

```bash
$ cq def SymbolKind
```
```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/symbol.rs:39:0 enum SymbolKind @@
```

Supports qualified names (`Struct::method`) and module paths. Uses a fast text pre-filter (memchr) before parsing, so it's sub-100ms even on large projects.

---

#### `body <symbol>`

Extract the full source body of a symbol.

```bash
$ cq body detect_project_root --in crates/codequery-core
```
```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/project.rs:33:0 function detect_project_root @@
pub fn detect_project_root(start: &Path) -> Result<PathBuf> {
    let canonical = start
        .canonicalize()
        .map_err(|e| CoreError::Path(format!("cannot canonicalize {}: {e}", start.display())))?;

    let mut current = canonical.as_path();

    loop {
        for marker in MARKERS {
            if current.join(marker).exists() {
                return Ok(current.to_path_buf());
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return Err(CoreError::ProjectNotFound(start.to_path_buf())),
        }
    }
}
```

---

#### `sig <symbol>`

Signature only — the function prototype, struct declaration, or class header without the body. Useful for fast API inspection.

```bash
$ cq sig detect_project_root --in crates/codequery-core
```
```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/project.rs:33:0 function detect_project_root @@
pub fn detect_project_root(start: &Path) -> Result<PathBuf>
```

---

#### `outline <file>`

All symbols in a file, with kinds and nesting. Works even on files with syntax errors.

```bash
$ cq outline crates/codequery-core/src/symbol.rs
```
```
@@ meta resolution=syntactic completeness=exhaustive total=7 @@

@@ crates/codequery-core/src/symbol.rs @@
  Symbol (struct, pub) :8
  SymbolKind (enum, pub) :39
  fmt::Display for SymbolKind (impl, priv) :145
    fmt (method, priv) :146
  Location (struct, pub) :203
  Visibility (enum, pub) :214
  fmt::Display for Visibility (impl, priv) :226
    fmt (method, priv) :227
  tests (module, priv) :238
```

Use `--depth` to limit nesting levels. Use `--kind function` to filter by symbol type.

---

#### `symbols`

All symbols across the entire project. Parses in parallel.

```bash
$ cq symbols --kind function --in crates/codequery-core/src/
```
```
@@ meta resolution=syntactic completeness=exhaustive total=21 @@

@@ crates/codequery-core/src/config.rs:91:0 function load_config @@
@@ crates/codequery-core/src/dirs.rs:20:0 function data_dir @@
@@ crates/codequery-core/src/dirs.rs:37:0 function cache_dir @@
...
```

---

#### `imports <file>`

All import/use/require statements in a file, across all supported languages.

```bash
$ cq imports crates/codequery-core/src/symbol.rs
```
```
@@ meta resolution=syntactic completeness=exhaustive total=2 @@

@@ crates/codequery-core/src/symbol.rs @@
  @@ crates/codequery-core/src/symbol.rs:3 use std::fmt @@
  @@ crates/codequery-core/src/symbol.rs:4 use std::path::PathBuf @@
```

---

#### `context <file:line>`

Given a file location, show the enclosing symbol. Useful when navigating from stack traces or grep output to the containing function.

```bash
$ cq context crates/codequery-core/src/symbol.rs:50
```
```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/symbol.rs:39:0 enum SymbolKind (contains line 50) @@
pub enum SymbolKind {
    // --- Core programming constructs ---
    /// A free function.
    Function,
    ...
    Trait,    // <- line 50
```

Use `--depth 2` to show multiple enclosing scopes (e.g., method inside class).

---

#### `hover <file:line[:column]>`

Type info, documentation, and signature at a source location. Uses AST analysis by default; uses the language server with `--semantic` or a running daemon.

```bash
$ cq hover src/main.rs:42:8
$ cq hover src/lib.rs:10 --semantic
```

---

#### `tree [path]`

Project file and directory tree, respecting `.gitignore` and `.cqignore`.

```bash
$ cq tree --depth 2
$ cq tree crates/codequery-core/
```

---

### Cross-Reference Commands

These commands use the three-tier precision cascade. Each result includes a `resolution` field (`syntactic`, `resolved`, or `semantic`) so consumers know the confidence level.

**Precision tiers:**

| Tier | How | Speed | Coverage |
|------|-----|-------|----------|
| `syntactic` | Tree-sitter name matching | sub-100ms | All 71 languages |
| `resolved` | Stack graph analysis | ~200ms | 10 languages |
| `semantic` | LSP daemon | sub-second (warm) / 10-30s (cold) | 40+ languages |

---

#### `refs <symbol>`

All references to a symbol across the project — call sites, imports, type uses.

```bash
$ cq refs Config
$ cq refs handle_request --semantic --context 3
```

---

#### `callers <symbol>`

Call sites only. A filtered subset of `refs`.

```bash
$ cq callers handle_request
$ cq callers send --semantic
```

---

#### `deps <symbol>`

What a function calls and uses in its body. Maps symbol relationships using import analysis and reference tracking.

```bash
$ cq deps load_config
$ cq deps Router --json
```

---

#### `callchain <symbol>`

Multi-level call hierarchy. Who calls the target, who calls those callers, and so on. Use `--depth` to control traversal depth (default: all).

```bash
$ cq callchain load_config
```
```
@@ meta resolution=syntactic completeness=best_effort total=16 note="recursive caller analysis; may miss indirect calls" @@

load_config (function) crates/codequery-core/src/config.rs:91
  ← test_load_config_returns_none_when_no_file (function) crates/codequery-core/src/config.rs:149
  ← test_load_config_parses_full_config (function) crates/codequery-core/src/config.rs:170
  ...
```

```bash
$ cq callchain process --depth 5
```

---

#### `hierarchy <type>`

Type hierarchy for a type — what it extends/implements and what extends/implements it.

```bash
$ cq hierarchy SymbolKind
```
```
@@ meta resolution=syntactic completeness=best_effort total=2 @@

@@ SymbolKind (enum) crates/codequery-core/src/symbol.rs:39 @@

Supertypes:
  ↑ fmt::Display (trait) crates/codequery-core/src/symbol.rs:145
```

```bash
$ cq hierarchy Animal --lang typescript
$ cq hierarchy Iterator --semantic
```

---

### Analysis and Refactoring

#### `dead`

Find unreferenced symbols. Private symbols that never appear as a reference are reported. Public symbols are flagged with a warning since they may have external callers.

```bash
$ cq dead
$ cq dead --kind function
$ cq dead --in src/legacy/
```

---

#### `diagnostics [file]`

Syntax errors and language server diagnostics. Always shows tree-sitter parse errors. With `--semantic` or a running daemon, also shows language server diagnostics.

```bash
$ cq diagnostics src/main.rs
$ cq diagnostics                    # whole project
$ cq diagnostics --in src/
```

---

#### `rename <old> <new>`

Rename a symbol across the project. Behavior depends on precision:
- At `semantic` or `resolved` precision: applies immediately.
- At `syntactic` precision: shows a diff preview by default; use `--apply` to force write.
- `--dry-run` always shows a preview without writing.

```bash
$ cq rename OldName NewName
$ cq rename foo bar --apply
$ cq rename Handler Router --dry-run
```

---

#### `search <pattern>`

Structural search using tree-sitter S-expression queries. Matches against the AST — more precise than text search because it understands code structure.

S-expressions match tree-sitter node types. Use `cq tree <file>` to explore the node types for a language.

```bash
# Find all Rust functions
$ cq search '(function_item name: (identifier) @name)' --lang rust

# Find a specific function by name
$ cq search '(function_item name: (identifier) @name (#eq? @name "main"))'

# Find TypeScript class declarations
$ cq search '(class_declaration name: (identifier) @name)' --lang typescript
```
```
@@ meta resolution=syntactic completeness=exhaustive total=141 @@

@@ crates/codequery-core/src/config.rs:90:7 @@
load_config

@@ crates/codequery-core/src/dirs.rs:20:7 @@
data_dir
...
```

Captures (`@name`) control which part of the match is returned. Use `--raw` for content without framing.

---

### Management Commands

#### `grammar`

Manage language grammar packages. All 71 languages are WASM plugins that auto-install on first use or can be pre-installed.

| Subcommand | Description |
|------------|-------------|
| `grammar list` | Show installed and available grammars |
| `grammar install <lang>` | Install a grammar package |
| `grammar update` | Update all installed packages |
| `grammar remove <lang>` | Remove an installed package |
| `grammar info <lang>` | Show details for a language |
| `grammar validate <lang>` | Validate extract.toml queries against the grammar |

```bash
$ cq grammar list
$ cq grammar install elixir
$ cq grammar install --all
$ cq grammar info elixir
$ cq grammar validate rust
```

---

#### `daemon`

Manage the background LSP daemon. A running daemon eliminates cold-start overhead for `--semantic` queries.

| Subcommand | Description |
|------------|-------------|
| `daemon start` | Start daemon in background |
| `daemon stop` | Stop a running daemon |
| `daemon status` | Show running servers and their state |

```bash
$ cq daemon start
$ cq daemon status
$ cq daemon stop
```

---

#### `cache`

Manage the disk cache (`.cq-cache/` in the project root). Caching is opt-in via `--cache` or `CQ_CACHE=1`.

| Subcommand | Description |
|------------|-------------|
| `cache clear` | Delete all cached data |

```bash
$ cq cache clear
```

---

#### `upgrade`

Check GitHub releases for a newer version and print upgrade instructions. Does not self-update.

```bash
$ cq upgrade
```

---

## Output Modes

### Framed (default)

Human and machine readable. Every result has an `@@ header @@` and source code passes through unescaped.

```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/symbol.rs:39:0 enum SymbolKind @@
pub enum SymbolKind {
    Function,
    Method,
    ...
}
```

**Meta header fields:**

| Field | Values | Meaning |
|-------|--------|---------|
| `resolution` | `syntactic`, `resolved`, `semantic` | Precision tier used |
| `completeness` | `exhaustive`, `best_effort` | Whether all results are guaranteed to be found |
| `total` | integer | Number of results |
| `note` | string (optional) | Explanation for best_effort results |

**Result header format:** `@@ <file>:<line>:<col> <kind> <name> @@`

Commands that return pure source (`body`, `sig`) or structure (`outline`, `callchain`) use consistent variants of this format.

---

### JSON (`--json`)

Structured output with full metadata. Recommended for programmatic use.

```bash
$ cq def SymbolKind --json
```
```json
{
  "resolution": "syntactic",
  "completeness": "exhaustive",
  "symbol": "SymbolKind",
  "definitions": [
    {
      "name": "SymbolKind",
      "kind": "enum",
      "file": "crates/codequery-core/src/symbol.rs",
      "line": 39,
      "column": 0,
      "end_line": 143,
      "visibility": "pub",
      "doc": "/// The kind of a source code symbol.",
      "body": "pub enum SymbolKind { ... }",
      "signature": "pub enum SymbolKind { ... }"
    }
  ],
  "total": 1
}
```

Use `--pretty` to force indented JSON when piping (terminal output is pretty-printed by default).

---

### Raw (`--raw`)

Content only. No framing, no metadata. Pipe-friendly.

```bash
$ cq body handle_request --raw | wc -l
$ cq search '(function_item) @f' --raw | grep "pub fn"
```

---

## Global Flags

These flags apply to all commands.

| Flag | Description |
|------|-------------|
| `--json` | Structured JSON output |
| `--raw` | Content only, no framing |
| `--pretty` | Force indented JSON (default when terminal) |
| `--in <path>` | Restrict file discovery to a subdirectory or file |
| `--kind <kind>` | Filter by symbol kind (`function`, `struct`, `class`, `trait`, `interface`, `method`, `enum`, `constant`, `variable`, `type_alias`, `module`) |
| `--lang <lang>` | Override language detection for target files |
| `--semantic` | Enable LSP-backed resolution |
| `--no-semantic` | Disable LSP even if daemon is running or `CQ_SEMANTIC=1` is set |
| `--project <path>` | Explicit project root (overrides auto-detection) |
| `--cache` | Enable disk caching |
| `--no-cache` | Disable caching (overrides `CQ_CACHE=1`) |
| `--context <N>` | Show N lines of surrounding context around each match |
| `--depth <N>` | Limit nesting depth (`tree`, `outline`, `callchain`) |
| `--limit <N>` | Cap number of results |
| `--dry-run` | Preview mode for `rename` — show diff without applying |
| `--apply` | Force apply for `rename` at any precision tier |

**Environment variables:**

| Variable | Equivalent flag |
|----------|----------------|
| `CQ_SEMANTIC=1` | `--semantic` |
| `CQ_CACHE=1` | `--cache` |

---

## MCP Server Setup

`cq-mcp` exposes all commands as AI-callable tools over JSON-RPC stdio. Works with Claude Code, Cursor, and any MCP-compatible client.

### Claude Code / Cursor / Claude Desktop

Add to your MCP config (`.claude/settings.json`, `cursor.json`, or `claude_desktop_config.json`):

**npm (recommended — no Rust toolchain needed):**
```json
{
  "mcpServers": {
    "cq": { "command": "npx", "args": ["-y", "@codequery/mcp"] }
  }
}
```

**pip:**
```json
{
  "mcpServers": {
    "cq": { "command": "uvx", "args": ["codequery-mcp"] }
  }
}
```

**Direct binary** (if `cq-mcp` is on PATH):
```json
{
  "mcpServers": {
    "cq": { "command": "cq-mcp" }
  }
}
```

### Available MCP Tools

| Tool | Maps to |
|------|---------|
| `cq_def` | `cq def` |
| `cq_body` | `cq body` |
| `cq_sig` | `cq sig` |
| `cq_refs` | `cq refs` |
| `cq_callers` | `cq callers` |
| `cq_deps` | `cq deps` |
| `cq_outline` | `cq outline` |
| `cq_symbols` | `cq symbols` |
| `cq_imports` | `cq imports` |
| `cq_search` | `cq search` |
| `cq_context` | `cq context` |
| `cq_tree` | `cq tree` |

Every tool accepts a `project` argument (defaults to cwd). The MCP server runs with `--json --semantic --no-cache` on every call — results are always fresh and at the best available precision.

### Installing the MCP binary directly

```bash
cargo install codequery-mcp
# or download from https://github.com/jmfirth/codequery/releases
```

---

## Daemon Mode

The daemon keeps language servers warm so `--semantic` queries don't pay cold-start costs.

```bash
# Start (runs in background, persists across terminal sessions)
cq daemon start

# Check what's running
cq daemon status

# Stop
cq daemon stop
```

When the daemon is running, any `--semantic` query reuses an already-initialized language server. Cold-start: 10-30 seconds. Warm: sub-second.

**Automatic upgrade:** When the daemon is running, all commands that support the precision cascade automatically use semantic precision without the `--semantic` flag. The cascade is:

```
daemon running?          → semantic  (sub-second, compiler-level)
--semantic flag (no daemon) → semantic  (cold-start: 10-30s)
stack graph rules?       → resolved  (import-aware, ~200ms)
fallback                 → syntactic (tree-sitter, sub-100ms)
```

Use `--no-semantic` to force syntactic even when the daemon is running.

---

## Grammar Management

71 languages are supported. All are WASM grammar packages stored in `~/.local/share/cq/languages/` and auto-install on first use.

### Auto-install

Languages download automatically the first time you query a file of that type:

```bash
# First use: downloads Elixir grammar (~2 seconds)
$ cq outline hello.ex
Downloading elixir grammar... done.

@@ meta resolution=syntactic completeness=exhaustive total=3 @@

@@ hello.ex @@
  greet (function, pub) :2
  ...
```

### Pre-installing

```bash
# Install a specific language
cq grammar install elixir

# Install everything
cq grammar install --all
```

### Inspecting

```bash
# Show what's installed and what's available
cq grammar list

# Show details for a language
cq grammar info elixir
# Language:     Elixir
# Extensions:   .ex, .exs
# Capabilities: grammar, extract, lsp
# LSP server:   elixir-ls
# Status:       installed

# Validate that a grammar's extract queries compile
cq grammar validate rust
```

---

## Configuration

### `.cq.toml`

Optional per-project configuration. Place at the project root.

```toml
[project]
# Additional glob patterns to exclude from file discovery
exclude = ["vendor/", "generated/", "**/*.pb.go"]

# Enable caching by default for this project
cache = true

[languages]
# Map non-standard extensions to languages
".mdx" = "markdown"
".mjx" = "javascript"

[lsp]
# Idle timeout before shutting down a language server (minutes)
timeout = 10

[lsp.rust]
# Override the language server binary
binary = "rust-analyzer"
args = []

[lsp.python]
binary = "pyright-langserver"
args = ["--stdio"]
```

All fields are optional. `cq` works with no configuration at all.

### `.cqignore`

gitignore-format patterns to exclude from file discovery. Same syntax as `.gitignore`. Place at the project root.

```
# .cqignore
vendor/
node_modules/
*.generated.ts
dist/
```

`.cqignore` patterns are additive with `.gitignore` — both are respected automatically.

### Project root detection

`cq` walks up from the current directory looking for:

- VCS roots: `.git`, `.hg`
- Language markers: `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `pom.xml`, `build.gradle`, `*.sln`

Use `--project <path>` to override.

---

## Scoping

Two flags narrow where `cq` looks. Useful for large monorepos or when you want results from a specific component.

### `--in <path>`

Restrict file discovery to a subdirectory or single file. Accepts relative or absolute paths.

```bash
# Narrow refs to a specific package
cq refs Config --in crates/codequery-core/

# Search only in a single file
cq def parse --in src/parser.rs

# Combine with other flags
cq symbols --kind struct --in crates/codequery-parse/
```

### `--project <path>`

Set the project root explicitly. Overrides auto-detection. Useful when running `cq` from outside the project or from a script.

```bash
cq outline src/main.rs --project /path/to/myproject
cq symbols --project ~/work/api-server/
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Results found |
| `1` | No results (valid query, nothing matched) |
| `2` | Usage error (bad arguments) |
| `3` | Project error (no project root found) |
| `4` | Parse error (reported as warnings; does not block results from other files) |

---

## Agent Harness Tips

- Framed output (the default) is best for agents — source code passes through unescaped with minimal token overhead. The `@@ meta @@` header carries `resolution` and `completeness` for trust calibration. Use `--json` only when you need structured metadata for filtering or aggregation.
- `cq body` instead of reading files. One function, minimal tokens, no surrounding noise.
- `cq context file:line` to navigate from error messages or stack traces to the containing function.
- `cq def` + `cq refs` for impact analysis before making changes.
- `cq outline` to orient in unfamiliar files; `cq tree` for directory-level orientation.
- `cq sig` for API contracts without implementation bodies.
- Use qualified names to disambiguate: `cq body Router::add_route`.
- `--in` to scope searches on large codebases and avoid noise.
- Pipe `--raw` output into other tools: `cq body fn_name --raw | wc -l`.
- `cq callchain` to understand the blast radius of changing a function.
- `cq dead --kind function` before refactoring to identify removable code.
- Include `llms.txt` from the project root in the system prompt for a compact reference.
