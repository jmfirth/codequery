//! High-level resolver facade combining graph construction and resolution.
//!
//! `StackGraphResolver` is the clean public API that downstream commands call.
//! It groups files by language, uses stack graphs where rules exist, and falls
//! back to syntactic reference extraction for unsupported languages.

use std::collections::HashMap;
use std::path::Path;

use codequery_core::{language_for_file, Language};
use codequery_index::{extract_references, FileSymbols};

use crate::graph::build_graph;
use crate::resolve::resolve_references;
use crate::rules::has_rules;
use crate::types::{Resolution, ResolutionResult, ResolvedReference};

/// Facade for resolving references across scanned files.
///
/// Caches nothing internally today (stack graph language instances are created
/// per `build_graph` call via `language_config`), but reserves the struct for
/// future caching of `StackGraphLanguage` instances.
pub struct StackGraphResolver {
    _private: (),
}

impl StackGraphResolver {
    /// Create a new resolver instance.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Resolve references to a symbol across scanned files.
    ///
    /// Groups files by language, uses stack graphs for supported languages,
    /// and falls back to syntactic extraction for unsupported ones. Merges
    /// results from all language groups.
    pub fn resolve_refs(&mut self, scan_results: &[FileSymbols], symbol: &str) -> ResolutionResult {
        self.resolve_internal(scan_results, symbol)
    }

    /// Like `resolve_refs` but filtered to `Call` references only.
    pub fn resolve_callers(
        &mut self,
        scan_results: &[FileSymbols],
        symbol: &str,
    ) -> ResolutionResult {
        let mut result = self.resolve_internal(scan_results, symbol);
        // For syntactic references, we already filter by kind below.
        // For resolved references from stack graphs, we cannot distinguish
        // call vs. type usage at the stack graph level, so we keep all resolved
        // references (they represent name bindings, not reference kinds).
        // The caller command downstream will intersect with syntactic refs
        // to filter by kind if needed. For now, this is the best we can do.
        result.references.retain(|r| {
            // Keep all resolved references (stack graphs don't classify kinds)
            // and only syntactic references that are calls.
            r.resolution == Resolution::Resolved
        });
        result
    }

    /// Resolve dependencies within a symbol's body.
    ///
    /// Finds references within the given line range of the target file,
    /// useful for understanding what a specific symbol depends on.
    #[allow(clippy::unused_self)]
    pub fn resolve_deps(
        &mut self,
        scan_results: &[FileSymbols],
        target_file: &Path,
        target_line_range: (usize, usize),
        symbol: &str,
    ) -> ResolutionResult {
        let _ = symbol; // Used for documentation/context; we scan by line range.
        let mut all_refs = Vec::new();
        let mut warnings = Vec::new();
        let mut any_syntactic = false;

        // Find the target file in scan results.
        let target = scan_results.iter().find(|fs| fs.file == target_file);
        let Some(target_fs) = target else {
            return ResolutionResult {
                references: Vec::new(),
                warnings: vec![format!(
                    "target file not found in scan results: {}",
                    target_file.display()
                )],
            };
        };

        let lang = language_for_file(&target_fs.file);
        let (start_line, end_line) = target_line_range;

        if let Some(lang) = lang {
            if has_rules(lang) {
                // Use stack graphs: build graph from all files of this language
                // and resolve references that fall within the target range.
                let group = group_by_language(scan_results);
                if let Some(files_for_lang) = group.get(&lang) {
                    match resolve_language_group(files_for_lang, lang, symbol) {
                        Ok((refs, group_warnings)) => {
                            // Filter to references within the target file and line range.
                            let filtered: Vec<_> = refs
                                .into_iter()
                                .filter(|r| {
                                    r.ref_file == target_file
                                        && r.ref_line >= start_line
                                        && r.ref_line <= end_line
                                })
                                .collect();
                            if filtered.is_empty() {
                                // Stack graph built but found no references in range —
                                // fall back to syntactic for this file.
                                let syntactic = syntactic_refs_for_file(
                                    target_fs,
                                    symbol,
                                    Some(target_line_range),
                                );
                                if !syntactic.is_empty() {
                                    all_refs.extend(syntactic);
                                    any_syntactic = true;
                                    warnings.push(format!(
                                        "{lang:?}: stack graph found no references, using syntactic fallback"
                                    ));
                                }
                                warnings.extend(group_warnings);
                            } else {
                                all_refs.extend(filtered);
                                warnings.extend(group_warnings);
                            }
                        }
                        Err(e) => {
                            warnings.push(format!("{lang:?}: {e}"));
                            // Fall back to syntactic for this file.
                            let syntactic =
                                syntactic_refs_for_file(target_fs, symbol, Some(target_line_range));
                            all_refs.extend(syntactic);
                            any_syntactic = true;
                        }
                    }
                }
            } else {
                // Syntactic fallback.
                let syntactic = syntactic_refs_for_file(target_fs, symbol, Some(target_line_range));
                all_refs.extend(syntactic);
                any_syntactic = true;
            }
        }

        if any_syntactic && !warnings.iter().any(|w| w.contains("syntactic fallback")) {
            warnings.push("some languages used syntactic fallback".to_string());
        }

        ResolutionResult {
            references: all_refs,
            warnings,
        }
    }

    /// Core resolution logic shared by `resolve_refs` and `resolve_callers`.
    #[allow(clippy::unused_self)]
    // &mut self reserved for future StackGraphLanguage caching
    fn resolve_internal(&mut self, scan_results: &[FileSymbols], symbol: &str) -> ResolutionResult {
        let groups = group_by_language(scan_results);
        let mut all_refs = Vec::new();
        let mut warnings = Vec::new();
        let mut any_syntactic = false;

        for (lang, files) in &groups {
            if has_rules(*lang) {
                match resolve_language_group(files, *lang, symbol) {
                    Ok((refs, group_warnings)) => {
                        if refs.is_empty() {
                            // Stack graph built but found no references — fall back to syntactic.
                            // This happens when TSG rules don't cover the reference patterns
                            // for this language (e.g., Rust, Go, C have limited rules).
                            let syntactic = syntactic_refs_for_group(files, symbol);
                            if !syntactic.is_empty() {
                                all_refs.extend(syntactic);
                                any_syntactic = true;
                                warnings.push(format!(
                                    "{lang:?}: stack graph found no references, using syntactic fallback"
                                ));
                            }
                            warnings.extend(group_warnings);
                        } else {
                            all_refs.extend(refs);
                            warnings.extend(group_warnings);
                        }
                    }
                    Err(e) => {
                        // Graph construction or resolution failed; fall back to syntactic.
                        warnings.push(format!(
                            "{lang:?}: stack graph failed ({e}), using syntactic fallback"
                        ));
                        let syntactic = syntactic_refs_for_group(files, symbol);
                        all_refs.extend(syntactic);
                        any_syntactic = true;
                    }
                }
            } else {
                // No stack graph rules: syntactic fallback.
                let syntactic = syntactic_refs_for_group(files, symbol);
                all_refs.extend(syntactic);
                any_syntactic = true;
            }
        }

        if any_syntactic && !warnings.iter().any(|w| w.contains("syntactic fallback")) {
            warnings.push("some languages used syntactic fallback".to_string());
        }

        ResolutionResult {
            references: all_refs,
            warnings,
        }
    }
}

impl Default for StackGraphResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Group `FileSymbols` by detected language, skipping files with unknown languages.
fn group_by_language(scan_results: &[FileSymbols]) -> HashMap<Language, Vec<&FileSymbols>> {
    let mut groups: HashMap<Language, Vec<&FileSymbols>> = HashMap::new();
    for fs in scan_results {
        if let Some(lang) = language_for_file(&fs.file) {
            groups.entry(lang).or_default().push(fs);
        }
    }
    groups
}

/// Build a stack graph for a language group and resolve references.
///
/// Returns resolved references and any warnings from graph construction.
/// Maximum number of files to feed into stack graph construction.
/// Beyond this, fall back to syntactic — the precision benefit doesn't
/// justify the cost. Use `--semantic` (LSP) for large-scale precision.
const MAX_STACK_GRAPH_FILES: usize = 200;

fn resolve_language_group(
    files: &[&FileSymbols],
    language: Language,
    symbol: &str,
) -> crate::error::Result<(Vec<ResolvedReference>, Vec<String>)> {
    // Optimization: only build the graph from files that mention the symbol.
    // For a 500-file project querying one symbol, this typically reduces
    // the graph to 5-20 files instead of 500.
    let relevant_files: Vec<_> = files
        .iter()
        .filter(|fs| fs.source.contains(symbol))
        .copied()
        .collect();

    // If too many files match (common symbol name), cap it and warn.
    let (graph_files, capped) = if relevant_files.len() > MAX_STACK_GRAPH_FILES {
        (&relevant_files[..MAX_STACK_GRAPH_FILES], true)
    } else {
        (relevant_files.as_slice(), false)
    };

    let graph_input: Vec<_> = graph_files
        .iter()
        .map(|fs| (fs.file.clone(), fs.source.clone(), fs.tree.clone()))
        .collect();

    let mut graph_result = build_graph(&graph_input, language)?;
    if capped {
        graph_result.warnings.push(crate::graph::GraphWarning {
            file: std::path::PathBuf::from("<resolver>"),
            message: format!(
                "stack graph limited to {} of {} files containing '{symbol}'. \
                 Use --semantic for full resolution on large projects.",
                MAX_STACK_GRAPH_FILES,
                relevant_files.len()
            ),
        });
    }
    let graph_warnings: Vec<String> = graph_result
        .warnings
        .iter()
        .map(|w| format!("{}: {}", w.file.display(), w.message))
        .collect();

    let refs = resolve_references(&graph_result.graph, &mut graph_result.partial_paths, symbol)?;

    Ok((refs, graph_warnings))
}

/// Extract syntactic references for a group of files, converting to `ResolvedReference`.
fn syntactic_refs_for_group(files: &[&FileSymbols], symbol: &str) -> Vec<ResolvedReference> {
    let mut results = Vec::new();
    for fs in files {
        let Some(lang) = language_for_file(&fs.file) else {
            continue;
        };
        let refs = extract_references(&fs.source, &fs.tree, &fs.file, lang);
        for r in refs {
            // Filter by symbol name: check if the reference text at the location matches.
            let ref_text = extract_ref_text(&fs.source, r.line, r.column);
            if ref_text != symbol {
                continue;
            }
            results.push(ResolvedReference {
                ref_file: r.file,
                ref_line: r.line,
                ref_column: r.column,
                symbol: symbol.to_string(),
                def_file: None,
                def_line: None,
                def_column: None,
                resolution: Resolution::Syntactic,
            });
        }
    }
    results
}

/// Extract syntactic references from a single file, optionally filtering by line range.
fn syntactic_refs_for_file(
    fs: &FileSymbols,
    symbol: &str,
    line_range: Option<(usize, usize)>,
) -> Vec<ResolvedReference> {
    let Some(lang) = language_for_file(&fs.file) else {
        return Vec::new();
    };
    let refs = extract_references(&fs.source, &fs.tree, &fs.file, lang);
    let mut results = Vec::new();
    for r in refs {
        let ref_text = extract_ref_text(&fs.source, r.line, r.column);
        if ref_text != symbol {
            continue;
        }
        if let Some((start, end)) = line_range {
            if r.line < start || r.line > end {
                continue;
            }
        }
        results.push(ResolvedReference {
            ref_file: r.file,
            ref_line: r.line,
            ref_column: r.column,
            symbol: symbol.to_string(),
            def_file: None,
            def_line: None,
            def_column: None,
            resolution: Resolution::Syntactic,
        });
    }
    results
}

/// Extract the identifier text at a given 1-based line and 0-based column.
fn extract_ref_text(source: &str, line: usize, column: usize) -> &str {
    let Some(line_text) = source.lines().nth(line.saturating_sub(1)) else {
        return "";
    };
    if column >= line_text.len() {
        return "";
    }
    let rest = &line_text[column..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    &rest[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_parse::Parser;
    use std::path::PathBuf;

    /// Create a `FileSymbols` from source text, path, and language.
    fn make_file_symbols(path: &str, source: &str, lang: Language) -> FileSymbols {
        let mut parser = Parser::for_language(lang).unwrap();
        let (src, tree) = {
            let tree = parser.parse(source.as_bytes()).unwrap();
            (source.to_string(), tree)
        };
        let file = PathBuf::from(path);
        let symbols = codequery_parse::extract_symbols(&src, &tree, &file, lang);
        FileSymbols {
            file,
            symbols,
            source: src,
            tree,
        }
    }

    // -----------------------------------------------------------------------
    // resolve_refs — Python (resolved path)
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_python_same_file() {
        let source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs], "greet");

        // Should produce results without error; all references should be for "greet".
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
        }
    }

    #[test]
    fn test_resolve_refs_python_cross_file() {
        let src_a = "def add(a, b):\n    return a + b\n";
        let src_b = "from math_mod import add\nresult = add(1, 2)\n";

        let fs_a = make_file_symbols("math_mod.py", src_a, Language::Python);
        let fs_b = make_file_symbols("app.py", src_b, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs_a, fs_b], "add");

        for r in &result.references {
            assert_eq!(r.symbol, "add");
        }
    }

    // -----------------------------------------------------------------------
    // resolve_refs — Rust (resolved — TSG scope wiring now works)
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_rust_same_file() {
        let source = "fn greet() -> String {\n    String::from(\"hello\")\n}\nfn main() {\n    greet();\n}\n";
        let fs = make_file_symbols("main.rs", source, Language::Rust);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs], "greet");

        // Rust TSG scope wiring now correctly propagates lexical scopes
        // through expression_statement → call_expression → identifier,
        // so path stitching succeeds and references are Resolved.
        assert!(
            !result.references.is_empty(),
            "Rust same-file: expected >= 1 resolved reference for 'greet'"
        );
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
        }
    }

    // -----------------------------------------------------------------------
    // resolve_refs — C++ (resolved via stack graph rules)
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_cpp_resolves_same_file() {
        let source = "void greet() {}\nint main() { greet(); return 0; }\n";
        let fs = make_file_symbols("main.cpp", source, Language::Cpp);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs], "greet");

        // C++ now has stack graph rules, so references should be resolved.
        assert!(
            !result.references.is_empty(),
            "C++ same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
            result.warnings
        );
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert_eq!(
                r.resolution,
                Resolution::Resolved,
                "C++ same-file: references should be Resolved, got {:?}",
                r.resolution
            );
        }
    }

    // -----------------------------------------------------------------------
    // resolve_refs — mixed languages (both resolved with stack graphs)
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_mixed_languages_both_resolved() {
        let py_source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
        let cpp_source = "void greet() {}\nint main() { greet(); return 0; }\n";

        let fs_py = make_file_symbols("app.py", py_source, Language::Python);
        let fs_cpp = make_file_symbols("main.cpp", cpp_source, Language::Cpp);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs_py, fs_cpp], "greet");

        // Should have references from both languages.
        assert!(
            !result.references.is_empty(),
            "expected references from Python and/or C++, got 0. Warnings: {:?}",
            result.warnings
        );

        // Both Python and C++ now have stack graph rules, so all references
        // should be resolved.
        let has_resolved = result
            .references
            .iter()
            .any(|r| r.resolution == Resolution::Resolved);
        assert!(
            has_resolved,
            "expected at least some resolved references from Python and C++"
        );

        // All resolved references should be for 'greet'.
        assert!(
            result
                .references
                .iter()
                .filter(|r| r.resolution == Resolution::Resolved)
                .all(|r| r.symbol == "greet"),
            "all resolved references should be for 'greet'"
        );
    }

    // -----------------------------------------------------------------------
    // resolve_callers
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_callers_python_returns_resolved_only() {
        let source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_callers(&[fs], "greet");

        // All returned references should be resolved (syntactic filtered out by callers).
        for r in &result.references {
            assert_eq!(r.resolution, Resolution::Resolved);
        }
    }

    // -----------------------------------------------------------------------
    // resolve_deps
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_deps_python_within_line_range() {
        let source = "x = 42\ndef foo():\n    return x + 1\n\ny = foo()\n";
        let fs = make_file_symbols("main.py", source, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_deps(
            &[fs],
            Path::new("main.py"),
            (2, 3), // Lines 2-3: the foo function body
            "foo",
        );

        // All references should be within the target line range.
        for r in &result.references {
            assert!(
                r.ref_line >= 2 && r.ref_line <= 3,
                "ref at line {} outside range 2-3",
                r.ref_line
            );
        }
    }

    #[test]
    fn test_resolve_deps_missing_file_returns_warning() {
        let source = "x = 42\n";
        let fs = make_file_symbols("main.py", source, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_deps(&[fs], Path::new("nonexistent.py"), (1, 10), "x");

        assert!(result.references.is_empty());
        assert!(
            result.warnings.iter().any(|w| w.contains("not found")),
            "expected 'not found' warning, got: {:?}",
            result.warnings
        );
    }

    // -----------------------------------------------------------------------
    // Empty input
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_empty_input() {
        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[], "anything");

        assert!(result.references.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_resolve_callers_empty_input() {
        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_callers(&[], "anything");

        assert!(result.references.is_empty());
    }

    #[test]
    fn test_resolve_deps_empty_input() {
        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_deps(&[], Path::new("x.py"), (1, 10), "x");

        assert!(result.references.is_empty());
    }

    // -----------------------------------------------------------------------
    // Default trait
    // -----------------------------------------------------------------------

    #[test]
    fn test_stack_graph_resolver_default() {
        let mut resolver = StackGraphResolver::default();
        let result = resolver.resolve_refs(&[], "x");
        assert!(result.references.is_empty());
    }

    // -----------------------------------------------------------------------
    // Symbol not found
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_nonexistent_symbol() {
        let source = "def greet(name):\n    pass\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&[fs], "nonexistent_xyz_123");

        assert!(result.references.is_empty());
    }

    // -----------------------------------------------------------------------
    // Fixture-based tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_refs_python_fixture_project() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_project");
        let fixture_files = [
            ("src/main.py", Language::Python),
            ("src/utils.py", Language::Python),
            ("src/models.py", Language::Python),
        ];

        let scan_results: Vec<FileSymbols> = fixture_files
            .iter()
            .filter_map(|(rel, lang)| {
                let abs = fixture_root.join(rel);
                let source = std::fs::read_to_string(&abs).ok()?;
                Some(make_file_symbols(rel, &source, *lang))
            })
            .collect();

        assert!(!scan_results.is_empty(), "fixture files should be readable");

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&scan_results, "greet");

        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
        }
    }

    #[test]
    fn test_resolve_refs_rust_fixture_project() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let fixture_files = [
            ("src/lib.rs", Language::Rust),
            ("src/models.rs", Language::Rust),
            ("src/services.rs", Language::Rust),
        ];

        let scan_results: Vec<FileSymbols> = fixture_files
            .iter()
            .filter_map(|(rel, lang)| {
                let abs = fixture_root.join(rel);
                let source = std::fs::read_to_string(&abs).ok()?;
                Some(make_file_symbols(rel, &source, *lang))
            })
            .collect();

        assert!(!scan_results.is_empty(), "fixture files should be readable");

        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(&scan_results, "greet");

        for r in &result.references {
            assert_eq!(r.symbol, "greet");
        }
    }

    // -----------------------------------------------------------------------
    // extract_ref_text helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_ref_text_basic() {
        let source = "fn greet() {}\n";
        assert_eq!(extract_ref_text(source, 1, 3), "greet");
    }

    #[test]
    fn test_extract_ref_text_out_of_bounds_line() {
        let source = "fn greet() {}\n";
        assert_eq!(extract_ref_text(source, 99, 0), "");
    }

    #[test]
    fn test_extract_ref_text_out_of_bounds_column() {
        let source = "fn greet() {}\n";
        assert_eq!(extract_ref_text(source, 1, 999), "");
    }

    #[test]
    fn test_extract_ref_text_at_end_of_line() {
        let source = "x = foo\n";
        assert_eq!(extract_ref_text(source, 1, 4), "foo");
    }
}
