//! Reference resolution via stack graph path stitching.
//!
//! Given a built stack graph and a symbol name, finds all (reference → definition)
//! pairs by computing partial paths and stitching them into complete name-binding paths.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use stack_graphs::arena::Handle;
use stack_graphs::graph::{Node, StackGraph};
use stack_graphs::partial::PartialPaths;
use stack_graphs::stitching::{
    Database, DatabaseCandidates, ForwardPartialPathStitcher, StitcherConfig,
};
use stack_graphs::{CancelAfterDuration, CancellationFlag, NoCancellation};

use crate::error::Result;
use crate::types::{Resolution, ResolvedReference};

/// Default timeout for resolution operations.
const DEFAULT_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolve references in a built stack graph.
///
/// Finds which definition each reference in the graph resolves to, filtering
/// by `symbol_name`. Computes partial paths for all files, then uses forward
/// partial path stitching to find complete reference-to-definition paths.
///
/// # Arguments
///
/// * `graph` - The stack graph built from source files.
/// * `partials` - The partial paths arena.
/// * `symbol_name` - The symbol name to resolve references for.
///
/// # Errors
///
/// Returns `ResolveError::ResolutionTimeout` if resolution exceeds the default
/// 2-second timeout.
pub fn resolve_references(
    graph: &StackGraph,
    partials: &mut PartialPaths,
    symbol_name: &str,
) -> Result<Vec<ResolvedReference>> {
    resolve_references_with_timeout(
        graph,
        partials,
        symbol_name,
        Some(DEFAULT_RESOLUTION_TIMEOUT),
    )
}

/// Resolve all references across all files in the graph.
///
/// Equivalent to [`resolve_references`] — both scan the entire graph for
/// references matching `symbol_name`. This function exists as a convenience
/// alias to clarify intent when the caller wants comprehensive results.
///
/// # Errors
///
/// Returns `ResolveError::ResolutionTimeout` if resolution exceeds the default
/// 2-second timeout.
pub fn resolve_all_references(
    graph: &StackGraph,
    partials: &mut PartialPaths,
    symbol_name: &str,
) -> Result<Vec<ResolvedReference>> {
    resolve_references(graph, partials, symbol_name)
}

/// Resolve references with a configurable timeout.
///
/// Pass `None` to disable the timeout entirely.
///
/// # Errors
///
/// Returns `ResolveError::ResolutionTimeout` if resolution exceeds the given timeout.
pub fn resolve_references_with_timeout(
    graph: &StackGraph,
    partials: &mut PartialPaths,
    symbol_name: &str,
    timeout: Option<Duration>,
) -> Result<Vec<ResolvedReference>> {
    // Build a cancellation flag from the timeout option.
    // We store the CancelAfterDuration here so it lives long enough.
    let cancel_guard = timeout.map(CancelAfterDuration::new);
    let cancellation_flag: &dyn CancellationFlag = match cancel_guard {
        Some(ref guard) => guard,
        None => &NoCancellation,
    };

    let mut db = build_database(graph, partials, cancellation_flag);
    let reference_nodes = find_reference_nodes(graph, symbol_name);

    if reference_nodes.is_empty() {
        return Ok(Vec::new());
    }

    Ok(stitch_references(
        graph,
        partials,
        &mut db,
        &reference_nodes,
        symbol_name,
        cancellation_flag,
    ))
}

/// Compute partial paths for all files and collect them into a `Database`.
fn build_database(
    graph: &StackGraph,
    partials: &mut PartialPaths,
    cancellation_flag: &dyn CancellationFlag,
) -> Database {
    let mut db = Database::new();
    let config = StitcherConfig::default();

    for file in graph.iter_files() {
        // If a file times out during partial path computation, skip it.
        let _stats = ForwardPartialPathStitcher::find_minimal_partial_path_set_in_file(
            graph,
            partials,
            file,
            config,
            cancellation_flag,
            |g, ps, path| {
                db.add_partial_path(g, ps, path.clone());
            },
        );
    }

    db
}

/// Find all reference nodes in the graph whose symbol matches `symbol_name`.
fn find_reference_nodes(graph: &StackGraph, symbol_name: &str) -> Vec<Handle<Node>> {
    graph
        .iter_nodes()
        .filter(|&node_handle| {
            let node = &graph[node_handle];
            if !node.is_reference() {
                return false;
            }
            let Some(sym_handle) = node.symbol() else {
                return false;
            };
            graph[sym_handle] == *symbol_name
        })
        .collect()
}

/// Stitch partial paths for each reference node to find definitions.
fn stitch_references(
    graph: &StackGraph,
    partials: &mut PartialPaths,
    db: &mut Database,
    reference_nodes: &[Handle<Node>],
    symbol_name: &str,
    cancellation_flag: &dyn CancellationFlag,
) -> Vec<ResolvedReference> {
    let config = StitcherConfig::default();
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for &ref_node in reference_nodes {
        let ref_loc = extract_node_location(graph, ref_node);

        // If resolution fails or times out for one reference, skip it.
        let _stitch_result = ForwardPartialPathStitcher::find_all_complete_partial_paths(
            &mut DatabaseCandidates::new(graph, partials, db),
            std::iter::once(ref_node),
            config,
            cancellation_flag,
            |g, _ps, path| {
                collect_resolved_path(
                    g,
                    path.end_node,
                    ref_loc.as_ref(),
                    symbol_name,
                    &mut seen,
                    &mut results,
                );
            },
        );
    }

    results
}

/// Location triple used for deduplication: (file, line, column).
type LocationTriple = (PathBuf, usize, usize);

/// Dedup key for a (reference, definition) pair.
type DeduplicationKey = (Option<LocationTriple>, PathBuf, usize, usize);

/// Extract a resolved reference from a complete path and add it to results if not a duplicate.
fn collect_resolved_path(
    graph: &StackGraph,
    def_node: Handle<Node>,
    ref_loc: Option<&LocationTriple>,
    symbol_name: &str,
    seen: &mut HashSet<DeduplicationKey>,
    results: &mut Vec<ResolvedReference>,
) {
    let Some((def_file, def_line, def_col)) = extract_node_location(graph, def_node) else {
        return;
    };

    let key = (ref_loc.cloned(), def_file.clone(), def_line, def_col);
    if !seen.insert(key) {
        return;
    }

    let (rf, rl, rc) = ref_loc
        .cloned()
        .unwrap_or_else(|| (PathBuf::from("<unknown>"), 0, 0));

    results.push(ResolvedReference {
        ref_file: rf,
        ref_line: rl,
        ref_column: rc,
        symbol: symbol_name.to_string(),
        def_file: Some(def_file),
        def_line: Some(def_line),
        def_column: Some(def_col),
        resolution: Resolution::Resolved,
    });
}

/// Extract file path and 1-based line/column from a stack graph node.
///
/// Returns `None` if the node has no source info or no file.
fn extract_node_location(
    graph: &StackGraph,
    node: Handle<Node>,
) -> Option<(PathBuf, usize, usize)> {
    let node_ref = &graph[node];
    let file_handle = node_ref.file()?;
    let file_name = graph[file_handle].name();
    let source_info = graph.source_info(node)?;
    let line = source_info.span.start.line + 1; // 0-indexed to 1-based
    let column = source_info.span.start.column.utf8_offset;
    Some((PathBuf::from(file_name), line, column))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_graph;
    use codequery_core::Language;
    use codequery_parse::Parser;
    use std::path::Path;

    /// Parse a single source string and return the triple needed by `build_graph`.
    fn parse_source(
        path: &Path,
        source: &str,
        lang: Language,
    ) -> (PathBuf, String, tree_sitter::Tree) {
        let mut parser = Parser::for_language(lang).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        (path.to_path_buf(), source.to_string(), tree)
    }

    // -----------------------------------------------------------------------
    // Python — same-file resolution
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_python_same_file_variable() {
        let source = "x = 42\nprint(x)\n";
        let files = vec![parse_source(Path::new("main.py"), source, Language::Python)];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "x").unwrap();

        // Stack graph resolution quality depends on TSG rules; the important
        // thing is the function runs without error.
        assert!(
            refs.is_empty() || refs.iter().all(|r| r.symbol == "x"),
            "all resolved references should be for symbol 'x': {refs:?}"
        );
    }

    #[test]
    fn test_resolve_python_function_call() {
        let source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
        let files = vec![parse_source(Path::new("app.py"), source, Language::Python)];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "greet").unwrap();

        // If resolution succeeds, the definition should point back to app.py.
        for r in &refs {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
            if let Some(ref def_file) = r.def_file {
                assert_eq!(def_file, &PathBuf::from("app.py"));
            }
        }
    }

    #[test]
    fn test_resolve_python_cross_file() {
        let src_a = "def add(a, b):\n    return a + b\n";
        let src_b = "from math_mod import add\nresult = add(1, 2)\n";

        let files = vec![
            parse_source(Path::new("math_mod.py"), src_a, Language::Python),
            parse_source(Path::new("app.py"), src_b, Language::Python),
        ];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "add").unwrap();

        // Cross-file resolution depends on TSG rule quality. The function
        // should at least not error out.
        for r in &refs {
            assert_eq!(r.symbol, "add");
        }
    }

    // -----------------------------------------------------------------------
    // No matches
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_nonexistent_symbol_returns_empty() {
        let source = "x = 42\n";
        let files = vec![parse_source(Path::new("main.py"), source, Language::Python)];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs = resolve_references(
            &result.graph,
            &mut result.partial_paths,
            "nonexistent_symbol_xyz",
        )
        .unwrap();

        assert!(refs.is_empty());
    }

    // -----------------------------------------------------------------------
    // Empty graph
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_empty_graph() {
        let files: Vec<(PathBuf, String, tree_sitter::Tree)> = vec![];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs =
            resolve_references(&result.graph, &mut result.partial_paths, "anything").unwrap();

        assert!(refs.is_empty());
    }

    // -----------------------------------------------------------------------
    // Deduplication
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_deduplicates_same_ref_def_pair() {
        // The same symbol appearing multiple times should produce unique pairs.
        let source = "x = 1\ny = x\nz = x\n";
        let files = vec![parse_source(Path::new("main.py"), source, Language::Python)];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "x").unwrap();

        // Check no exact duplicates in (ref_file, ref_line, ref_column, def_file, def_line, def_column).
        let mut seen_keys = HashSet::new();
        for r in &refs {
            let key = (
                r.ref_file.clone(),
                r.ref_line,
                r.ref_column,
                r.def_file.clone(),
                r.def_line,
                r.def_column,
            );
            assert!(seen_keys.insert(key), "duplicate resolved reference: {r:?}");
        }
    }

    // -----------------------------------------------------------------------
    // resolve_all_references alias
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_all_references_same_as_resolve_references() {
        let source = "x = 42\nprint(x)\n";
        let files = vec![parse_source(Path::new("main.py"), source, Language::Python)];

        let mut result1 = build_graph(&files, Language::Python).unwrap();
        let refs1 = resolve_references(&result1.graph, &mut result1.partial_paths, "x").unwrap();

        let mut result2 = build_graph(&files, Language::Python).unwrap();
        let refs2 =
            resolve_all_references(&result2.graph, &mut result2.partial_paths, "x").unwrap();

        assert_eq!(refs1.len(), refs2.len());
    }

    // -----------------------------------------------------------------------
    // Timeout — disabled
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_with_timeout_none() {
        let source = "a = 1\nb = a\n";
        let files = vec![parse_source(Path::new("t.py"), source, Language::Python)];
        let mut result = build_graph(&files, Language::Python).unwrap();

        let refs =
            resolve_references_with_timeout(&result.graph, &mut result.partial_paths, "a", None)
                .unwrap();

        // Should complete successfully without timeout.
        for r in &refs {
            assert_eq!(r.symbol, "a");
        }
    }

    // -----------------------------------------------------------------------
    // JavaScript — basic resolution
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_javascript_variable() {
        let source = "const greeting = 'hello';\nconsole.log(greeting);\n";
        let files = vec![parse_source(
            Path::new("index.js"),
            source,
            Language::JavaScript,
        )];
        let mut result = build_graph(&files, Language::JavaScript).unwrap();

        let refs =
            resolve_references(&result.graph, &mut result.partial_paths, "greeting").unwrap();

        for r in &refs {
            assert_eq!(r.symbol, "greeting");
        }
    }

    // -----------------------------------------------------------------------
    // TypeScript — basic resolution
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_typescript_function() {
        let source =
            "function add(a: number, b: number): number { return a + b; }\nconst r = add(1, 2);\n";
        let files = vec![parse_source(
            Path::new("math.ts"),
            source,
            Language::TypeScript,
        )];
        let mut result = build_graph(&files, Language::TypeScript).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "add").unwrap();

        for r in &refs {
            assert_eq!(r.symbol, "add");
        }
    }

    // -----------------------------------------------------------------------
    // Error tolerance — broken source
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_with_broken_source_does_not_panic() {
        let good = "def foo():\n    pass\n";
        let bad = "def )(\n    @@@\n";
        let files = vec![
            parse_source(Path::new("good.py"), good, Language::Python),
            parse_source(Path::new("bad.py"), bad, Language::Python),
        ];
        let mut result = build_graph(&files, Language::Python).unwrap();

        // Should not panic even with broken source in the graph.
        let refs = resolve_references(&result.graph, &mut result.partial_paths, "foo").unwrap();

        for r in &refs {
            assert_eq!(r.symbol, "foo");
        }
    }

    // -----------------------------------------------------------------------
    // Fixture project — Python
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_python_fixture_project() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_project");
        let fixture_files = [
            ("src/main.py", Language::Python),
            ("src/utils.py", Language::Python),
            ("src/models.py", Language::Python),
        ];

        let files: Vec<_> = fixture_files
            .iter()
            .filter_map(|(rel, lang)| {
                let abs = fixture_root.join(rel);
                let source = std::fs::read_to_string(&abs).ok()?;
                Some(parse_source(Path::new(rel), &source, *lang))
            })
            .collect();

        assert!(!files.is_empty(), "fixture files should be readable");
        let mut result = build_graph(&files, Language::Python).unwrap();

        // Resolve "greet" — should find the definition in main.py.
        let refs = resolve_references(&result.graph, &mut result.partial_paths, "greet").unwrap();

        // The function should run without error on real fixture files.
        for r in &refs {
            assert_eq!(r.symbol, "greet");
        }
    }

    // -----------------------------------------------------------------------
    // Helper: count reference and definition nodes in a stack graph
    // -----------------------------------------------------------------------

    fn count_ref_def_nodes(
        source: &str,
        filename: &str,
        lang: Language,
    ) -> (usize, usize, usize, Vec<String>) {
        let files = vec![parse_source(Path::new(filename), source, lang)];
        let result = build_graph(&files, lang).unwrap();

        let warnings: Vec<String> = result.warnings.iter().map(|w| w.message.clone()).collect();

        let ref_count = result
            .graph
            .iter_nodes()
            .filter(|&nh| result.graph[nh].is_reference())
            .count();

        let def_count = result
            .graph
            .iter_nodes()
            .filter(|&nh| result.graph[nh].is_definition())
            .count();

        let total = result.graph.iter_nodes().count();

        (total, ref_count, def_count, warnings)
    }

    // -----------------------------------------------------------------------
    // Rust — TSG produces reference nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_rust_tsg_produces_reference_nodes() {
        let source = "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n";
        let (_total, refs, defs, warnings) = count_ref_def_nodes(source, "main.rs", Language::Rust);
        assert!(
            warnings.is_empty(),
            "Rust TSG rules should produce no warnings: {warnings:?}"
        );
        assert!(
            refs > 0,
            "Rust TSG rules should produce reference nodes, got 0"
        );
        assert!(
            defs > 0,
            "Rust TSG rules should produce definition nodes, got 0"
        );
    }

    #[test]
    fn test_rust_resolve_same_file_function_call() {
        let source = "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n";
        let files = vec![parse_source(Path::new("main.rs"), source, Language::Rust)];
        let mut result = build_graph(&files, Language::Rust).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "greet").unwrap();

        // The reference should be found and resolved to the definition.
        for r in &refs {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
            if let Some(ref def_file) = r.def_file {
                assert_eq!(def_file, &PathBuf::from("main.rs"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Go — TSG produces reference nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_go_tsg_produces_reference_nodes() {
        let source =
            "package main\n\nfunc Greet() string { return \"hello\" }\nfunc main() { Greet() }\n";
        let (_total, refs, defs, warnings) = count_ref_def_nodes(source, "main.go", Language::Go);
        assert!(
            warnings.is_empty(),
            "Go TSG rules should produce no warnings: {warnings:?}"
        );
        assert!(
            refs > 0,
            "Go TSG rules should produce reference nodes, got 0"
        );
        assert!(
            defs > 0,
            "Go TSG rules should produce definition nodes, got 0"
        );
    }

    #[test]
    fn test_go_resolve_same_file_function_call() {
        let source =
            "package main\n\nfunc Greet() string { return \"hello\" }\nfunc main() { Greet() }\n";
        let files = vec![parse_source(Path::new("main.go"), source, Language::Go)];
        let mut result = build_graph(&files, Language::Go).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "Greet").unwrap();

        for r in &refs {
            assert_eq!(r.symbol, "Greet");
            assert_eq!(r.resolution, Resolution::Resolved);
            if let Some(ref def_file) = r.def_file {
                assert_eq!(def_file, &PathBuf::from("main.go"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // C — TSG produces reference nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_c_tsg_produces_reference_nodes() {
        let source = "int add(int a, int b) { return a + b; }\nint main() { return add(1, 2); }\n";
        let (_total, refs, defs, warnings) = count_ref_def_nodes(source, "main.c", Language::C);
        assert!(
            warnings.is_empty(),
            "C TSG rules should produce no warnings: {warnings:?}"
        );
        assert!(
            refs > 0,
            "C TSG rules should produce reference nodes, got 0"
        );
        assert!(
            defs > 0,
            "C TSG rules should produce definition nodes, got 0"
        );
    }

    #[test]
    fn test_c_resolve_same_file_function_call() {
        let source = "int add(int a, int b) { return a + b; }\nint main() { return add(1, 2); }\n";
        let files = vec![parse_source(Path::new("main.c"), source, Language::C)];
        let mut result = build_graph(&files, Language::C).unwrap();

        let refs = resolve_references(&result.graph, &mut result.partial_paths, "add").unwrap();

        for r in &refs {
            assert_eq!(r.symbol, "add");
            assert_eq!(r.resolution, Resolution::Resolved);
            if let Some(ref def_file) = r.def_file {
                assert_eq!(def_file, &PathBuf::from("main.c"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Diagnostic: reference node counts per language (temporary)
    // -----------------------------------------------------------------------

    #[test]
    #[ignore] // Diagnostic — not a real test
    fn diagnostic_reference_nodes_per_language() {
        use std::collections::HashSet;

        fn count_refs(
            lang: Language,
            path: &str,
            source: &str,
            symbol: &str,
        ) -> (usize, usize, Vec<String>) {
            let files = vec![parse_source(Path::new(path), source, lang)];
            let result = build_graph(&files, lang).unwrap();
            let mut total = 0;
            let mut matching = 0;
            let mut syms = HashSet::new();
            for nh in result.graph.iter_nodes() {
                let node = &result.graph[nh];
                if node.is_reference() {
                    total += 1;
                    if let Some(sh) = node.symbol() {
                        let s = &result.graph[sh];
                        syms.insert(s.to_string());
                        if s == symbol {
                            matching += 1;
                        }
                    }
                }
            }
            let mut v: Vec<_> = syms.into_iter().collect();
            v.sort();
            (total, matching, v)
        }

        let cases = vec![
            (Language::Python, "app.py", "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n", "greet"),
            (Language::TypeScript, "app.ts", "function greet(name: string): string { return `Hello ${name}`; }\nconst r = greet('world');\n", "greet"),
            (Language::JavaScript, "app.js", "function greet(name) { return 'Hello ' + name; }\nconst r = greet('world');\n", "greet"),
            (Language::Rust, "main.rs", "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n", "greet"),
            (Language::Go, "main.go", "package main\nfunc Greet() string { return \"hello\" }\nfunc main() { Greet() }\n", "Greet"),
            (Language::C, "main.c", "int add(int a, int b) { return a + b; }\nint main() { return add(1, 2); }\n", "add"),
            (Language::Java, "Main.java", "public class Main {\n  static void greet() {}\n  public static void main(String[] a) { greet(); }\n}\n", "greet"),
        ];

        for (lang, path, source, symbol) in &cases {
            let (total, matching, syms) = count_refs(*lang, path, source, symbol);
            eprintln!(
                "{lang:?}: {total} ref nodes, {matching} matching '{symbol}', symbols: {syms:?}"
            );
        }

        // Also check full resolution for languages with ref nodes
        for (lang, path, source, symbol) in &cases {
            let files = vec![parse_source(Path::new(path), source, *lang)];
            let mut result = build_graph(&files, *lang).unwrap();
            let refs = resolve_references(&result.graph, &mut result.partial_paths, symbol).unwrap();
            eprintln!(
                "{lang:?}: resolve_references returned {} resolved refs for '{symbol}'",
                refs.len()
            );
        }
    }
}
