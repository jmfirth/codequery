# cq Examples

Real output from running `cq` against its own codebase (and an Elixir project for runtime language demos).

All output includes a `@@ meta @@` header with resolution tier, completeness, and result count.

---

## outline — What's in this file?

```bash
$ cq outline crates/codequery-core/src/symbol.rs
```

```
@@ meta resolution=syntactic completeness=exhaustive total=7 @@

@@ crates/codequery-core/src/symbol.rs @@
  Symbol (struct, pub) :8
  SymbolKind (enum, pub) :39
  fmt::Display for SymbolKind (impl, priv) :70
    fmt (method, priv) :71
  Location (struct, pub) :94
  Visibility (enum, pub) :105
  fmt::Display for Visibility (impl, priv) :117
    fmt (method, priv) :118
  tests (module, priv) :129
```

---

## def — Where is this symbol defined?

```bash
$ cq def SymbolKind
```

```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/symbol.rs:39:0 enum SymbolKind @@
```

Scope to a file or directory:

```bash
$ cq def extract_symbols --in crates/codequery-parse/src/extract.rs
```

```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-parse/src/extract.rs:46:0 function extract_symbols @@
```

---

## body — Full source of a symbol

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

## sig — Type signature without the body

```bash
$ cq sig Symbol --in crates/codequery-core
```

```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/symbol.rs:8:0 struct Symbol @@
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub visibility: Visibility,
    pub children: Vec<Symbol>,
    pub doc: Option<String>,
    pub body: Option<String>,
    pub signature: Option<String>,
}
```

---

## context — What function contains this line?

Given a compiler error at line 50 of extract.rs:

```bash
$ cq context crates/codequery-parse/src/extract.rs:50
```

```
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-parse/src/extract.rs:46:0 function extract_symbols (contains line 50) @@
pub fn extract_symbols(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,    // <- line 50
) -> Vec<Symbol> {
    match language {
        Language::Rust => RustExtractor::extract_symbols(source, tree, file),
        ...
    }
}
```

---

## imports — What does this file depend on?

```bash
$ cq imports crates/codequery-cli/src/commands/def.rs
```

```
@@ meta resolution=syntactic completeness=exhaustive total=6 @@

@@ crates/codequery-cli/src/commands/def.rs @@
  @@ crates/codequery-cli/src/commands/def.rs:3 use std::path::Path @@
  @@ crates/codequery-cli/src/commands/def.rs:5 use codequery_core::Language @@
  @@ crates/codequery-cli/src/commands/def.rs:6 use codequery_lsp::{daemon_file, oneshot} @@
  @@ crates/codequery-cli/src/commands/def.rs:8 use super::common::find_symbols_by_name @@
  @@ crates/codequery-cli/src/commands/def.rs:9 use crate::args::{ExitCode, OutputMode} @@
  @@ crates/codequery-cli/src/commands/def.rs:10 use crate::output::format_def @@
```

---

## refs — Who references this symbol?

```bash
$ cq refs greet --project tests/fixtures/rust_project
```

```
@@ meta resolution=resolved completeness=best_effort total=5 @@

@@ src/lib.rs:9:0 function greet (definition) @@
@@ src/lib.rs:9:0 definition @@
    pub fn greet(name: &str) -> String {
@@ src/lib.rs:15:14 call @@
    let msg = greet("world");
@@ tests/integration.rs:1:21 import @@
    use fixture_project::greet;
@@ tests/integration.rs:5:15 call @@
    assert_eq!(greet("world"), "Hello, world!");
@@ tests/integration.rs:10:15 call @@
    assert_eq!(greet(""), "Hello, !");

5 references (resolved)
```

Note `resolution=resolved` — stack graphs traced the imports and calls across files. With `cq daemon` running, this upgrades to `resolution=semantic` with compiler-level precision.

---

## callers — Who calls this function?

```bash
$ cq callers greet --project tests/fixtures/rust_project
```

```
@@ meta resolution=resolved completeness=best_effort total=3 @@

@@ src/lib.rs:9:0 function greet (definition) @@
@@ src/lib.rs:15:14 call (in run) @@
    let msg = greet("world");
@@ tests/integration.rs:5:15 call (in test_greet) @@
    assert_eq!(greet("world"), "Hello, world!");
@@ tests/integration.rs:10:15 call (in test_greet_empty) @@
    assert_eq!(greet(""), "Hello, !");

3 callers (resolved)
```

---

## deps — What does this function depend on?

```bash
$ cq deps process_users --project tests/fixtures/rust_project
```

```
@@ meta resolution=syntactic completeness=best_effort total=7 @@

@@ src/services.rs:38:0 function process_users @@
  User (type_reference) -> src/models.rs
  Vec (type_reference) -> <unresolved>
  String (type_reference) -> <unresolved>
  collect (call) -> <unresolved>
  map (call) -> <unresolved>
  iter (call) -> <unresolved>
  summarize (call) -> src/services.rs
```

`<unresolved>` = standard library or external crate (not in project).

---

## symbols — Find all symbols of a kind

```bash
$ cq symbols --kind enum --in crates/codequery-core
```

Lists every enum defined in the core crate with its file and line.

---

## tree — Project structure at a glance

```bash
$ cq tree crates/codequery-core/src/symbol.rs --depth 0
```

```
@@ meta resolution=syntactic completeness=exhaustive total=7 @@

@@ crates/codequery-core/src/symbol.rs @@
crates/codequery-core/src/symbol.rs
  Symbol (struct, pub) :8
  SymbolKind (enum, pub) :39
  fmt::Display for SymbolKind (impl, priv) :70
  Location (struct, pub) :94
  Visibility (enum, pub) :105
  fmt::Display for Visibility (impl, priv) :117
  tests (module, priv) :129
```

---

## search — Structural pattern matching (S-expressions)

Find all functions returning `Result`:

```bash
$ cq search '(function_item name: (identifier) @name return_type: (generic_type type: (type_identifier) @ret (#eq? @ret "Result")))' \
  --in crates/codequery-core
```

```
@@ meta resolution=syntactic completeness=exhaustive total=5 @@

@@ crates/codequery-core/src/config.rs:90:7 @@
load_config

@@ crates/codequery-core/src/discovery.rs:239:7 @@
discover_files

@@ crates/codequery-core/src/discovery.rs:290:7 @@
discover_files_with_config

@@ crates/codequery-core/src/project.rs:32:7 @@
detect_project_root

@@ crates/codequery-core/src/project.rs:62:7 @@
detect_project_root_or
```

Patterns use tree-sitter's S-expression query language — works across any language grammar.

---

## dead — Find unreferenced symbols

```bash
$ cq dead --project tests/fixtures/rust_project
```

```
@@ meta resolution=syntactic completeness=best_effort total=18 note="structural analysis; public symbols may have external callers not visible to cq" @@

@@ src/lib.rs:14:0 function run (pub) — zero references @@
@@ src/lib.rs:20:0 const MAX_RETRIES (pub) — zero references @@
@@ src/services.rs:16:4 method is_adult (pub) — zero references @@
@@ src/services.rs:20:4 method internal_helper — zero references @@
@@ src/services.rs:38:0 function process_users (pub) — zero references @@
...
```

---

## diagnostics — Syntax errors from tree-sitter

```bash
$ cq diagnostics path/to/broken_file.rs
```

Reports parse errors (ERROR/MISSING nodes) with line numbers. Clean files produce empty output with `total=0` in the meta header. Works on any language with a grammar installed.

---

## hover — Type info at a location

```bash
$ cq hover crates/codequery-core/src/symbol.rs:8
```

```
@@ meta resolution=syntactic completeness=exhaustive total=2 @@

@@ crates/codequery-core/src/symbol.rs:8:0 signature @@
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    ...
}

@@ crates/codequery-core/src/symbol.rs:8:0 docs @@
/// A source code symbol extracted from a parsed file.
```

With `cq daemon` running, hover returns full type information from the language server.

---

## rename — Rename a symbol across the project

Dry-run preview (syntactic precision):

```bash
$ cq rename greet hello --project tests/fixtures/rust_project --dry-run
```

```
@@ meta resolution=syntactic completeness=best_effort total=5 note="syntactic name matching; may include false positives" @@

Rename greet → hello: 5 edits across 2 files [syntactic — preview only]
Run with --apply or use a higher precision tier (daemon) to apply.

--- src/lib.rs
+++ src/lib.rs
@@ -9:7 @@
-greet
+hello
@@ -15:14 @@
-greet
+hello

--- tests/integration.rs
+++ tests/integration.rs
@@ -1:21 @@
-greet
+hello
@@ -5:15 @@
-greet
+hello
@@ -10:15 @@
-greet
+hello
```

Apply with `--apply`. At `resolved` or `semantic` precision, rename applies directly. At `syntactic`, it shows a preview by default.

---

## callchain — Multi-level call hierarchy

```bash
$ cq callchain greet --project tests/fixtures/rust_project --depth 2
```

```
@@ meta resolution=syntactic completeness=best_effort total=4 note="recursive caller analysis; may miss indirect calls" @@

greet (function) src/lib.rs:9
  ← run (function) src/lib.rs:14
  ← test_greet (test) tests/integration.rs:4
  ← test_greet_empty (test) tests/integration.rs:9
```

---

## hierarchy — Type hierarchy

```bash
$ cq hierarchy Validate --project tests/fixtures/rust_project
```

```
@@ meta resolution=syntactic completeness=best_effort total=2 @@

@@ Validate (trait) src/traits.rs:4 @@

Subtypes:
  ↓ User (struct) src/models.rs:5
```

---

## grammar — Install language support

```bash
$ cq grammar install elixir
```

```
Downloading elixir language package for cq v1.0.0...
  from: https://github.com/jmfirth/codequery/releases/download/v1.0.0/lang-elixir.tar.gz
  grammar.wasm    ✓
  extract.toml    ✓
  lsp.toml        ✓
Installed to ~/.local/share/cq/languages/elixir/
```

```bash
$ cq grammar list
```

```
Installed:
  elixir      Elixir/Phoenix (grammar + extract + lsp)
  ...

Available:
  rust        Rust systems programming language (grammar + extract + lsp + stack-graphs)
  typescript  TypeScript / JavaScript with types (grammar + extract + lsp + stack-graphs)
  python      Python (grammar + extract + lsp + stack-graphs)
  ...
```

```bash
$ cq grammar info elixir
```

```
Language:     Elixir
Description:  Elixir/Phoenix
Extensions:   .ex, .exs
Capabilities: grammar, extract, lsp
LSP server:   elixir-ls
Status:       installed
```

After install, all commands work on the new language:

```bash
$ cq outline hello.ex
```

```
@@ meta resolution=syntactic completeness=exhaustive total=3 @@

@@ hello.ex @@
  greet (function, pub) :2
  farewell (function, pub) :6
  Hello (module, pub) :1
```

---

## JSON output — Structured data with precision metadata

```bash
$ cq def detect_project_root --json --in crates/codequery-core
```

```json
{
  "resolution": "syntactic",
  "completeness": "exhaustive",
  "symbol": "detect_project_root",
  "definitions": [
    {
      "name": "detect_project_root",
      "kind": "function",
      "file": "crates/codequery-core/src/project.rs",
      "line": 33,
      "column": 0,
      "end_line": 52,
      "visibility": "pub",
      "children": [],
      "doc": "/// Detect the project root...",
      "body": "pub fn detect_project_root(start: &Path) -> Result<PathBuf> { ... }",
      "signature": "pub fn detect_project_root(start: &Path) -> Result<PathBuf>"
    }
  ],
  "total": 1
}
```

---

## Precision cascade

Every result carries its resolution tier so consumers know the confidence level:

| Tier | How | When |
|------|-----|------|
| `semantic` | LSP daemon (compiler-level) | `cq daemon` running |
| `resolved` | Stack graphs (import-aware) | 10 languages with TSG rules |
| `syntactic` | Tree-sitter name matching | Always available, all languages |

The cascade is automatic — `cq` uses the best available tier:

```
daemon running?  → semantic  (sub-second, compiler-level)
stack graph rules? → resolved  (follows imports, qualified names)
fallback          → syntactic (tree-sitter name matching)
```

---

## Performance

```bash
$ time cq def Symbol --in crates/codequery-core
```

Narrow commands complete in under 10ms. Wide commands under 2s on 400k lines with 8 cores.
