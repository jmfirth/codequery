# codequery (`cq`) Coding Conventions

This document defines the rules all contributors (human and agent) follow. When in doubt, this document wins.

---

## 1. Rust Style

### Formatting
- `rustfmt` with default settings. No overrides in `rustfmt.toml`.
- Run `cargo fmt` before every commit. The pre-commit hook enforces this.

### Linting
- `#![warn(clippy::pedantic)]` in every crate's `lib.rs` or `main.rs`.
- Zero clippy warnings. `cargo clippy --workspace -- -D warnings` must pass.
- `#[allow(clippy::...)]` is permitted ONLY with a comment explaining why:
  ```rust
  #[allow(clippy::too_many_lines)]
  // Symbol extraction match arms for all node types; splitting would obscure the logic
  ```

### Naming

- Types: `PascalCase`
- Functions, methods, variables: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Crate names: `cq-xxx` (kebab-case in Cargo.toml, `cq_xxx` as Rust identifiers)
- Module files: `snake_case.rs`
- Symbol model types: `PascalCase` matching their domain (e.g., `Symbol`, `SymbolKind`, `Reference`, `Location`)
- Test functions: `test_<what>_<condition>_<expected>` (e.g., `test_outline_rust_struct_includes_methods`)

### Documentation

- All `pub` items get doc comments (`///`).
- Doc comments describe **what** and **why**, not **how** (the code shows how).
- Crate-level doc comments (`//!`) in every `lib.rs` with a one-line summary and the crate's role in the query pipeline.
- No doc comments on private items unless the logic is genuinely non-obvious.

---

## 2. Error Handling

### Library crates
- Use `thiserror` derive for all error enums.
- Each crate has its own error type in `error.rs`, re-exported from `lib.rs`.
- Each crate defines `pub type Result<T> = std::result::Result<T, XxxError>;`
- No `unwrap()` or `expect()` in library code. Ever.
- Use `?` for propagation. Use `map_err` when crossing crate boundaries.
- Error messages are lowercase, no trailing punctuation (Rust convention).

### User-facing errors vs internal errors

These are two fundamentally different things. Never conflate them.

- **User-facing errors** are problems with the query or environment: symbol not found, no project root detected, unsupported language. These produce structured error output (exit code, message) and are expected runtime conditions.
- **Internal errors** are bugs in cq itself — invariant violations, unexpected tree-sitter states, I/O failures. These use `thiserror` error types and propagate via `?`. An internal error means cq has a bug.

### Parse errors

Tree-sitter parse errors on individual files are **warnings**, not fatal errors. They are collected and reported in a `warnings` field but do not prevent results from other files. This is a core design principle — cq works on broken code.

### Panics
- `panic!()`, `todo!()`, `unimplemented!()`: never in merged library code.
- `unreachable!()`: acceptable when the code path is provably unreachable and the compiler can't see it.
- In tests: `unwrap()`, `expect()`, and `panic!()` are fine.

---

## 3. Dependencies

### Principles
- **Minimize dependency count.** Every dependency is a security and maintenance surface.
- **Prefer well-maintained, widely-used crates.** Check download counts and last publish date.
- **No feature bloat.** Enable only the features we use. Disable default features when appropriate.
- **Pin major versions** in Cargo.toml (e.g., `"1"` not `"*"`).

### Approved dependencies

| Crate | Purpose | Used in |
|-------|---------|---------|
| `thiserror` | Error derive macros | All library crates |
| `anyhow` | Top-level error handling | `cq-cli` |
| `clap` (derive) | Argument parsing | `cq-cli` |
| `tree-sitter` | Core parsing engine | `cq-parse` |
| `tree-sitter-rust` | Rust grammar | `cq-parse` |
| `tree-sitter-typescript` | TS/JS grammar | `cq-parse` (Phase 1) |
| `tree-sitter-python` | Python grammar | `cq-parse` (Phase 1) |
| `tree-sitter-go` | Go grammar | `cq-parse` (Phase 1) |
| `tree-sitter-c` | C grammar | `cq-parse` (Phase 1) |
| `tree-sitter-cpp` | C++ grammar | `cq-parse` (Phase 1) |
| `tree-sitter-java` | Java grammar | `cq-parse` (Phase 1) |
| `rayon` | Parallel file parsing | `cq-index` (Phase 1) |
| `memmap2` | Memory-mapped file I/O for grep pre-filter | `cq-index` (Phase 1) |
| `memchr` | Fast byte search for grep pre-filter | `cq-index` (Phase 1) |
| `ignore` | .gitignore-compatible file walking | `cq-core` |
| `stack-graphs` | Scope graph name resolution | `cq-resolve` (Phase 2) |
| `tree-sitter-stack-graphs` | Stack graph / tree-sitter integration | `cq-resolve` (Phase 2) |
| `serde` + `serde_json` | JSON output format | `cq-cli` |
| `tempfile` | Test temp directories | dev-dependency |

Adding a new dependency requires justification. Don't pull in a crate for something the standard library can do.

### Forbidden patterns
- No async runtime dependencies (tokio, async-std) — cq is synchronous.
- No `unsafe` without a `// SAFETY:` comment explaining the invariants.
- No `build.rs` scripts unless absolutely necessary for grammar compilation (and documented why).

---

## 4. Architecture

### Crate boundaries
- Crate boundaries are defined during phase planning and documented in PLAN.md.
- No circular dependencies. The dependency graph is a DAG.
- Cross-crate communication uses well-defined trait interfaces or shared types from `cq-core`.
- Private implementation details stay private. Only the designed public API is `pub`.

### Tree-sitter patterns

**Parser usage:**
- Tree-sitter parsers are created per-language and reused across files of the same language.
- Parse results are `tree_sitter::Tree` values — they borrow the source text.
- Always pass source as `&[u8]` to the parser. Use `.utf8_text()` on nodes for string extraction.
- Tree-sitter queries (tags, highlights) are compiled once and reused.

**Tags queries:**
- Definition and reference extraction uses tree-sitter's tags query system where available.
- Each language has a `tags.scm` query file that identifies definitions and references.
- When tags queries are insufficient, fall back to AST node type matching.

**Error tolerance:**
- Tree-sitter always produces a tree, even for broken code. Nodes with errors have `is_error()` or `is_missing()`.
- Skip error nodes gracefully — extract what you can, warn about what you can't.
- Never bail out of a file entirely because of parse errors.

**Node navigation:**
- Use named children (`child_by_field_name`) over positional children (`child(N)`) — more resilient to grammar updates.
- Prefer `TreeCursor` for deep traversals to avoid excessive allocations.

### Stack graph patterns (Phase 2+)

**Resolution layer:**
- Stack graphs consume tree-sitter parse trees and produce name bindings.
- `cq-resolve` owns graph construction and resolution. `cq-index` consumes resolution results.
- Per-language stack graph rules live in `cq-resolve/src/rules/`. Each language has its own module.
- Fallback behavior: if stack graph rules don't exist for a language, cross-reference commands use syntactic matching and metadata reflects it (`"resolution": "syntactic"`).

**Performance considerations:**
- Stack graph construction is incremental — only rebuild for changed files.
- Graph resolution should stay within the performance budget: sub-100ms for narrow commands, sub-2s for wide commands.
- If resolution is too slow for a particular query, fall back to syntactic and label accordingly.

### Symbol model

- `Symbol` is the core type: name, kind, file, line, column, end_line, visibility, optional body/signature/doc.
- `SymbolKind` is an enum: Function, Method, Struct, Class, Trait, Interface, Enum, Type, Const, Static, Module, Impl, Test.
- `Location` is file + line + column.
- `Reference` adds a `kind` field (Call, TypeUsage, Import, Assignment) to Location.
- These types live in `cq-core` and are used by all other crates.

### Output formatting

- Three modes: Framed (default), JSON (`--json`), Raw (`--raw`).
- Framed output uses `@@ file:line:column kind name @@` headers with raw source between them.
- JSON is compact when piped, pretty when TTY.
- Output formatting logic lives in `cq-cli`, not in library crates. Library crates return typed data.

### Ownership and borrowing
- Prefer borrowing over cloning. Clone only when ownership transfer is needed.
- Use `&str` in function parameters, `String` in struct fields that own data.
- Use `Cow<'_, str>` when a function might or might not need to allocate.
- Source text is typically borrowed from memory-mapped files or read buffers.

### Type design
- Use newtypes to distinguish semantically different values of the same primitive type (e.g., `LineNumber`, `Column`) — but only when confusion is a real risk. Don't over-newtype.
- Enums over booleans when a function has more than one boolean parameter.
- Prefer `Option` over sentinel values (no `-1` meaning "not found").

### Module organization
- One primary type per file. `parser.rs` contains `Parser`, `scanner.rs` contains `Scanner`.
- `mod.rs` is forbidden. Use `module_name.rs` with `mod module_name;` in the parent.
- Test modules: `#[cfg(test)] mod tests { ... }` at the bottom of the file they test.

---

## 5. Testing

### Philosophy
- **Test-first.** Write the test, see it fail, then implement.
- **Test behavior, not implementation.** Tests assert on observable outputs, not internal state.
- **100% coverage of public API.** Every `pub fn` has at least one test. Untested public API is a bug.

### Test organization
- Unit tests: `#[cfg(test)] mod tests` in each source file
- Integration tests: `tests/integration/` — one file per command, test against fixture projects
- Cross-language tests: `tests/cross_language/` — same command across all Tier 1 languages
- Performance tests: `tests/performance/` — benchmarks on large projects
- Test helpers: `#[cfg(test)]` gated modules

### Test fixtures
- Fixture projects live in `tests/fixtures/` with one directory per scenario:
  - `rust_project/` — Rust project with functions, structs, traits, impls, modules
  - `typescript_project/` — TypeScript project (Phase 1)
  - `python_project/` — Python project (Phase 1)
  - `go_project/` — Go project (Phase 1)
  - `mixed_project/` — Multi-language project (Phase 1)
  - `broken_project/` — Project with intentional syntax errors
- Fixtures are minimal but representative. Each fixture must exercise the features being tested.
- Fixtures are checked into the repo and reviewed like any other code.

### Test naming
```
test_<command>_<scenario>_<expected_behavior>
```
Examples:
- `test_outline_rust_file_lists_all_symbols`
- `test_def_finds_function_by_name`
- `test_def_multiple_matches_returns_all`
- `test_body_includes_doc_comment`
- `test_refs_includes_call_sites_and_imports`
- `test_project_detection_finds_cargo_toml`

### Test quality
- No `#[should_panic]` — test error returns instead.
- No `sleep()` in tests. Use deterministic synchronization.
- No filesystem side effects outside `tempfile` directories (except reading fixtures).
- Each test is independent. No shared mutable state between tests.
- Test both the happy path and edge cases (empty file, no matches, broken syntax, ambiguous names).

### Coverage
- Target: 100% of public API surface area.
- Use `cargo-tarpaulin` or `cargo-llvm-cov` for measurement (configured via `just coverage`).
- Coverage of private functions is nice but not required — good public API coverage exercises most private code.

---

## 6. Performance

### Principles
- **Correctness first, then performance.** Don't optimize before profiling.
- **Sub-100ms for narrow commands.** `def`, `body`, `sig`, `outline`, `imports`, `context` must be fast enough that users don't notice latency.
- **Under 2s for wide commands.** `refs`, `callers`, `symbols`, `tree` on a 400k-line project with 8 cores.

### Specific guidelines
- Use the grep pre-filter (memmap + memchr) for narrow commands to avoid parsing unnecessary files.
- Parse files in parallel with rayon for wide commands.
- Avoid `collect()` into a Vec when you can iterate directly.
- Use `&str` slicing instead of `String::clone()` where lifetime allows.
- Memory-map large files instead of reading them entirely into heap memory.
- Tree-sitter parser instances are reusable — don't create a new parser per file.

---

## 7. Safety

- No `unsafe` without a `// SAFETY:` comment. Every unsafe block documents why it's sound.
- cq should have minimal (ideally zero) `unsafe` code outside of tree-sitter FFI boundaries.
- Never trust input sizes. Validate before allocating. A malicious or pathological source file must not cause OOM.
- Handle adversarial input gracefully — deeply nested code, extremely long lines, files with no valid syntax.

---

## 8. Git and Commits

- Branch naming: `task/NNN-short-name`
- Commit messages: imperative mood, one-line summary, optional blank line + body
- One logical change per commit
- No merge commits in feature branches (rebase workflow)
- `main` always builds and passes `just test`
- Commit hooks enforce `just check && just test` before commit

---

## 9. What NOT to Do

- Don't add features beyond what the current task specifies.
- Don't refactor code outside your task's scope.
- Don't add comments explaining obvious code.
- Don't add `// TODO` without a tracked task in PLAN.md.
- Don't use `String` where `&str` suffices.
- Don't use `Box<dyn Trait>` where a generic `<T: Trait>` works (unless you need type erasure).
- Don't add optional dependencies or feature flags without architectural justification.
- Don't suppress warnings with `#[allow]` — fix the warning.
- Don't write "defensive" code against impossible states. If a state is impossible, `unreachable!()` is appropriate.
- Don't conflate user-facing errors (query/environment problems) with internal errors (cq bugs).
- Don't bail out of a file because of parse errors — extract what you can and warn.
