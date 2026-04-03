# codequery Architecture

This document is for contributors and the deeply curious. It covers the full system: crate responsibilities, the query pipeline, precision tiers, the grammar plugin system, daemon architecture, output format, and how the test suite is structured.

---

## Overview

`cq` is a semantic code query tool. It takes source code as input — a project, a file, a symbol name — and returns structured information about it: where a symbol is defined, what calls it, what it imports, what its signature is. It operates on real source trees, not grep output.

The primary abstraction is the tree-sitter parse tree. Every file that passes through `cq` is parsed into an AST. Symbol extraction, search, and reference analysis all operate on that AST. For cross-reference commands, a second tier of analysis is available: stack graph resolution, which follows imports and qualified names across files. A third tier, LSP integration, reaches compiler-level precision when a language server is running.

The system has five non-negotiable constraints:

1. **Stateless by default.** Every invocation parses what it needs. Caching is opt-in via `--cache`. The daemon is optional and the system falls back gracefully when it is not running.
2. **Error-tolerant.** Tree-sitter produces usable ASTs even on broken code. A parse error in one file must never block results from other files.
3. **Cross-language from one binary.** All 71 languages are available via WASM grammar packages that install on first use. The binary ships at ~7.9MB with no compiled-in grammars.
4. **Human-readable default output.** Framed plain text with `@@ file:line:column kind name @@` delimiters. JSON and raw modes are available via flags.
5. **Performance contract.** Narrow commands (`def`, `body`, `sig`) under 100ms on any project size. Wide commands (`refs`, `callers`, `symbols`) under 2s on 400k lines with 8 cores.

---

## Crate Structure

The project is organized as a Cargo workspace with 7 crates. The dependency graph is acyclic, with `codequery-core` at the bottom and `codequery-cli` / `codequery-mcp` at the top.

```
codequery-cli ──────────────────────────────────┐
codequery-mcp ──────────────────────────────────┤
                                                 ▼
codequery-lsp ──────────────────────────────────┐
                                                 ▼
codequery-resolve ──────────────────────────────┐
                                                 ▼
codequery-index ────────────────────────────────┐
                                                 ▼
codequery-parse ────────────────────────────────┐
                                                 ▼
codequery-core ◄────────────────────────────────┘
```

### codequery-core

The shared data layer. No parsing, no output logic. Everything else depends on it.

Key modules:
- `symbol.rs` — `Symbol`, `SymbolKind`, `Visibility`, `Location`. The canonical representation of an extracted source entity.
- `discovery.rs` — `Language` enum (22 variants, Tier 1 through structured data), `language_for_file`, `discover_files`. File walking uses the `ignore` crate for `.gitignore`-aware traversal, plus a custom `.cqignore` filename. The registry JSON (baked in at compile time from `languages/registry.json`) provides the extension map for all 71 known languages.
- `query.rs` — `Resolution` (`Syntactic`, `Resolved`, `Semantic`), `Completeness` (`Exhaustive`, `BestEffort`), `QueryResult<T>`. These metadata types wrap every command result.
- `config.rs` — `ProjectConfig` loaded from `.cq.toml`. Controls exclude patterns, language extension overrides, cache defaults, and LSP server overrides.
- `extract_config.rs` — `ExtractConfig` and `SymbolRule`. Schema for `extract.toml` files used by plugin languages.
- `project.rs` — `detect_project_root`. Walks up from cwd looking for VCS roots (`.git`, `.hg`) and language markers (`Cargo.toml`, `package.json`, `go.mod`, etc.).

### codequery-parse

Tree-sitter parsing and symbol extraction for all compiled-in languages, plus the runtime grammar loading infrastructure.

Key modules:
- `parser.rs` — `Parser` struct. The primary entry point. `Parser::for_language(lang)` tries the compiled grammar first, then native runtime (`.so`/`.dylib`), then WASM. `Parser::for_name(name)` adds a builtin lookup before the fallback path. Auto-install is triggered when a language is in the registry but no grammar is installed: `cq grammar install <name>` is invoked transparently on first use, with a per-process deduplication guard.
- `extract.rs` — `extract_symbols` and `extract_symbols_by_name`. Dispatch to the appropriate per-language extractor, or to `extract_with_config` for plugin languages.
- `languages/` — 21 per-language modules (`rust.rs`, `python.rs`, `typescript.rs`, etc.), each implementing `LanguageExtractor`. These are hand-written tree-sitter query extractors for the compiled-in languages.
- `extract_engine.rs` — `CompiledExtractor`. The declarative extraction engine that interprets `extract.toml` rules at runtime. Tree-sitter queries are compiled once per process and cached in a `LazyLock<Mutex<HashMap>>`.
- `runtime_grammar.rs` — `load_runtime_grammar`. Loads `.so`/`.dylib` grammar files from `$XDG_DATA_HOME/cq/grammars/` via `libloading`. Unsafe surface is minimal and annotated.
- `wasm_loader.rs` — `find_wasm_grammar`, `load_wasm_language_cached`. Loads `.wasm` grammar packages from `~/.local/share/cq/languages/<name>/grammar.wasm`. Handles function name resolution (e.g., `common-lisp` → `commonlisp`) via an explicit `wasm_name` file or hyphen-to-underscore transformation.
- `search.rs` — `search_file`. Structural search via tree-sitter S-expression queries.
- `diagnostics.rs` — `extract_syntax_errors`. Walks the parse tree looking for `ERROR` nodes.
- `imports.rs` — `extract_imports`. Per-language import extraction.
- `hierarchy.rs` — `extract_supertype_relations`. Extracts class/trait/interface inheritance for type hierarchy commands.

### codequery-index

Parallel scanning and the symbol index. This is the engine behind wide commands.

Key modules:
- `scanner.rs` — `scan_project`, `scan_with_filter`, `FileSymbols`. Discovers all source files, parses them in parallel with `rayon`, and returns `Vec<FileSymbols>`. Each `FileSymbols` retains the `tree_sitter::Tree` to avoid re-parsing in downstream phases. The scanner tries builtin language detection first; if that fails it falls back to registry name lookup and `Parser::for_name`.
- `grep.rs` — `file_contains_word`, `filter_files`. Text pre-filter using `memmap2` for zero-copy file reads and `memchr::memmem` for fast word-boundary search. Files below 32KB are read directly; larger files are memory-mapped. This narrows the candidate set before expensive tree-sitter parsing.
- `index.rs` — `SymbolIndex`. In-memory index of all symbols keyed by name and file, built from scan results.
- `refs.rs` — `extract_references`. Cross-file reference extraction from parse trees.
- `cache.rs` — `CacheStore`, `CachedFile`. Disk cache for file scan results, keyed by file path, mtime, and size. Serialized with bincode. Opt-in only.

### codequery-resolve

Stack graph resolution. Transforms tree-sitter parse trees into scope graphs and answers name-binding queries.

Key modules:
- `rules.rs` — Per-language `StackGraphLanguage` factory. `has_rules(lang)` returns true for: Python, TypeScript, JavaScript, Java, Go, C, C++, Rust, Ruby, C#. The TSG rules themselves live in `tsg/<language>/stack-graphs.tsg`.
- `graph.rs` — `build_graph`, `build_graph_with_timeout`. Constructs a `StackGraph` from a parse tree using the language's TSG rules.
- `resolve.rs` — `resolve_references`, `resolve_references_with_timeout`. Queries the graph for name bindings.
- `resolver.rs` — `StackGraphResolver`. Facade that orchestrates multi-file graph construction and resolution for a given symbol.
- `types.rs` — `ResolutionResult`, `ResolvedReference`. The typed output of stack graph resolution.

### codequery-lsp

LSP client, JSON-RPC transport, server lifecycle, and the background daemon.

Key modules:
- `server.rs` — `LspServer`. Spawns a language server process, performs the LSP initialize handshake (including capabilities negotiation), and manages shutdown. Communicates via stdio using `StdioTransport`.
- `transport.rs` — `StdioTransport`. JSON-RPC message framing (Content-Length headers) over stdio.
- `protocol.rs` — `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcNotification`, `JsonRpcError`. Minimal JSON-RPC types for what cq actually uses.
- `client.rs` — `DaemonClient`. Connects to a running daemon over TCP, sends an `Authenticate` request, then dispatches `Query` requests.
- `daemon.rs` — `Daemon`. The background daemon process. Binds a TCP socket on `127.0.0.1:0` (OS-assigned port), writes a daemon info file, and runs a synchronous accept loop. Maintains a pool of `LspServer` instances keyed by `(project_root, language)`. Idle servers are evicted after a configurable timeout (default 30 minutes, overridden by `CQ_LSP_TIMEOUT`). No async runtime — plain `std::net::TcpListener` with non-blocking mode and a 100ms sleep between accept attempts.
- `daemon_file.rs` — `DaemonInfo`. The on-disk record for a running daemon: `{ port, token, pid, project, started }`. Written to `~/.cache/cq/daemons/<project-hash>.json` where the hash is a 16-character hex string derived from the canonical project path. `is_daemon_running` probes liveness by attempting a TCP connect with a 500ms timeout.
- `socket.rs` — `DaemonRequest`, `DaemonResponse`. The message protocol between client and daemon. Messages are length-prefixed JSON: 4-byte big-endian u32 length followed by the JSON payload.
- `cascade.rs` — `resolve_with_cascade`. The four-step resolution strategy (see Precision Cascade section).
- `oneshot.rs` — `semantic_definition`, `semantic_refs`. Start a language server, run one query, shut down. The slow-but-correct path when `--semantic` is passed and no daemon is running.
- `config.rs` — `LanguageServerRegistry`, `ServerConfig`. Default LSP server binaries and arguments for all 22 languages.
- `queries.rs` — `path_to_uri`, `uri_to_path`. LSP URI ↔ filesystem path conversion.

### codequery-cli

The `cq` binary. 24 commands, argument parsing, output formatting.

Key modules:
- `main.rs` — Entry point. Parses global args, dispatches to command modules.
- `args.rs` — `CqArgs`, `OutputMode` (`Framed`, `Json`, `Raw`). All global flags: `--project`, `--in` (scope), `--json`, `--raw`, `--pretty`, `--kind`, `--lang`, `--semantic`, `--cache`.
- `commands/` — One module per command: `def`, `body`, `sig`, `refs`, `callers`, `outline`, `symbols`, `imports`, `search`, `context`, `tree`, `deps`, `hover`, `diagnostics`, `rename`, `dead`, `callchain`, `hierarchy`, `daemon`, `grammar`, `upgrade`. Plus `common.rs` for shared utilities.
- `output.rs` — All formatting logic. Converts typed symbol data from `codequery-core` into framed text, JSON, or raw output. No I/O in this module — pure string construction. JSON output is wrapped in `QueryResult<T>` which carries `resolution` and `completeness` metadata alongside the payload.

### codequery-mcp

The `cq-mcp` binary. Implements the Model Context Protocol so AI agents can use `cq` as a tool provider.

Key modules:
- `main.rs` — Entry point. Runs the MCP stdio server.
- `protocol.rs` — MCP protocol types: `ToolDefinition`, `ToolCallResult`, `ContentItem`.
- `tools.rs` — 18 tool definitions and dispatch. Each tool shells out to the `cq` binary with appropriate arguments and returns the output. The tools are: `cq_def`, `cq_body`, `cq_sig`, `cq_refs`, `cq_callers`, `cq_deps`, `cq_outline`, `cq_imports`, `cq_symbols`, `cq_search`, `cq_context`, `cq_tree`, `cq_hover`, `cq_diagnostics`, `cq_rename`, `cq_dead`, `cq_callchain`, `cq_hierarchy`.

---

## Query Pipeline

Every command follows the same basic pipeline. The variation is in which stages are traversed and in what order.

```
file discovery
     │
     ▼
language detection
     │
     ▼
[optional: text pre-filter (memchr word-boundary search)]
     │
     ▼
tree-sitter parse (→ AST)
     │
     ▼
symbol extraction (→ Vec<Symbol>)
     │
     ▼
[optional: index build / reference extraction / stack graph resolution]
     │
     ▼
output formatting
```

**Narrow commands** (`def`, `body`, `sig`, `outline`, `imports`, `context`): text pre-filter → candidate files only → parse subset → extract. The pre-filter uses `memchr::memmem` with word-boundary checking to avoid parsing files that cannot possibly contain the target symbol.

**Wide commands** (`refs`, `callers`, `symbols`, `tree`, `dead`): parallel scan all files with `rayon` → build symbol index → query → merge results. The `FileSymbols` struct retains the `tree_sitter::Tree` across pipeline stages to avoid re-parsing.

**Cross-reference commands** (`refs`, `callers`, `deps`) additionally invoke the precision cascade after the scan phase.

---

## Precision Cascade

Cross-reference commands attempt resolution at the highest available precision tier, falling back gracefully. Implemented in `codequery-lsp/src/cascade.rs` as `resolve_with_cascade`.

```
1. Daemon running?      → semantic (compiler-level, sub-50ms)
2. --semantic flag set? → oneshot LSP (2-5s startup, then compiler-level)
3. Stack graph rules?   → resolved (scope-aware, follows imports)
4. Fallback             → syntactic (tree-sitter name matching)
```

Step 1 checks `is_daemon_running` by reading the daemon info file and probing the TCP port (500ms timeout). If the daemon is reachable, `DaemonClient::connect` authenticates with the stored token and queries for LSP locations.

Step 2 is only entered if `semantic_requested` is true (the `--semantic` flag). It starts a fresh language server, waits for it to index the project (via `$/progress` readiness detection), runs the query, and shuts down.

Step 3 uses `StackGraphResolver`, which is always available regardless of whether any language server is installed. It builds stack graphs from the scan results and resolves name bindings.

Step 4 is the baseline. If steps 1 and 2 error (daemon not running, no server installed, etc.), the code falls through to step 3. Step 3 itself returns either `Resolved` or `Syntactic` precision depending on whether the language has TSG rules.

Every result carries `Resolution` metadata so consumers know what they got:
- `Semantic` — came from a language server (steps 1 or 2)
- `Resolved` — came from stack graphs (step 3, language has TSG rules)
- `Syntactic` — tree-sitter name matching only (step 3 fallback, or step 4)

---

## Grammar Plugin System

All 71 languages are delivered as WASM plugins. No grammars are compiled into the binary. The plugin system provides all language support without recompiling the binary.

### registry.json

`languages/registry.json` is baked into the binary at compile time via `include_str!`. It maps 71 language names to their file extensions. The extension map is built once on first access and cached in a `OnceLock<HashMap<String, String>>`. `language_name_for_file` consults this map for any file extension not handled by the compiled-in `language_for_file` match.

### Grammar resolution order

`Parser::for_language` and `Parser::for_name` follow a two-step resolution:

1. **Native runtime grammar** — `load_runtime_grammar(name)` looks for `tree-sitter-<name>.<dylib|so|dll>` in `$XDG_DATA_HOME/cq/grammars/` and loads it via `libloading` (unsafe, annotated). This is an optional local override path.
2. **WASM grammar** — `find_wasm_grammar(name)` looks for `~/.local/share/cq/languages/<name>/grammar.wasm`. If found, `load_wasm_language_cached` loads it. The WASM function name may differ from the language name; resolution checks a `wasm_name` file in the package directory, then tries hyphen-to-underscore transformation.

**Auto-install**: if none of the above succeeds and the language is in the registry, the parser triggers an auto-install. It shells out to `curl` to download `https://github.com/jmfirth/codequery/releases/download/v<version>/lang-<name>.tar.gz`, extracts it to `~/.local/share/cq/languages/<name>/`, and retries WASM loading. A `Mutex<Vec<String>>` tracks which languages have been attempted this process to prevent repeated download attempts.

### extract.toml

Plugin languages that want symbol extraction (not just parsing) ship an `extract.toml` alongside their `grammar.wasm`. This file defines extraction rules declaratively:

```toml
[language]
name = "elixir"
extensions = [".ex", ".exs"]

[[symbols]]
kind = "function"
query = "(def (call target: (identifier) @name)) @body"
name = "@name"
body = "@body"
doc = "preceding_comment"
```

`CompiledExtractor` compiles these tree-sitter queries once per process and caches them. Rules with malformed queries are logged and skipped — a bad rule does not prevent other rules from working.

---

## Language Support Architecture

All 71 languages follow the same runtime plugin path. There are no compiled-in grammars.

### Plugin languages (the universal path)

All languages are looked up via `language_name_for_file`, which consults the registry JSON baked into the binary at compile time. Language grammars are loaded at runtime via WASM:

- `language_name_for_file` returns the language name string from the registry JSON.
- `Parser::for_name(name)` loads the grammar via native runtime or WASM (with auto-install).
- `extract_symbols_by_name(source, tree, file, name)` looks for an `extract.toml` in the installed package directory and runs it through `CompiledExtractor`.

Languages with hand-written extractors in `codequery-parse/src/languages/` are organized into tiers for test coverage purposes:

- **Tier 1** (8): Rust, TypeScript, JavaScript, Python, Go, C, C++, Java
- **Tier 2** (9): Ruby, PHP, C#, Swift, Kotlin, Scala, Zig, Lua, Bash
- **Structured data** (5): HTML, CSS, JSON, YAML, TOML

Stack graph TSG rules exist for all Tier 1 languages plus Ruby and C#. All other languages fall back to `Syntactic` precision for cross-reference commands.

---

## Daemon Architecture

The daemon keeps language servers warm between `cq` invocations to eliminate startup cost.

### Lifecycle

On `cq daemon start`:
1. `Daemon::new` initializes the server pool and generates a 32-character hex authentication token (time + PID based, not cryptographically secure, sufficient for localhost-only use).
2. `Daemon::run` binds `TcpListener::bind("127.0.0.1:0")` — port 0 means the OS picks an available port.
3. A `DaemonInfo` struct `{ port, token, pid, project, started }` is written as JSON to `~/.cache/cq/daemons/<project-hash>.json`.
4. The daemon enters the synchronous accept loop.

### Server pool

The pool is a `HashMap<(PathBuf, Language), PooledServer>`. On a `Query` request:
- The daemon checks if a server for `(project, language)` is already running.
- If not, it starts one via `LspServer::start` using the configured binary for that language.
- The server is reused for subsequent queries; `last_used` is updated on each use.
- Between connections, `evict_idle_servers` scans the pool and shuts down servers idle longer than the configured timeout.

### Token authentication

Every new TCP connection must send an `Authenticate { token }` message as the first request. The daemon compares against its stored token. Connections that fail authentication receive an `Error` response and are closed.

### Signal handling

The daemon registers handlers for `SIGTERM` and `SIGINT` (via an `AtomicBool` shared with the accept loop). On signal, the loop exits cleanly, `shutdown_all_servers` sends `shutdown`/`exit` notifications to all pooled servers, and the daemon info file is removed.

### Liveness probing

`is_daemon_running(project_root)` reads the daemon info file and attempts `TcpStream::connect_timeout` to `127.0.0.1:<port>` with a 500ms timeout. This handles the case where a daemon info file exists but the process has died.

---

## Output Format

The default output format is framed text. This is not a concession to humans at the expense of machines — it is specifically designed for LLM token efficiency. JSON wrapping every result in structural overhead costs tokens without adding information. The framed format is grep-friendly, parseable, and token-sparse.

### Framed format

Every result set begins with a meta header:

```
@@ meta resolution=syntactic completeness=exhaustive total=3 @@
```

Each result is a frame header, optionally followed by content:

```
@@ src/server.rs:42:4 function handle_request @@
pub fn handle_request(req: Request) -> Response {
    ...
}
```

The format of a frame header: `@@ <file>:<line>:<column> <kind> <name> @@`

Multiple results are separated by blank lines.

### Raw format

`--raw` removes the `@@` delimiters. The meta line becomes a `#` comment:

```
# meta resolution=syntactic completeness=exhaustive total=3
src/server.rs:42:4 function handle_request
```

Useful for piping into other tools that expect `file:line:column` format.

### JSON format

`--json` emits a `QueryResult<T>` structure. The metadata fields (`resolution`, `completeness`, `note`) are at the top level; the data fields are flattened alongside them:

```json
{
  "resolution": "syntactic",
  "completeness": "exhaustive",
  "total": 1,
  "definitions": [
    {
      "name": "handle_request",
      "kind": "function",
      "file": "src/server.rs",
      "line": 42,
      "column": 4,
      "end_line": 58,
      "visibility": "public"
    }
  ]
}
```

`--pretty` enables indented formatting (default when stdout is a TTY). Compact JSON is the default when piping. The MCP server uses JSON output internally when shelling out to `cq`.

---

## Testing Model

The test suite has five distinct levels, reflecting the different failure modes at each layer.

### Unit tests

`#[cfg(test)]` modules inside each crate. Test internal API correctness: parser construction, symbol extraction from known source snippets, registry lookups, cache serialization. Do not require external binaries or network access.

### Integration tests

`crates/codequery-cli/tests/`. Each test file covers a command or a scenario:

| File | Coverage |
|------|----------|
| `test_def.rs` | `def` command against fixture projects |
| `test_outline.rs` | `outline` command, nesting, visibility |
| `test_cross_language.rs` | Same commands across multiple languages |
| `test_resolution.rs` | Resolution tier metadata on results |
| `test_global.rs` | Global flags, output modes |
| `test_real_usage.rs` | End-to-end scenarios against real-ish projects |
| `test_phase3.rs` | Stack graph resolution integration |
| `test_tier2.rs` | Tier 2 language commands |

Tests use temporary directories for isolation. Fixture projects live in `tests/fixtures/`.

### Cross-language coverage

`test_coverage_tier1.rs` and `test_coverage_tier2.rs` run the same set of commands against all Tier 1 and Tier 2 languages respectively, verifying that no language silently returns empty results for basic commands. Tests for languages whose grammars are not installed are skipped with an informative message rather than failing.

### Precision proof and LSP comparison

`test_proof.rs` verifies that stack graph resolution produces `Resolved` results (not `Syntactic`) for supported languages against known fixtures.

`test_precision.rs` compares stack graph results against LSP ground truth on real codebases, measuring how often the two agree. Requires language servers to be installed; skipped otherwise.

`test_lsp.rs` and `test_lsp_live.rs` test the LSP client and daemon communication directly.

### Stack graph strict

`test_stack_graph_strict.rs` asserts exact resolution tiers for each language — which commands should produce `Resolved` vs. `Syntactic`. This is the guard against regressions in TSG rules.

### Validation scripts

Outside the Rust test suite:
- `scripts/smoke-test.sh` — clones 15+ real open-source projects (ripgrep, Django, Flask, Express, etc.) and runs `cq` commands against them, checking for panics and zero-result regressions.
- `scripts/lsp-validation.sh` — compares stack graph output against LSP output for sampled symbol locations in real projects, computing agreement rates by language.
