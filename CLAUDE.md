# codequery (`cq`)

A semantic code query tool for the command line. Tree-sitter-powered structural navigation for AI agents and humans.

See `SPECIFICATION.md` for the full design.

---

## Project State

Phase 4 (LSP Integration) complete. 1590 tests, 6 crates, 12 commands, 16 languages. Phase 5 (Distribution) not started. 70 tasks completed across Phases 0-4.

## Key Documents

| Document | Purpose | Read when |
|----------|---------|-----------|
| `SPECIFICATION.md` | Tool design, command surface, output formats, architecture | Before any design decision |
| `PLAN.md` | Task plan, dependencies, status tracking | Before starting any work |
| `PROCESS.md` | Agent workflow, quality gates, task lifecycle | Before any work |
| `CONVENTIONS.md` | Coding standards, style, architecture rules | Before writing code |
| `BOOTSTRAP.md` | TL/Orchestrator role activation | TL role only |
| `agents/` | Developer, reviewer, plan-reviewer role contracts | Agents read their own role doc |
| `tasks/` | Per-task specifications | Developer and reviewer agents |

## Query Pipeline

```
file discovery → language detection → tree-sitter parse → AST query → symbol extraction → output formatting
```

For narrow commands (def, body, sig): text pre-filter → candidate files → parse subset → extract
For wide commands (refs, callers, symbols): parallel parse all files → index → query → merge results

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `codequery-core` | Symbol types, project detection, file discovery, config |
| `codequery-parse` | Tree-sitter parsing, per-language extraction (16 languages), search engine |
| `codequery-index` | Parallel scanning (rayon), grep pre-filter (memchr), symbol index, reference extraction, caching |
| `codequery-resolve` | Stack graph resolution (7 languages), TSG rules, resolver facade |
| `codequery-lsp` | LSP client, JSON-RPC transport, server lifecycle, daemon, cascade |
| `codequery-cli` | Binary entry point, 12 commands, output formatting |

## Architectural Invariants

These are non-negotiable constraints from the specification:

1. **Stateless by default.** Every invocation parses what it needs. Optional caching is opt-in only. An optional daemon mode (`cq daemon start`) provides a warm LSP connection for semantic precision; the daemon is never required -- the three-tier cascade (daemon, oneshot LSP, stack graph) falls back gracefully.
2. **Error-tolerant.** Tree-sitter produces usable ASTs even on broken code. A parse error in one file must not block results from other files.
3. **Cross-language from one binary.** Tier 1 grammars (Rust, TypeScript, Python, Go, C/C++, Java) are compiled into the binary. No runtime dependencies on language toolchains.
4. **Human-readable default output.** Framed plain text with `@@ file:line:column kind name @@` delimiters. JSON and raw modes via flags.
5. **Performance contract.** Narrow commands sub-100ms on any project size. Wide commands under 2s on 400k lines with 8 cores.

## Commands

| Command | What it does |
|---------|-------------|
| `just check` | Format check + clippy |
| `just test` | Fast test suite (<10s) |
| `just test-all` | Full suite: unit + integration + cross-language + performance |
| `just build` | Debug build |
| `just release` | Release build |
| `just start` | Run cq (pass-through args) |
| `just ci` | Full CI pipeline |
| `just doc` | Build and open docs |

## Code Standards

- `#![warn(clippy::pedantic)]` on all crates
- `cargo fmt` enforced — no exceptions
- No `unwrap()` or `expect()` in library code
- Doc comments on all public types, traits, functions
- `thiserror` for library errors, `anyhow` in binary crate only
- See `CONVENTIONS.md` for complete standards

## Testing Model

| Level | What | Where |
|-------|------|-------|
| Unit | Internal API correctness | `#[cfg(test)]` in each module |
| Integration | Command → expected output against fixture projects | `tests/integration/` |
| Cross-language | Same commands across all Tier 1 languages | `tests/cross_language/` |
| Performance | Benchmarks on large projects | `tests/performance/` |

Phase goals are measured by test passage and command correctness. See `PROCESS.md` for the full quality framework.
