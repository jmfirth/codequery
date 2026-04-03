# codequery (`cq`)

Semantic code query tool for the command line. Tree-sitter-powered structural navigation for AI agents and humans.


---

## Project State

Release-ready. ~885 tests, 7 crates, 24 commands, 71 languages. Stack graphs for 10 languages (all Tier 1 + Ruby, C#), hardened against 24 real-world open-source projects. All grammars load at runtime via WASM plugins (auto-install on first use). Binary is 7.9MB. MCP server ships as `cq-mcp`.

## Key Documents

| Document | Purpose | Read when |
|----------|---------|-----------|
| `CONVENTIONS.md` | Coding standards, style, architecture rules | Before writing any code |

## Crate Structure

| Crate | Purpose |
|-------|---------|
| `codequery-core` | Symbol types, project detection, file discovery, config |
| `codequery-parse` | Tree-sitter parsing, per-language extraction (71 languages via WASM plugins), search engine |
| `codequery-index` | Parallel scanning (rayon), grep pre-filter (memchr), symbol index, reference extraction, caching |
| `codequery-resolve` | Stack graph resolution (10 languages), TSG rules, resolver facade |
| `codequery-lsp` | LSP client, JSON-RPC transport, server lifecycle, daemon, cascade |
| `codequery-cli` | Binary entry point (`cq`), 24 commands, output formatting |
| `codequery-mcp` | MCP server (`cq-mcp`), exposes all commands as AI-callable tools |

## Query Pipeline

```
file discovery → language detection → tree-sitter parse → AST query → symbol extraction → output formatting
```

For narrow commands (`def`, `body`, `sig`): text pre-filter → candidate files → parse subset → extract
For wide commands (`refs`, `callers`, `symbols`): parallel parse all files → index → query → merge results

### Precision Cascade

Cross-reference commands (`refs`, `callers`, `deps`) use a three-tier cascade:

```
1. Daemon running?  → semantic precision (sub-second, compiler-level)
2. --semantic flag?  → oneshot LSP (10-30s, but precise)
3. Stack graph rules? → scope-resolved (follows imports, qualified names)
4. Fallback          → syntactic (tree-sitter name matching)
```

Every result carries `resolution` metadata (`semantic`, `resolved`, or `syntactic`) so consumers know the precision level.

## Architectural Invariants

These are non-negotiable constraints:

1. **Stateless by default.** Every invocation parses what it needs. Optional caching is opt-in. Daemon mode is optional — the cascade falls back gracefully.
2. **Error-tolerant.** Tree-sitter produces usable ASTs even on broken code. A parse error in one file must not block results from other files.
3. **Cross-language from one binary.** All 71 language grammars load at runtime via WASM plugins — auto-installed on first use from the plugin registry. No compiled-in grammars; the binary stays small (7.9MB, strip+LTO+panic=abort). No runtime dependencies on language toolchains.
4. **Human-readable default output.** Framed plain text with `@@ file:line:column kind name @@` delimiters. JSON and raw modes via flags.
5. **Performance contract.** Narrow commands sub-100ms on any project size. Wide commands under 2s on 400k lines with 8 cores.

## Build Commands

| Command | What it does |
|---------|-------------|
| `just check` | Format check + clippy |
| `just test` | Full test suite |
| `just build` | Debug build |
| `just release` | Release build |
| `just ci` | Full CI pipeline |
| `just man` | Generate man page |

## Development Workflow

- Run `just check` before committing — enforces `cargo fmt` and clippy
- Run `just test` after changes — must pass before merging
- Write tests for new functionality — correctness tests first (red-green)
- Follow `CONVENTIONS.md` strictly for all code changes

### Testing Model

| Level | What | Where |
|-------|------|-------|
| Unit | Internal API correctness | `#[cfg(test)]` in each module |
| Integration | Command → expected output against fixture projects | `crates/codequery-cli/tests/` |
| Cross-language | Same commands across tier-1 and tier-2 languages | `test_coverage_tier1.rs`, `test_coverage_tier2.rs` |
| Precision | Stack graph resolution proof, LSP comparison | `test_proof.rs`, `test_precision.rs` |
| Strict | Exact resolution tiers per language | `test_stack_graph_strict.rs` |

### Stack Graph Rules

TSG rules live in `crates/codequery-resolve/tsg/{language}/stack-graphs.tsg`. When writing or modifying TSG rules:

- **Never use `(_)` wildcards** in parent-child stanzas. Always use explicit type lists. Wildcards match comments, literals, and other node types that don't have scoped variable stubs, breaking graph construction.
- Test against real-world projects, not just synthetic fixtures. Use `scripts/smoke-test.sh` to scan popular open-source projects.
- Validate against LSP ground truth using `scripts/lsp-validation.sh`.
- After changes, run the comprehensive TSG error survey (see `test_stack_graph_strict.rs` diagnostic tests).

### Validation Scripts

| Script | Purpose |
|--------|---------|
| `scripts/smoke-test.sh` | Clone and test against 15+ real open-source projects |
| `scripts/lsp-validation.sh` | Compare stack graph results against language server ground truth |
