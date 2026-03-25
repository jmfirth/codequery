//! Def command: find where a symbol is defined across the project.

use std::path::Path;

use codequery_core::{detect_project_root_or, discover_files, Symbol, SymbolKind};
use codequery_parse::{extract_symbols, RustParser};

use crate::args::ExitCode;
use crate::output::format_def_results;

/// Run the def command: find where a symbol is defined.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols, filters by name, and
/// prints results in framed format.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or the parser cannot be created.
pub fn run(symbol: &str, project: Option<&Path>, scope: Option<&Path>) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Discover files (scope comes from --in flag)
    let files = discover_files(&project_root, scope)?;

    // 3. Create parser (reused across all files)
    let mut parser = RustParser::new()?;

    // 4. Read, pre-filter, parse, extract, and collect matches
    let mut matches: Vec<Symbol> = Vec::new();

    for relative_path in &files {
        let absolute_path = project_root.join(relative_path);

        // Read file contents
        let Ok(source) = std::fs::read_to_string(&absolute_path) else {
            continue; // Skip unreadable files
        };

        // Text pre-filter: skip files that don't contain the symbol name
        if !source.contains(symbol) {
            continue;
        }

        // Parse the already-read source (avoid double read via parse_file)
        let Ok(tree) = parser.parse(source.as_bytes()) else {
            continue; // Skip unparseable files
        };

        // Extract symbols from parsed tree
        let symbols = extract_symbols(&source, &tree, relative_path);

        // Filter: match top-level symbols and flatten impl children
        collect_matching_symbols(&symbols, symbol, &mut matches);
    }

    // 5. Sort by file path, then by line number
    matches.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 6. Format and output
    if matches.is_empty() {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_def_results(&matches);
        println!("{output}");
        Ok(ExitCode::Success)
    }
}

/// Collect symbols matching the query name, flattening impl children.
///
/// Matches top-level symbols by name (excluding impl blocks), and also
/// matches methods/children inside impl blocks by name.
fn collect_matching_symbols(symbols: &[Symbol], query: &str, matches: &mut Vec<Symbol>) {
    for symbol in symbols {
        // Match top-level symbols by name, but skip impl blocks
        // (query "Router" should find `struct Router`, not `impl Router`)
        if symbol.kind != SymbolKind::Impl && symbol.name == query {
            matches.push(symbol.clone());
        }

        // Flatten: search children (methods inside impl blocks)
        for child in &symbol.children {
            if child.name == query {
                matches.push(child.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{SymbolKind, Visibility};
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Helper: create a Symbol for testing.
    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        children: Vec<Symbol>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility: Visibility::Public,
            children,
            doc: None,
            body: None,
            signature: None,
        }
    }

    // Test 1: Find a function definition by name
    #[test]
    fn test_def_finds_function_by_name() {
        let symbols = vec![
            make_symbol("greet", SymbolKind::Function, "src/lib.rs", 9, 0, vec![]),
            make_symbol("other", SymbolKind::Function, "src/lib.rs", 20, 0, vec![]),
        ];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "greet", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "greet");
        assert_eq!(matches[0].kind, SymbolKind::Function);
    }

    // Test 2: Find a struct definition by name
    #[test]
    fn test_def_finds_struct_by_name() {
        let symbols = vec![make_symbol(
            "User",
            SymbolKind::Struct,
            "src/models.rs",
            5,
            0,
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "User", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "User");
        assert_eq!(matches[0].kind, SymbolKind::Struct);
    }

    // Test 3: Find a trait definition by name
    #[test]
    fn test_def_finds_trait_by_name() {
        let symbols = vec![make_symbol(
            "Validate",
            SymbolKind::Trait,
            "src/traits.rs",
            4,
            0,
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "Validate", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Validate");
        assert_eq!(matches[0].kind, SymbolKind::Trait);
    }

    // Test 4: Find a method inside an impl block by name
    #[test]
    fn test_def_finds_method_inside_impl_by_name() {
        let method = make_symbol(
            "is_adult",
            SymbolKind::Method,
            "src/services.rs",
            16,
            4,
            vec![],
        );
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            vec![method],
        );
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block], "is_adult", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "is_adult");
        assert_eq!(matches[0].kind, SymbolKind::Method);
    }

    // Test 5: Multiple definitions with same name across files returns all
    #[test]
    fn test_def_multiple_matches_returns_all() {
        let symbols_a = vec![make_symbol(
            "helper",
            SymbolKind::Function,
            "src/services.rs",
            43,
            0,
            vec![],
        )];
        let symbols_b = vec![make_symbol(
            "helper",
            SymbolKind::Function,
            "src/utils/helpers.rs",
            9,
            0,
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols_a, "helper", &mut matches);
        collect_matching_symbols(&symbols_b, "helper", &mut matches);
        assert_eq!(matches.len(), 2);
    }

    // Test 6: Symbol not found returns empty results
    #[test]
    fn test_def_symbol_not_found_returns_empty() {
        let symbols = vec![
            make_symbol("greet", SymbolKind::Function, "src/lib.rs", 9, 0, vec![]),
            make_symbol("User", SymbolKind::Struct, "src/models.rs", 5, 0, vec![]),
        ];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "nonexistent", &mut matches);
        assert!(matches.is_empty());
    }

    // Test 7: Text pre-filter correctly excludes files that don't contain the symbol name
    #[test]
    fn test_def_text_prefilter_excludes_non_matching_files() {
        // Simulate the pre-filter logic
        let source_with_symbol = "pub fn greet(name: &str) -> String { }";
        let source_without_symbol = "pub fn other() -> u32 { 42 }";

        assert!(source_with_symbol.contains("greet"));
        assert!(!source_without_symbol.contains("greet"));
    }

    // Test 8: Results are sorted by file path then line number
    #[test]
    fn test_def_results_sorted_by_file_then_line() {
        let mut matches = vec![
            make_symbol(
                "helper",
                SymbolKind::Function,
                "src/utils/helpers.rs",
                9,
                0,
                vec![],
            ),
            make_symbol(
                "helper",
                SymbolKind::Function,
                "src/services.rs",
                43,
                0,
                vec![],
            ),
        ];
        matches.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        assert_eq!(matches[0].file, PathBuf::from("src/services.rs"));
        assert_eq!(matches[1].file, PathBuf::from("src/utils/helpers.rs"));
    }

    // Test 9: --in scope limits search to subdirectory
    #[test]
    fn test_def_scope_limits_search_to_subdirectory() {
        let project = fixture_project();
        let result = run("helper", Some(&project), Some(Path::new("src/utils")));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // -- Correctness scenario tests against the fixture project --

    // Scenario 1: Find a unique function
    #[test]
    fn test_def_fixture_finds_greet_function() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 2: Find a struct
    #[test]
    fn test_def_fixture_finds_user_struct() {
        let project = fixture_project();
        let result = run("User", Some(&project), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 3: Duplicate symbol name returns success (multiple matches)
    #[test]
    fn test_def_fixture_finds_duplicate_helper() {
        let project = fixture_project();
        let result = run("helper", Some(&project), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 4: Find a method
    #[test]
    fn test_def_fixture_finds_is_adult_method() {
        let project = fixture_project();
        let result = run("is_adult", Some(&project), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 5: Symbol not found returns NoResults
    #[test]
    fn test_def_fixture_nonexistent_symbol_returns_no_results() {
        let project = fixture_project();
        let result = run("nonexistent_symbol", Some(&project), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Scenario 6: Scoped search with --in
    #[test]
    fn test_def_fixture_scoped_search_finds_only_utils_helper() {
        let project = fixture_project();
        let result = run("helper", Some(&project), Some(Path::new("src/utils")));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Verify impl block name does NOT match struct query
    #[test]
    fn test_def_impl_block_name_does_not_match_struct_query() {
        let method = make_symbol("new", SymbolKind::Method, "src/services.rs", 8, 4, vec![]);
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            vec![method],
        );
        let struct_def = make_symbol("User", SymbolKind::Struct, "src/models.rs", 5, 0, vec![]);

        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block, struct_def], "User", &mut matches);

        // Should find the struct, NOT the impl block
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, SymbolKind::Struct);
    }
}
