# cq

Semantic code query tool for the command line.

Tree-sitter-powered structural navigation for AI agents and humans.

---

## Why

There is a gap in the toolchain between `grep` and language servers.

`grep` / `ripgrep` is fast but semantically blind. Searching for `authenticate` returns the definition, every call site, comments mentioning it, strings containing it, and variable names that happen to include the substring. You parse the results yourself.

Language servers (LSP) are precise but heavy. They require installing the language toolchain, starting a server process, waiting for indexing (10-60s), and maintaining a persistent connection. They fail on broken code, which is the normal state during active development.

`cq` fills the gap:

- **Semantic, not textual.** Knows the difference between a definition, a call, a type reference, and a string literal.
- **Works on broken code.** Tree-sitter is error-tolerant -- it produces a usable AST even when the code has syntax errors.
- **Cross-language.** One binary handles Rust, TypeScript, Python, Go, C, C++, Java, and more. No toolchain required.
- **Zero setup.** `cargo install codequery-cli` and it works. Auto-detects project root, languages, and structure.
- **Stateless by default.** No daemon, no index files, no background process. Every invocation parses what it needs.
- **Fast.** Sub-100ms for targeted queries on any project size. Under 2s for project-wide scans on 400k lines.

---

## Quick Examples

Find where a symbol is defined:

```
$ cq def authenticate
@@ src/auth/mod.rs:23:4 function authenticate @@
```

Get the full source body of a function:

```
$ cq body handle_request
@@ src/api/routes.rs:42:4 function handle_request @@
pub async fn handle_request(req: Request) -> Response {
    let auth = authenticate(&req).await?;
    let data = parse_body(&req).await?;
    process(auth, data).await
}
```

Find all references to a symbol:

```
$ cq refs User
@@ src/api/routes.rs:44:20 call @@
    let auth = authenticate(&req).await?;
@@ src/ws/handler.rs:18:12 call @@
    if authenticate(&conn).await.is_err() {
@@ src/middleware/session.rs:5:4 import @@
use crate::auth::authenticate;

3 references (syntactic match -- may be incomplete)
```

See the structure of a file:

```
$ cq outline src/api/routes.rs
@@ src/api/routes.rs @@
  Router (struct, pub) :10
    new (method, pub) :18
    add_route (method, pub) :25
  handle_request (function, pub) :42
```

Structural search with AST pattern matching:

```
$ cq search "fn $NAME($$$) -> Result<$T>"
@@ src/api/routes.rs:42 handle_request @@
pub async fn handle_request(req: Request) -> Result<Response>
@@ src/auth/mod.rs:23 authenticate @@
pub async fn authenticate(req: &Request) -> Result<AuthContext>
```

---

## Installation

Once published:

```
cargo install codequery-cli
```

From source:

```
git clone https://github.com/jfirth/codequery.git
cd codequery
cargo build --release
# binary at target/release/cq
```

---

## Commands

| Command | Description |
|---------|-------------|
| `cq def <symbol>` | Find where a symbol is defined |
| `cq body <symbol>` | Get the full source body of a symbol |
| `cq sig <symbol>` | Get the type signature / public interface |
| `cq refs <symbol>` | Find all references to a symbol |
| `cq callers <symbol>` | Find call sites for a function or method |
| `cq deps <symbol>` | Analyze internal dependencies of a function |
| `cq outline [file]` | List all symbols in a file with nesting |
| `cq symbols [--kind K]` | List all symbols in the project |
| `cq imports <file>` | List imports / dependencies for a file |
| `cq search <pattern>` | Structural search using AST patterns |
| `cq context <file>:<line>` | Get the enclosing symbol for a line |
| `cq tree [path]` | Hierarchical symbol tree for a directory |

---

## Global Flags

| Flag | Description |
|------|-------------|
| `--json` | JSON output (compact when piped, pretty on TTY) |
| `--raw` | Raw content only, no framing or metadata |
| `--in <path>` | Narrow search scope to a directory or file |
| `--kind <K>` | Filter results by symbol kind |
| `--lang <L>` | Force language detection |
| `--project <path>` | Explicit project root (default: auto-detect) |
| `--cache` | Enable disk caching for this invocation |
| `--context <N>` | Include N surrounding lines with results |
| `--depth <N>` | Limit nesting depth (`tree`, `context`) |
| `--limit <N>` | Maximum number of results |

---

## Output Modes

### Framed (default)

Human-readable output with `@@` frame headers containing metadata and raw source between them:

```
$ cq body handle_request
@@ src/api/routes.rs:42:4 function handle_request @@
pub async fn handle_request(req: Request) -> Response {
    let auth = authenticate(&req).await?;
    let data = parse_body(&req).await?;
    process(auth, data).await
}
```

### JSON (`--json`)

Structured output for programmatic use and `jq` composition:

```
$ cq def handle_request --json
[
  {
    "symbol": "handle_request",
    "kind": "function",
    "file": "src/api/routes.rs",
    "line": 42,
    "column": 4,
    "resolution": "syntactic",
    "completeness": "exhaustive"
  }
]
```

### Raw (`--raw`)

Content only, no framing. For piping into other tools:

```
$ cq body handle_request --raw
pub async fn handle_request(req: Request) -> Response {
    let auth = authenticate(&req).await?;
    let data = parse_body(&req).await?;
    process(auth, data).await
}

$ cq body handle_request --raw | wc -l
5
```

---

## Language Support

### Tier 1 -- Full extraction and scope-resolved cross-references

All twelve commands with stack graph name resolution.

- Rust
- TypeScript / JavaScript / JSX / TSX
- Python
- Go
- C / C++
- Java

### Tier 2 -- Extraction only

Definition, outline, body, signature, and import commands work. Cross-reference commands use syntactic (name-based) matching.

- Ruby
- PHP
- C#
- Swift
- Kotlin
- Scala
- Zig
- Lua
- Bash / Shell

### Tier 3 -- Loadable at runtime

Additional languages via tree-sitter grammar `.so`/`.dylib` files placed in `$XDG_DATA_HOME/cq/grammars/` or `~/.local/share/cq/grammars/`.

---

## Precision Levels

Every response carries `resolution` and `completeness` metadata so consumers know how much to trust results.

### Syntactic

Tree-sitter AST name and structure matching. Knows definitions from references from strings. Cannot resolve which `bar()` method a call site invokes when multiple types have a `bar()` method.

### Resolved

Stack graph scope resolution. Resolves import paths, qualified names, scope chains, and re-exports. Cannot resolve type inference, trait dispatch, or generics.

### Semantic (future)

Full type resolution via language server integration. Not in v1.0.

**Per-command precision:**

| Command | Completeness |
|---------|-------------|
| `def`, `body`, `sig` | Exhaustive |
| `outline`, `symbols`, `tree` | Exhaustive |
| `imports`, `context`, `search` | Exhaustive |
| `refs`, `callers`, `deps` | Best-effort (scope-resolved for Tier 1 languages) |

For best-effort commands, JSON output includes a `note` field explaining the limitation. Framed output appends a summary line.

---

## Configuration

### `.cq.toml`

Project-level configuration file. Placed in the project root.

### `.cqignore`

Additional file exclusion rules beyond `.gitignore`. Same syntax as `.gitignore`. Useful for excluding generated code, vendored dependencies, or large directories that slow down wide commands.

### Project root detection

cq walks up from the current directory looking for (in priority order): `.git/`, `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `setup.py`, `pom.xml`, `build.gradle`, `Makefile`, `CMakeLists.txt`, `.cq.toml`. Override with `--project <path>`.

---

## For AI Agents

cq is designed as a primitive for AI agent code navigation. Key properties:

**Token efficiency.** `cq body handle_request` returns 5 lines instead of reading a 500-line file. An agent that uses cq reads 10-50x fewer tokens per navigation step.

**Structured output.** `--json` produces machine-readable output with symbol kind, location, scope, and precision metadata. Compose with `jq` for complex queries.

**Precision metadata.** Every response includes `resolution` (how results were found) and `completeness` (whether the result set is exhaustive or best-effort). An agent can adjust its confidence automatically.

**Qualified names.** `cq body Router::add_route` disambiguates without reading multiple files. `cq body api::routes::handle_request` for module-qualified lookup.

**Context from errors.** `cq context src/api/routes.rs:47` maps a compiler error or stack trace line directly to the enclosing function body. One command replaces the three-step workflow of outline, find, read.

**Stateless.** No setup, no daemon, no warm-up. Works in ephemeral environments, containers, CI, and WASM runtimes.

---

## Architecture

```
file discovery -> language detection -> tree-sitter parse -> AST query -> symbol extraction -> output formatting
```

For narrow commands (`def`, `body`, `sig`): text pre-filter (memmap + memchr) identifies candidate files, then only those files are parsed. This keeps targeted queries sub-100ms regardless of project size.

For wide commands (`refs`, `callers`, `symbols`): all source files are parsed in parallel across available cores using rayon. Results are merged and returned.

Tree-sitter grammars for Tier 1 languages are compiled into the binary. No runtime dependencies on language toolchains.

---

## License

Apache 2.0
