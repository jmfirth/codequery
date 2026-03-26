//! Sig command: extract the type signature of a symbol without its body.

use std::path::Path;

use codequery_core::{
    detect_project_root_or, discover_files, language_for_file, Symbol, SymbolKind,
};
use codequery_parse::{extract_symbols, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_sig;

/// Run the sig command: extract the signature of a symbol.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols, filters by name, and
/// prints signature results in the requested output mode.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or the parser cannot be created.
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Discover files (scope comes from --in flag)
    let files = discover_files(&project_root, scope)?;

    // 3. Read, pre-filter, parse, extract, and collect matches
    let mut matches: Vec<Symbol> = Vec::new();

    // Track the current parser language so we can reuse parsers across
    // files of the same language.
    let mut current_parser: Option<(codequery_core::Language, Parser)> = None;

    for relative_path in &files {
        let absolute_path = project_root.join(relative_path);

        // Detect language for this file
        let Some(language) = language_for_file(relative_path) else {
            continue; // Skip files with unrecognized extensions
        };

        // Read file contents
        let Ok(source) = std::fs::read_to_string(&absolute_path) else {
            continue; // Skip unreadable files
        };

        // Text pre-filter: skip files that don't contain the symbol name
        if !source.contains(symbol) {
            continue;
        }

        // Reuse parser if same language, otherwise create a new one
        let parser = match &mut current_parser {
            Some((lang, p)) if *lang == language => p,
            _ => {
                current_parser = Some((language, Parser::for_language(language)?));
                &mut current_parser.as_mut().expect("just assigned").1
            }
        };

        // Parse the already-read source (avoid double read via parse_file)
        let Ok(tree) = parser.parse(source.as_bytes()) else {
            continue; // Skip unparseable files
        };

        // Extract symbols from parsed tree
        let symbols = extract_symbols(&source, &tree, relative_path, language);

        // Filter: match top-level symbols and flatten impl children
        collect_matching_symbols(&symbols, symbol, &mut matches);
    }

    // 4. Sort by file path, then by line number
    matches.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 5. Format and output
    if matches.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_sig(&matches, symbol, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if matches.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

/// Collect symbols matching the query name, flattening impl children.
///
/// Matches top-level symbols by name (excluding impl blocks), and also
/// matches methods/children inside impl blocks by name.
fn collect_matching_symbols(symbols: &[Symbol], query: &str, matches: &mut Vec<Symbol>) {
    for symbol in symbols {
        // Match top-level symbols by name, but skip impl blocks
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
        signature: Option<&str>,
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
            signature: signature.map(String::from),
        }
    }

    // Test 1: Function signature shows declaration without body
    #[test]
    fn test_sig_function_shows_declaration_without_body() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            vec![],
            Some("pub fn greet(name: &str) -> String"),
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "greet", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].signature.as_deref(),
            Some("pub fn greet(name: &str) -> String")
        );
    }

    // Test 2: Struct signature shows field list
    #[test]
    fn test_sig_struct_shows_field_list() {
        let symbols = vec![make_symbol(
            "User",
            SymbolKind::Struct,
            "src/models.rs",
            5,
            0,
            vec![],
            Some("pub struct User {\n    pub name: String,\n    pub age: u32,\n}"),
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "User", &mut matches);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].signature.as_deref().unwrap().contains("User"));
    }

    // Test 3: Trait signature shows method signatures
    #[test]
    fn test_sig_trait_shows_method_signatures() {
        let symbols = vec![make_symbol(
            "Validate",
            SymbolKind::Trait,
            "src/traits.rs",
            4,
            0,
            vec![],
            Some("pub trait Validate {\n    fn is_valid(&self) -> bool;\n    fn errors(&self) -> Vec<String>;\n}"),
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "Validate", &mut matches);
        assert_eq!(matches.len(), 1);
        assert!(matches[0]
            .signature
            .as_deref()
            .unwrap()
            .contains("Validate"));
    }

    // Test 4: Symbol not found returns empty results
    #[test]
    fn test_sig_symbol_not_found_returns_empty() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            vec![],
            Some("pub fn greet(name: &str) -> String"),
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "nonexistent", &mut matches);
        assert!(matches.is_empty());
    }

    // Test 5: Method inside impl found by name
    #[test]
    fn test_sig_finds_method_inside_impl() {
        let method = make_symbol(
            "is_adult",
            SymbolKind::Method,
            "src/services.rs",
            16,
            4,
            vec![],
            Some("pub fn is_adult(&self) -> bool"),
        );
        let impl_block = Symbol {
            name: "User".to_string(),
            kind: SymbolKind::Impl,
            file: PathBuf::from("src/services.rs"),
            line: 6,
            column: 0,
            end_line: 23,
            visibility: Visibility::Public,
            children: vec![method],
            doc: None,
            body: None,
            signature: None,
        };
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block], "is_adult", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].signature.as_deref(),
            Some("pub fn is_adult(&self) -> bool")
        );
    }

    // Test 6: Fixture — function signature found
    #[test]
    fn test_sig_fixture_finds_greet_function() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 7: Fixture — struct signature found
    #[test]
    fn test_sig_fixture_finds_user_struct() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 8: Fixture — trait signature found
    #[test]
    fn test_sig_fixture_finds_validate_trait() {
        let project = fixture_project();
        let result = run("Validate", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 9: Fixture — nonexistent symbol returns NoResults
    #[test]
    fn test_sig_fixture_nonexistent_symbol_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 10: JSON mode returns success
    #[test]
    fn test_sig_json_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Json, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 11: JSON mode with no results returns NoResults
    #[test]
    fn test_sig_json_mode_no_results_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
            Some(&project),
            None,
            OutputMode::Json,
            true,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 12: Raw mode returns success
    #[test]
    fn test_sig_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Raw, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 13: --in scope limits search
    #[test]
    fn test_sig_scope_limits_search_to_subdirectory() {
        let project = fixture_project();
        let result = run(
            "helper",
            Some(&project),
            Some(Path::new("src/utils")),
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
