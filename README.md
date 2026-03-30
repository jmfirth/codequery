# cq

The `jq` for source code.

---

`cq` is a semantic code query tool for the command line. **75 languages, one binary, zero setup.** It answers structural questions about code: where is this symbol defined, what does it look like, who calls it, what does it depend on. Languages install automatically on first use.

There is a gap between `grep` and language servers. Grep is fast but semantically blind. Language servers are precise but heavy — they need toolchains, indexing time, and compilable code. `cq` bridges the gap with three tiers of precision that activate automatically:

- **Tree-sitter** -- instant (<100ms), works on broken code, knows definitions from references from strings. 75 languages.
- **Stack graphs** -- scope-resolved name binding. Follows imports, qualified names, and cross-file references. 10 languages, no setup required.
- **Language servers** -- compiler-level precision via LSP. Resolves generics, trait dispatch, type inference. 40+ languages with built-in server configs. Optional `cq daemon` keeps servers warm for sub-second queries.

The same command produces the same output format at every tier. Only the `resolution` metadata field changes — from `syntactic` to `resolved` to `semantic`. The cascade runs automatically: you get the best precision available without configuring anything.

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

## Agent Integration

`cq` is built for AI agents. `cq body handle_request` returns 5 lines instead of reading a 500-line file. Every response includes precision metadata so agents know how much to trust results.

Two ways to give your agent `cq`:

### Option 1: CLI + llms.txt (works with any agent framework)

Install `cq` on the agent's PATH, then paste the contents of [`llms.txt`](llms.txt) into the agent's system prompt. The agent can immediately use `cq` via shell commands — `llms.txt` teaches it the full command surface, output formats, flags, and efficient usage patterns. No pretraining required.

```python
# Example: adding cq to an agent's system prompt
system_prompt = open("path/to/cq/llms.txt").read() + "\n\n" + your_instructions
```

### Option 2: MCP server (native tool integration)

Add `cq` as an MCP tool server. All 12 commands become native tool calls for Claude, Cursor, and any MCP-compatible harness. No Rust toolchain needed — `npx` downloads the binary automatically.

```json
{
  "mcpServers": {
    "cq": { "command": "npx", "args": ["-y", "@codequery/mcp"] }
  }
}
```

Also available via pip (`uvx codequery-mcp`) or direct binary (`cq-mcp`).

Both options give access to the same 12 commands, 75 languages, and three-tier precision cascade.

---

## Language Support

**75 languages.** Languages install automatically on first use — `cq outline app.ex` downloads Elixir support in the background, then shows results. No manual setup.

Python, TypeScript, JavaScript, Rust, Go, C, C++, Java, Ruby, PHP, C#, Swift, Kotlin, HTML, CSS, JSON, YAML, TOML, Elixir, Haskell, Dart, SQL, Scala, Zig, Lua, Bash, Dockerfile, Terraform, Markdown, XML, Protobuf, GraphQL, Vue, Svelte, F#, Groovy, Objective-C, Nix, CMake, SCSS, Elm, Solidity, Verilog, CUDA, Fortran, Ada, Pascal, LaTeX, Prisma, Bicep, and more. Run `cq grammar list` for the full list.

```
$ cq grammar install --all     # pre-download everything (optional)
```

**Scope-resolved cross-references** for 10 languages (Rust, TypeScript, JavaScript, Python, Go, C, C++, Java, Ruby, C#) — follows imports, qualified names, and resolves across files without a language server.

**LSP semantic precision** for 40+ languages with built-in server configs. The cascade runs automatically — no configuration needed.

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

**Syntactic.** Tree-sitter AST name and structure matching. Knows definitions from references from string literals. Cannot disambiguate when multiple types share a method name. All 75 languages. Instant — under 100ms for targeted queries, under 1s for project-wide scans.

**Resolved.** Stack graph scope resolution. Follows import paths, qualified names, scope chains, and re-exports. Disambiguates across modules without a language server. 10 languages. Adds 1-2s for scope resolution on large projects.

**Semantic.** Full type resolution via language server. Resolves trait dispatch, generics, macros, and the full type system. 40+ languages with built-in LSP configs. Cold start: 3-30s (starts a server, queries, stops). With `cq daemon`: sub-second (server stays warm).

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

Stack graph rules hardened against ~1,800 source files from 24 open-source projects with zero errors:

| Language | Validated against |
|----------|------------------|
| Rust | ripgrep, serde, tokio, clap |
| Go | gin, cobra, hugo, fiber |
| C | redis, jq, curl, zstd |
| C++ | nlohmann/json, fmt, catch2, leveldb |
| Ruby | sinatra, rack, jekyll, devise |
| C# | newtonsoft-json, dapper, polly |

### LSP ground truth comparison

30 validation tests comparing stack graph results against language server output across 4 server implementations (rust-analyzer, gopls, clangd, typescript-language-server). Zero false positives across all non-ambiguous symbols. Coverage is 100% for function calls and imports, verified on codebases up to 7,800 lines.

---

## Why Agents Love cq

- **10-50x fewer tokens** -- `cq body handle_request` returns 5 lines, not a 500-line file
- **Structured output** -- `--json` gives symbol kind, location, precision metadata. Compose with `jq`
- **Self-calibrating** -- `resolution` and `completeness` fields let agents gauge trust automatically
- **Qualified names** -- `cq body Router::add_route` disambiguates without reading multiple files
- **Error navigation** -- `cq context src/api/routes.rs:47` maps a stack trace line to its enclosing function
- **Stateless** -- no setup, no daemon, no warm-up. Works in ephemeral environments, containers, CI

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
