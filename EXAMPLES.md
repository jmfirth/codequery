# cq Examples

Real output from running `cq` against its own codebase.

---

## outline — What's in this file?

```bash
$ cq outline crates/codequery-core/src/symbol.rs
```

```
@@ crates/codequery-core/src/symbol.rs @@
  Symbol (struct, pub) :8
  SymbolKind (enum, pub) :39
  fmt::Display for SymbolKind (impl, priv) :68
    fmt (method, priv) :69
  Location (struct, pub) :91
  Visibility (enum, pub) :102
  fmt::Display for Visibility (impl, priv) :114
    fmt (method, priv) :115
  tests (module, priv) :126
```

---

## def — Where is this symbol defined?

```bash
$ cq def SymbolKind
```

```
@@ crates/codequery-core/src/symbol.rs:39:0 enum SymbolKind @@
```

Scoped search narrows results:

```bash
$ cq def extract_symbols --in crates/codequery-parse/src
```

```
@@ crates/codequery-parse/src/extract.rs:40:0 function extract_symbols @@

@@ crates/codequery-parse/src/languages/bash.rs:17:4 method extract_symbols @@

@@ crates/codequery-parse/src/languages/c.rs:17:4 method extract_symbols @@

@@ crates/codequery-parse/src/languages/cpp.rs:18:4 method extract_symbols @@

...
```

---

## body — Get the full source of a function

```bash
$ cq body detect_project_root --in crates/codequery-core
```

```
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

## sig — Type signature without the implementation

```bash
$ cq sig LanguageExtractor
```

```
@@ crates/codequery-parse/src/languages.rs:31:0 trait LanguageExtractor @@
pub trait LanguageExtractor {
    /// Extract all symbol definitions from a parsed source file.
    ///
    /// # Arguments
    /// * `source` — the source text (needed to extract node text via byte ranges)
    /// * `tree` — the parsed tree-sitter tree
    /// * `file` — the file path (stored in each `Symbol` for output)
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol>;
}
```

```bash
$ cq sig Symbol --in crates/codequery-core
```

```
@@ crates/codequery-core/src/symbol.rs:8:0 struct Symbol @@
pub struct Symbol {
    /// The symbol's name as it appears in source code.
    pub name: String,
    /// What kind of symbol this is (function, struct, etc.).
    pub kind: SymbolKind,
    /// The file path containing this symbol.
    pub file: PathBuf,
    /// The 1-based starting line number.
    pub line: usize,
    /// The 0-based starting column number.
    pub column: usize,
    /// The 1-based ending line number.
    pub end_line: usize,
    /// The visibility of this symbol.
    pub visibility: Visibility,
    /// Child symbols (e.g., methods inside an impl block).
    pub children: Vec<Symbol>,
    /// Documentation comment attached to this symbol, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Full source text of the symbol body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Signature/header only (no body).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}
```

---

## context — What function contains this line?

Given a compiler error at line 45 of extract.rs, find the enclosing function:

```bash
$ cq context crates/codequery-parse/src/extract.rs:45
```

```
@@ crates/codequery-parse/src/extract.rs:40:0 function extract_symbols (contains line 45) @@
pub fn extract_symbols(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,
) -> Vec<Symbol> {    // <- line 45
    match language {
        Language::Rust => RustExtractor::extract_symbols(source, tree, file),
        Language::Python => PythonExtractor::extract_symbols(source, tree, file),
        Language::Go => GoExtractor::extract_symbols(source, tree, file),
        Language::Java => JavaExtractor::extract_symbols(source, tree, file),
        Language::TypeScript | Language::JavaScript => {
            TypeScriptExtractor::extract_symbols(source, tree, file)
        }
        Language::C => CExtractor::extract_symbols(source, tree, file),
        Language::Cpp => CppExtractor::extract_symbols(source, tree, file),
        Language::Ruby => RubyExtractor::extract_symbols(source, tree, file),
        Language::Php => PhpExtractor::extract_symbols(source, tree, file),
        Language::CSharp => CSharpExtractor::extract_symbols(source, tree, file),
        Language::Swift => SwiftExtractor::extract_symbols(source, tree, file),
        Language::Kotlin => KotlinExtractor::extract_symbols(source, tree, file),
        Language::Scala => ScalaExtractor::extract_symbols(source, tree, file),
        Language::Zig => ZigExtractor::extract_symbols(source, tree, file),
        Language::Lua => LuaExtractor::extract_symbols(source, tree, file),
        Language::Bash => BashExtractor::extract_symbols(source, tree, file),
    }
}
```

---

## imports — What does this file depend on?

```bash
$ cq imports crates/codequery-cli/src/commands/refs.rs
```

```
@@ crates/codequery-cli/src/commands/refs.rs @@
  @@ crates/codequery-cli/src/commands/refs.rs:3 use std::path::Path @@
  @@ crates/codequery-cli/src/commands/refs.rs:5 use codequery_core::{detect_project_root_or, Reference, ReferenceKind, Resolution, Symbol} @@
  @@ crates/codequery-cli/src/commands/refs.rs:6 use codequery_index::{extract_references, scan_project_cached, SymbolIndex} @@
  @@ crates/codequery-cli/src/commands/refs.rs:7 use codequery_lsp::resolve_with_cascade @@
  @@ crates/codequery-cli/src/commands/refs.rs:8 use codequery_resolve::StackGraphResolver @@
  @@ crates/codequery-cli/src/commands/refs.rs:10 use crate::args::{ExitCode, OutputMode} @@
  @@ crates/codequery-cli/src/commands/refs.rs:11 use crate::output::format_refs @@
```

---

## tree — Project structure at a glance

```bash
$ cq tree crates/codequery-core/src --depth 0
```

```
@@ crates/codequery-core/src @@
crates/codequery-core/src/config.rs
  ProjectConfig (struct, pub) :14
  LspConfig (struct, pub) :29
  LspServerOverride (struct, pub) :41
  ConfigFile (struct, priv) :50
  ProjectSection (struct, priv) :58
  LspSection (struct, priv) :68
  LspServerOverrideToml (struct, priv) :76
  load_config (function, pub) :91
  tests (module, priv) :142
crates/codequery-core/src/discovery.rs
  Language (enum, pub) :14
  Language (impl, priv) :53
  language_for_file (function, pub) :88
  language_for_file_with_overrides (function, pub) :98
  discover_files (function, pub) :142
  discover_files_with_config (function, pub) :193
  build_exclude_matchers (function, priv) :244
  is_excluded (function, priv) :252
  tests (module, priv) :258
crates/codequery-core/src/error.rs
  CoreError (enum, pub) :7
  Result (type, pub) :30
  tests (module, priv) :33
crates/codequery-core/src/lib.rs
  config (module, pub) :9
  discovery (module, pub) :10
  error (module, pub) :11
  path_utils (module, pub) :12
  project (module, pub) :13
  query (module, pub) :14
  reference (module, pub) :15
  symbol (module, pub) :16
crates/codequery-core/src/path_utils.rs
  resolve_display_path (function, pub) :14
  tests (module, priv) :20
crates/codequery-core/src/project.rs
  MARKERS (const, priv) :10
  detect_project_root (function, pub) :33
  detect_project_root_or (function, pub) :63
  tests (module, priv) :85
crates/codequery-core/src/query.rs
  Resolution (enum, pub) :12
  Completeness (enum, pub) :24
  QueryResult (struct, pub) :36
  tests (module, priv) :50
crates/codequery-core/src/reference.rs
  Reference (struct, pub) :13
  ReferenceKind (enum, pub) :35
  fmt::Display for ReferenceKind (impl, priv) :46
  tests (module, priv) :59
crates/codequery-core/src/symbol.rs
  Symbol (struct, pub) :8
  SymbolKind (enum, pub) :39
  fmt::Display for SymbolKind (impl, priv) :68
  Location (struct, pub) :91
  Visibility (enum, pub) :102
  fmt::Display for Visibility (impl, priv) :114
  tests (module, priv) :126
```

---

## symbols — Find all symbols of a kind

```bash
$ cq symbols --kind trait
```

```
@@ crates/codequery-parse/src/languages.rs:31:0 trait LanguageExtractor @@
@@ tests/fixtures/php_project/src/models.php:24:0 trait Loggable @@
@@ tests/fixtures/rust_project/src/traits.rs:4:0 trait Validate @@
@@ tests/fixtures/rust_project/src/traits.rs:15:0 trait Summary @@
@@ tests/fixtures/scala_project/Main.scala:9:0 trait Drawable @@
@@ tests/fixtures/scala_project/Main.scala:29:0 trait Guarded @@
```

---

## refs — Who references this symbol?

```bash
$ cq refs greet --project tests/fixtures/rust_project
```

```
@@ src/lib.rs:9:0 function greet (definition) @@

0 references (syntactic match -- may be incomplete)
```

The `best-effort` label is doing its job here. The syntactic reference extractor finds cross-file references by name matching but has known limitations in Rust (the strongest extraction is via the LSP cascade). With `cq daemon` running, this upgrades to `resolution: "semantic"` and returns the actual call sites from `tests/integration.rs`.

---

## callers — Who calls this function?

Same precision model as refs, filtered to call sites only:

```bash
$ cq callers greet --project tests/fixtures/rust_project
```

```
@@ src/lib.rs:9:0 function greet (definition) @@

0 callers (syntactic match -- may be incomplete)
```

The daemon/LSP cascade is where callers reaches its full potential — type-resolved call hierarchy from the language server.

---

## deps — What does this function depend on?

```bash
$ cq deps process_users --project tests/fixtures/rust_project
```

```
@@ src/services.rs:38:0 function process_users @@
  User (type_reference) -> src/models.rs
  Vec (type_reference) -> <unresolved>
  String (type_reference) -> <unresolved>
  collect (call) -> <unresolved>
  map (call) -> <unresolved>
  iter (call) -> <unresolved>
  summarize (call) -> src/services.rs
```

`<unresolved>` means the dependency is from the standard library or an external crate -- not defined in the project. `User` resolves to `src/models.rs`, `summarize` resolves to a method in `src/services.rs`.

With `--json`, each dependency carries its own resolution metadata:

```json
{
  "resolution": "syntactic",
  "completeness": "best_effort",
  "symbol": "process_users",
  "dependencies": [
    { "name": "User", "kind": "type_reference", "defined_in": "src/models.rs", "resolution": "syntactic" },
    { "name": "summarize", "kind": "call", "defined_in": "src/services.rs", "resolution": "syntactic" }
  ]
}
```

---

## search — Structural pattern matching (raw S-expression)

Find all functions that return `Result`:

```bash
$ cq search --raw '(function_item name: (identifier) @name return_type: (generic_type type: (type_identifier) @ret (#eq? @ret "Result")))' --in crates/codequery-core
```

```
@@ crates/codequery-core/src/config.rs:90:7 @@
load_config

@@ crates/codequery-core/src/discovery.rs:141:7 @@
discover_files

@@ crates/codequery-core/src/discovery.rs:192:7 @@
discover_files_with_config

@@ crates/codequery-core/src/project.rs:32:7 @@
detect_project_root

@@ crates/codequery-core/src/project.rs:62:7 @@
detect_project_root_or
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
            "doc": "/// Detect the project root by walking up from `start` looking for marker files/dirs.\n///\n/// Markers are checked in priority order at each directory level. First match wins.\n/// Returns `Err(CoreError::ProjectNotFound)` if no marker is found before the filesystem root.\n///\n/// # Errors\n///\n/// Returns `CoreError::Path` if `start` cannot be canonicalized.\n/// Returns `CoreError::ProjectNotFound` if no marker is found walking up to the filesystem root.",
            "body": "pub fn detect_project_root(start: &Path) -> Result<PathBuf> {\n    let canonical = start\n        .canonicalize()\n        .map_err(|e| CoreError::Path(format!(\"cannot canonicalize {}: {e}\", start.display())))?;\n\n    let mut current = canonical.as_path();\n\n    loop {\n        for marker in MARKERS {\n            if current.join(marker).exists() {\n                return Ok(current.to_path_buf());\n            }\n        }\n\n        match current.parent() {\n            Some(parent) => current = parent,\n            None => return Err(CoreError::ProjectNotFound(start.to_path_buf())),\n        }\n    }\n}",
            "signature": "pub fn detect_project_root(start: &Path) -> Result<PathBuf>"
        }
    ],
    "total": 1
}
```

---

## Performance

```bash
$ time cq def Symbol --in crates/codequery-core
```

```
@@ crates/codequery-core/src/symbol.rs:8:0 struct Symbol @@

real    0.007s
```

Narrow commands complete in under 10ms on this codebase. The specification target is sub-100ms on any project size.
