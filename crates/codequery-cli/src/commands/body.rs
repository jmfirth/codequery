//! Body command: extract the full source body of a symbol.

use std::path::Path;

use codequery_core::{
    detect_project_root_or, discover_files, language_for_file, Symbol, SymbolKind,
};
use codequery_parse::{extract_symbols, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_body;

/// Run the body command: extract the full source body of a symbol.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols (with body text populated),
/// filters by name, and prints bodies in the requested output mode.
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
        let output = format_body(&matches, symbol, mode, pretty);
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
        body: Option<&str>,
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
            body: body.map(String::from),
            signature: None,
        }
    }

    // Test 1: Find a function body by name
    #[test]
    fn test_body_finds_function_body() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Some("pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}"),
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "greet", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "greet");
        assert!(matches[0].body.is_some());
    }

    // Test 2: Find a struct body by name
    #[test]
    fn test_body_finds_struct_body() {
        let symbols = vec![make_symbol(
            "User",
            SymbolKind::Struct,
            "src/models.rs",
            5,
            0,
            Some("pub struct User {\n    pub name: String,\n    pub age: u32,\n}"),
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "User", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "User");
        assert!(matches[0].body.is_some());
    }

    // Test 3: Nonexistent symbol returns empty
    #[test]
    fn test_body_nonexistent_symbol_returns_empty() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Some("pub fn greet() {}"),
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "nonexistent", &mut matches);
        assert!(matches.is_empty());
    }

    // Test 4: Method inside impl block is found
    #[test]
    fn test_body_finds_method_in_impl() {
        let method = make_symbol(
            "is_adult",
            SymbolKind::Method,
            "src/services.rs",
            16,
            4,
            Some("pub fn is_adult(&self) -> bool {\n        self.age >= 18\n    }"),
            vec![],
        );
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            None,
            vec![method],
        );
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block], "is_adult", &mut matches);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].body.is_some());
    }

    // Test 5: Multiple matches returns all
    #[test]
    fn test_body_multiple_matches_returns_all() {
        let symbols_a = vec![make_symbol(
            "helper",
            SymbolKind::Function,
            "src/services.rs",
            43,
            0,
            Some("pub fn helper() -> &'static str {\n    \"services helper\"\n}"),
            vec![],
        )];
        let symbols_b = vec![make_symbol(
            "helper",
            SymbolKind::Function,
            "src/utils/helpers.rs",
            3,
            0,
            Some("pub fn helper() -> &'static str {\n    \"utils helper\"\n}"),
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols_a, "helper", &mut matches);
        collect_matching_symbols(&symbols_b, "helper", &mut matches);
        assert_eq!(matches.len(), 2);
    }

    // Test 6: Impl block name does NOT match
    #[test]
    fn test_body_impl_block_name_does_not_match() {
        let method = make_symbol(
            "new",
            SymbolKind::Method,
            "src/services.rs",
            8,
            4,
            Some("pub fn new() {}"),
            vec![],
        );
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            None,
            vec![method],
        );
        let struct_def = make_symbol(
            "User",
            SymbolKind::Struct,
            "src/models.rs",
            5,
            0,
            Some("pub struct User {}"),
            vec![],
        );
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block, struct_def], "User", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, SymbolKind::Struct);
    }

    // -- Correctness scenario tests against the fixture project --

    // Scenario 1: cq body greet returns function body with source text
    #[test]
    fn test_body_fixture_greet_returns_body() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 2: cq body User returns struct body
    #[test]
    fn test_body_fixture_user_returns_body() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 3: cq body nonexistent returns exit code 1
    #[test]
    fn test_body_fixture_nonexistent_returns_no_results() {
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

    // Scenario 4: --json includes body/signature/doc fields
    #[test]
    fn test_body_fixture_json_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Json, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 5: --raw outputs body text only
    #[test]
    fn test_body_fixture_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Raw, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Scenario 6: --in scoping works
    #[test]
    fn test_body_fixture_scope_limits_search() {
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

    // Scenario 7: Multiple matches — all bodies returned
    #[test]
    fn test_body_fixture_multiple_matches() {
        let project = fixture_project();
        let result = run("helper", Some(&project), None, OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // JSON mode with no results
    #[test]
    fn test_body_json_no_results() {
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
}
