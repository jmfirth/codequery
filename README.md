# cq

The `jq` for source code.

---

`cq` is a semantic code query tool for the command line. One binary, 16 languages, zero setup. It answers structural questions about code: where is this symbol defined, what does it look like, who calls it, what does it depend on.

There is a gap between `grep` and language servers. Grep is fast but semantically blind. Language servers are precise but heavy — they need toolchains, indexing time, and compilable code. `cq` bridges the gap with three tiers of precision that activate automatically:

- **Tree-sitter** -- instant, works on broken code, knows definitions from references from strings. Always available.
- **Stack graphs** -- scope-resolved name binding. Follows imports, qualified names, and cross-file references. Available for 10 languages, no setup required.
- **Language servers** -- compiler-level precision via LSP. Resolves generics, trait dispatch, type inference. Available for all 16 languages when a language server is installed. Optional `cq daemon` keeps servers warm for sub-second queries.

The same command produces the same output format at every tier. Only the `resolution` metadata field changes — from `syntactic` to `resolved` to `semantic`. The cascade runs automatically: you get the best precision available without configuring anything.

**Built for AI agents.** `cq body handle_request` returns 5 lines instead of reading a 500-line file. Every response includes precision metadata so agents know how much to trust results. Integrate via two paths:

- **CLI + llms.txt** -- any agent with shell access can call `cq` directly. The included [`llms.txt`](llms.txt) teaches agents the full command surface, output formats, and efficient usage patterns.
- **MCP server** -- `cq-mcp` exposes all 12 commands as native tools for Claude, Cursor, and any MCP-compatible AI tool. Auto-starts a language server daemon for compiler-level precision.

---

## Quick Demo

Find a definition, then extract its full body:

```
$ cq def authenticate
@@ src/auth/mod.rs:23:4 function authenticate @@

$ cq body authenticate
@@ src/auth/mod.rs:23:4 function authenticate @@
pub async fn authenticate(req: &Request) -> Result<AuthContext> {
    let token = extract_token(req)?;
    let claims = verify_jwt(&token).await?;
    AuthContext::from_claims(claims)
}
```

Find all references -- and see exactly how they were resolved:

```
$ cq refs authenticate --json
{
  "symbol": "authenticate",
  "resolution": "resolved",
  "completeness": "best_effort",
  "definitions": [
    { "file": "src/auth/mod.rs", "line": 23, "kind": "function" }
  ],
  "references": [
    { "file": "src/api/routes.rs", "line": 44, "kind": "call", "context": "    let auth = authenticate(&req).await?;" },
    { "file": "src/ws/handler.rs", "line": 18, "kind": "call", "context": "    if authenticate(&conn).await.is_err() {" },
    { "file": "src/middleware/session.rs", "line": 5, "kind": "import", "context": "use crate::auth::authenticate;" }
  ],
  "total": 3
}
```

Progressive precision -- same command, different backends:

```
$ cq refs Config                    # syntactic: tree-sitter name matching
$ cq refs Config                    # resolved: stack graphs follow imports (automatic when available)
$ cq refs Config --semantic         # semantic: full LSP type resolution via language server
```

The output format is identical. Only the `resolution` metadata field changes. An agent or script consuming `cq` output does not need to know which backend was used.

---

## Installation

From crates.io (once published):

```
cargo install codequery-cli
```

From source:

```
git clone https://github.com/jmfirth/codequery.git
cd codequery
cargo build --release
# binary at target/release/cq
```

Homebrew formula coming soon.

---

## Language Support

| Language | Tier | Precision | Notes |
|----------|------|-----------|-------|
| Rust | 1 | Resolved | Stack graph + LSP (rust-analyzer) |
| TypeScript | 1 | Resolved | Stack graph + LSP (typescript-language-server) |
| JavaScript | 1 | Resolved | Stack graph, includes JSX/TSX |
| Python | 1 | Resolved | Stack graph + LSP (pyright) |
| Go | 1 | Resolved | Stack graph + LSP (gopls) |
| C | 1 | Resolved | Stack graph + LSP (clangd) |
| C++ | 1 | Resolved | Stack graph + LSP (clangd) |
| Java | 1 | Resolved | Stack graph |
| Ruby | 1 | Resolved | Custom stack graph rules |
| C# | 1 | Resolved | Custom stack graph rules |
| PHP | 2 | Syntactic | Full extraction, name-based refs |
| Swift | 2 | Syntactic | Full extraction, name-based refs |
| Kotlin | 2 | Syntactic | Full extraction, name-based refs |
| Scala | 2 | Syntactic | Full extraction, name-based refs |
| Zig | 2 | Syntactic | Full extraction, name-based refs |
| Lua | 2 | Syntactic | Full extraction, name-based refs |
| Bash | 2 | Syntactic | Full extraction, name-based refs |

**Tier 1** languages have tree-sitter grammars compiled into the binary and stack graph rules for scope-resolved cross-references. **Tier 2** languages have full extraction (def, body, sig, outline, imports) but use syntactic name matching for cross-reference commands. **Tier 3** (not listed) supports runtime-loadable grammars via `.so`/`.dylib` files in `~/.local/share/cq/grammars/`.

---

## Commands

| Command | What it does |
|---------|-------------|
| `cq def <symbol>` | Find where a symbol is defined |
| `cq body <symbol>` | Extract the full source body of a symbol |
| `cq sig <symbol>` | Get the type signature or public interface |
| `cq refs <symbol>` | Find all references across the project |
| `cq callers <symbol>` | Find call sites for a function or method |
| `cq deps <symbol>` | Analyze internal dependencies of a function |
| `cq outline [file]` | List all symbols in a file with nesting |
| `cq symbols [--kind K]` | List all symbols in the project |
| `cq imports <file>` | List imports and dependencies for a file |
| `cq search <pattern>` | Structural search using AST patterns |
| `cq context <file>:<line>` | Get the enclosing symbol for a line |
| `cq tree [path]` | Hierarchical symbol tree for a directory |

### Global Flags

| Flag | Description |
|------|-------------|
| `--json` | JSON output (compact when piped, pretty on TTY) |
| `--raw` | Raw content only, no framing or metadata |
| `--pretty` | Force pretty-printed JSON |
| `--in <path>` | Narrow search scope to a directory or file |
| `--kind <K>` | Filter results by symbol kind |
| `--lang <L>` | Force language detection |
| `--semantic` | Use LSP for compiler-level precision |
| `--no-semantic` | Disable LSP even if daemon is running |
| `--project <path>` | Explicit project root (default: auto-detect) |
| `--cache` | Enable disk caching for this invocation |

### Output Modes

**Framed** (default) -- human-readable `@@ file:line:column kind name @@` headers with source content between them. Designed for quick scanning.

**JSON** (`--json`) -- structured output with symbol metadata, resolution info, and completeness fields. Compose with `jq` for complex queries.

**Raw** (`--raw`) -- content only, no framing. For piping into other tools: `cq body handle_request --raw | wc -l`.

---

## How Precision Works

Every `cq` result includes `resolution` and `completeness` metadata so consumers know exactly how much to trust the output.

### Three tiers of precision

**Syntactic.** Tree-sitter AST name and structure matching. Knows definitions from references from string literals. Cannot disambiguate when multiple types share a method name. Available for all 16 languages. Instant — under 100ms for targeted queries, under 1s for project-wide scans.

**Resolved.** Stack graph scope resolution. Follows import paths, qualified names, scope chains, and re-exports. Disambiguates across modules without a language server. Available for the 10 languages with TSG rules. Adds 1-2s for scope resolution on large projects.

**Semantic.** Full type resolution via language server. Resolves trait dispatch, generics, macros, and the full type system. Available for all 16 languages when a language server is installed. Cold start: 3-30s (starts a server, queries, stops). With `cq daemon`: sub-second (server stays warm).

### Automatic cascade

The precision cascade runs on every query, no configuration needed:

```
1. Daemon running?      --> semantic (sub-second, compiler-level)
2. --semantic flag?     --> oneshot LSP (start server, query, stop)
3. Stack graph rules?   --> resolved (follows imports, qualified names)
4. Fallback             --> syntactic (tree-sitter name matching)
```

A `cq refs` call on a machine with `cq daemon` running gets type-resolved results. The same call in a fresh CI container gets tree-sitter results. Both produce the same output format.

### Per-command completeness

| Command | Completeness |
|---------|-------------|
| `def`, `body`, `sig` | Exhaustive |
| `outline`, `symbols`, `tree` | Exhaustive |
| `imports`, `context`, `search` | Exhaustive |
| `refs`, `callers`, `deps` | Best-effort (scope-resolved or semantic when available) |

For best-effort commands, JSON output includes a `note` field explaining the limitation. Framed output appends a summary line.

---

## Quality and Validation

### Test suite

1,900+ automated tests across 7 crates, covering unit, integration, cross-language, precision, and proof tests.

### Real-world project validation

Stack graph rules hardened against ~1,800 source files from 24 open-source projects with zero TSG errors:

| Language | Projects | Tests |
|----------|----------|-------|
| Rust | ripgrep, serde, tokio, clap | 350+ |
| Go | gin, cobra, hugo, fiber | 300+ |
| C | redis, jq, curl, zstd | 300+ |
| C++ | nlohmann/json, fmt, catch2, leveldb | 250+ |
| Ruby | sinatra, rack, jekyll, devise | 250+ |
| C# | newtonsoft-json, dapper, polly | 250+ |

### LSP ground truth comparison

30 validation tests comparing stack graph results against language server output across 4 server implementations (rust-analyzer, gopls, clangd, typescript-language-server). Zero false positives across all non-ambiguous symbols. Coverage is 100% for function calls and imports, verified on codebases up to 7,800 lines.

---

## For AI Agents

`cq` is designed as a primitive for agentic code navigation.

**Token efficiency.** `cq body handle_request` returns 5 lines instead of reading a 500-line file. An agent using `cq` reads 10-50x fewer tokens per navigation step.

**Structured output.** `--json` produces machine-readable output with symbol kind, location, scope, and precision metadata. Compose with `jq` for complex queries.

**Precision metadata.** Every response includes `resolution` (how results were found) and `completeness` (whether the result set is exhaustive or best-effort). An agent can adjust its confidence automatically -- no guessing about result quality.

**Qualified names.** `cq body Router::add_route` disambiguates without reading multiple files. `cq body api::routes::handle_request` for module-qualified lookup.

**Context from errors.** `cq context src/api/routes.rs:47` maps a compiler error or stack trace line directly to the enclosing function. One command replaces the three-step workflow of outline, find, read.

**Stateless.** No setup, no daemon, no warm-up. Works in ephemeral environments, containers, CI, and WASM runtimes.

---

## Configuration

### `.cq.toml`

Project-level configuration. Placed in the project root. Supports LSP server overrides, timeout settings, and per-language binary paths.

### `.cqignore`

Additional file exclusion rules beyond `.gitignore`. Same syntax. Useful for excluding generated code, vendored dependencies, or large directories.

### Project root detection

`cq` walks up from the current directory looking for (in priority order): `.git/`, `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `setup.py`, `pom.xml`, `build.gradle`, `Makefile`, `CMakeLists.txt`, `.cq.toml`. Override with `--project <path>`.

### Daemon mode

Keep language servers warm for fast semantic queries:

```
$ cq daemon start          # background process, manages LSP server pool
$ cq refs authenticate     # auto-detects daemon, sub-second semantic results
$ cq daemon status         # show running servers
$ cq daemon stop           # clean shutdown
```

The daemon is auto-detected. When running, cross-reference commands automatically upgrade to semantic precision. Servers are started lazily per (project, language) and evicted after idle timeout.

---

## Development

Requires [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).

| Command | What it does |
|---------|-------------|
| `just check` | Format check + clippy lint |
| `just fmt` | Auto-format all code |
| `just test` | Run test suite (1,900+ tests) |
| `just test-all` | Full suite including LSP live tests |
| `just build` | Debug build |
| `just release` | Release build |
| `just run <args>` | Run cq (e.g., `just run refs greet --json`) |
| `just run-mcp` | Run cq-mcp server |
| `just ci` | Full CI pipeline (check + test + build + docs) |
| `just smoke-test` | Validate against real open-source projects |
| `just lsp-validate` | Compare results against language server ground truth |
| `just man` | Generate man page |
| `just doc` | Build and open API docs |

---

## License

Apache 2.0
