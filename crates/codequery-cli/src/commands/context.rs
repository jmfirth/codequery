//! Context command: find the enclosing symbol for a given `file:line` location.

use std::path::Path;

use codequery_core::{detect_project_root_or, language_name_for_file, Symbol};
use codequery_parse::{extract_symbols_by_name, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_context_output;

/// Run the context command: find the enclosing symbol for a line.
///
/// Parses the `file:line` location argument, reads and parses the file,
/// extracts symbols, finds the innermost symbol whose line range contains
/// the target line, and prints the result with a line marker.
///
/// # Errors
///
/// Returns an error if the parser cannot be created (language grammar failure).
pub fn run(
    location: &str,
    project: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    depth: Option<usize>,
) -> anyhow::Result<ExitCode> {
    // 1. Parse file:line argument
    let Some((file_str, target_line)) = parse_location(location) else {
        eprintln!("error: invalid location format, expected file:line");
        return Ok(ExitCode::UsageError);
    };

    let file = Path::new(file_str);

    // 2. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 3. Validate file exists — resolve relative paths against cwd
    let absolute_file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };

    if !absolute_file.exists() {
        eprintln!("error: file not found: {}", file.display());
        return Ok(ExitCode::ProjectError);
    }

    // 4. Detect language from file extension
    let Some(lang_name) = language_name_for_file(&absolute_file) else {
        eprintln!("error: unsupported file type: {}", absolute_file.display());
        return Ok(ExitCode::ProjectError);
    };

    // 5. Compute relative path for display
    let relative_path = absolute_file
        .canonicalize()?
        .strip_prefix(project_root.canonicalize()?)
        .map_or_else(|_| file.to_path_buf(), Path::to_path_buf);

    // 6. Parse
    let mut parser = Parser::for_name(&lang_name)?;
    let (source, tree) = match parser.parse_file(&absolute_file) {
        Ok(result) => result,
        Err(codequery_parse::ParseError::Io(e)) => {
            eprintln!("error: cannot read file: {e}");
            return Ok(ExitCode::ProjectError);
        }
        Err(e) => return Err(e.into()),
    };

    let has_parse_errors = tree.root_node().has_error();

    // 7. Extract symbols
    let symbols = extract_symbols_by_name(&source, &tree, &relative_path, &lang_name);

    // 8. Find enclosing symbol(s)
    let context_chain = find_enclosing_chain(&symbols, target_line);

    if context_chain.is_empty() {
        if mode == OutputMode::Json {
            let output = format_context_output(None, target_line, &relative_path, mode, pretty);
            println!("{output}");
        }
        return Ok(ExitCode::NoResults);
    }

    // Apply depth: if depth is specified, take only the last N levels
    let chain = if let Some(d) = depth {
        let start = context_chain.len().saturating_sub(d);
        &context_chain[start..]
    } else {
        &context_chain[context_chain.len() - 1..]
    };

    // The innermost symbol is always the last in the chain
    let innermost = chain.last().expect("chain is non-empty");

    // 9. Format and output
    let output = format_context_output(Some(innermost), target_line, &relative_path, mode, pretty);
    println!("{output}");

    if has_parse_errors {
        Ok(ExitCode::ParseWarning)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Parse a `file:line` location string by splitting on the last `:`.
///
/// Returns `None` if the format is invalid (no `:`, or line is not a number).
fn parse_location(location: &str) -> Option<(&str, usize)> {
    let colon_pos = location.rfind(':')?;
    if colon_pos == 0 {
        return None;
    }
    let file_part = &location[..colon_pos];
    let line_part = &location[colon_pos + 1..];
    let line: usize = line_part.parse().ok()?;
    if line == 0 {
        return None; // Lines are 1-based
    }
    Some((file_part, line))
}

/// Find the chain of enclosing symbols from outermost to innermost.
///
/// Walks all symbols (including children) and builds a chain of nested
/// symbols whose line ranges contain the target line. The last element
/// is the innermost (deepest) enclosing symbol.
fn find_enclosing_chain(symbols: &[Symbol], target_line: usize) -> Vec<&Symbol> {
    fn walk_symbols<'a>(
        symbols: &'a [Symbol],
        target_line: usize,
        current_chain: &mut Vec<&'a Symbol>,
        best_chain: &mut Vec<&'a Symbol>,
    ) {
        for symbol in symbols {
            if symbol.line <= target_line && target_line <= symbol.end_line {
                current_chain.push(symbol);
                // This symbol contains the target. Check if it's deeper than current best.
                if current_chain.len() > best_chain.len() {
                    best_chain.clear();
                    best_chain.extend(current_chain.iter());
                }
                // Recurse into children
                walk_symbols(&symbol.children, target_line, current_chain, best_chain);
                current_chain.pop();
            }
        }
    }

    let mut best_chain: Vec<&Symbol> = Vec::new();
    let mut current_chain: Vec<&Symbol> = Vec::new();
    walk_symbols(symbols, target_line, &mut current_chain, &mut best_chain);
    best_chain
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
        end_line: usize,
        column: usize,
        children: Vec<Symbol>,
        body: Option<&str>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line,
            visibility: Visibility::Public,
            children,
            doc: None,
            body: body.map(String::from),
            signature: None,
        }
    }

    // -----------------------------------------------------------------------
    // parse_location tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_context_parse_location_valid() {
        let result = parse_location("src/main.rs:42");
        assert_eq!(result, Some(("src/main.rs", 42)));
    }

    #[test]
    fn test_context_parse_location_windows_path_with_colon() {
        // Should split on the last colon
        let result = parse_location("C:/src/main.rs:10");
        assert_eq!(result, Some(("C:/src/main.rs", 10)));
    }

    #[test]
    fn test_context_parse_location_no_colon_returns_none() {
        let result = parse_location("src/main.rs");
        assert!(result.is_none());
    }

    #[test]
    fn test_context_parse_location_non_numeric_line_returns_none() {
        let result = parse_location("src/main.rs:abc");
        assert!(result.is_none());
    }

    #[test]
    fn test_context_parse_location_zero_line_returns_none() {
        let result = parse_location("src/main.rs:0");
        assert!(result.is_none());
    }

    #[test]
    fn test_context_parse_location_empty_file_part_returns_none() {
        let result = parse_location(":42");
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // find_enclosing_chain tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_context_find_function_containing_line() {
        let symbols = vec![
            make_symbol(
                "foo",
                SymbolKind::Function,
                "test.rs",
                1,
                5,
                0,
                vec![],
                None,
            ),
            make_symbol(
                "bar",
                SymbolKind::Function,
                "test.rs",
                10,
                15,
                0,
                vec![],
                None,
            ),
        ];
        let chain = find_enclosing_chain(&symbols, 3);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "foo");
    }

    #[test]
    fn test_context_find_method_inside_impl_containing_line() {
        let method = make_symbol(
            "do_thing",
            SymbolKind::Method,
            "test.rs",
            5,
            8,
            4,
            vec![],
            None,
        );
        let impl_block = make_symbol(
            "MyStruct",
            SymbolKind::Impl,
            "test.rs",
            3,
            10,
            0,
            vec![method],
            None,
        );
        let symbols = vec![impl_block];
        let chain = find_enclosing_chain(&symbols, 6);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "MyStruct");
        assert_eq!(chain[1].name, "do_thing");
    }

    #[test]
    fn test_context_line_outside_any_symbol_returns_empty() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "test.rs",
            5,
            10,
            0,
            vec![],
            None,
        )];
        let chain = find_enclosing_chain(&symbols, 3);
        assert!(chain.is_empty());
    }

    #[test]
    fn test_context_line_at_symbol_start_boundary() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "test.rs",
            5,
            10,
            0,
            vec![],
            None,
        )];
        let chain = find_enclosing_chain(&symbols, 5);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "foo");
    }

    #[test]
    fn test_context_line_at_symbol_end_boundary() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "test.rs",
            5,
            10,
            0,
            vec![],
            None,
        )];
        let chain = find_enclosing_chain(&symbols, 10);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "foo");
    }

    #[test]
    fn test_context_innermost_selected_for_nested_symbols() {
        let inner = make_symbol(
            "inner",
            SymbolKind::Method,
            "test.rs",
            5,
            8,
            4,
            vec![],
            None,
        );
        let outer = make_symbol(
            "outer",
            SymbolKind::Impl,
            "test.rs",
            3,
            10,
            0,
            vec![inner],
            None,
        );
        // The chain should include both, with innermost last
        let symbols = vec![outer];
        let chain = find_enclosing_chain(&symbols, 6);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "outer");
        assert_eq!(chain[1].name, "inner");
    }

    // -----------------------------------------------------------------------
    // Integration tests against fixture project
    // -----------------------------------------------------------------------

    // Test: Find function containing a line within it
    #[test]
    fn test_context_fixture_finds_greet_function() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:10", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: Find method inside impl containing the target line
    #[test]
    fn test_context_fixture_finds_method_in_impl() {
        let project = fixture_project();
        let file = project.join("src/services.rs");
        // Line 17 is inside is_adult method
        let location = format!("{}:17", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: Line outside any symbol returns no results
    #[test]
    fn test_context_fixture_line_outside_symbols_returns_no_results() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        // Line 1 is the module doc comment, outside any function
        let location = format!("{}:1", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test: Invalid file returns error
    #[test]
    fn test_context_nonexistent_file_returns_project_error() {
        let project = fixture_project();
        let file = project.join("src/nonexistent.rs");
        let location = format!("{}:10", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    // Test: Invalid location format
    #[test]
    fn test_context_invalid_location_returns_usage_error() {
        let project = fixture_project();
        let result = run(
            "not-a-location",
            Some(&project),
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::UsageError);
    }

    // Test: JSON output mode
    #[test]
    fn test_context_json_mode_returns_success() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:10", file.display());
        let result = run(&location, Some(&project), OutputMode::Json, true, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: Raw output mode
    #[test]
    fn test_context_raw_mode_returns_success() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:10", file.display());
        let result = run(&location, Some(&project), OutputMode::Raw, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: JSON output when no results
    #[test]
    fn test_context_json_mode_no_results_still_outputs() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:1", file.display());
        let result = run(&location, Some(&project), OutputMode::Json, true, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test: depth flag limits nesting levels
    #[test]
    fn test_context_depth_flag_works() {
        let project = fixture_project();
        let file = project.join("src/services.rs");
        // Line 17 is inside is_adult method inside impl User
        let location = format!("{}:17", file.display());
        // With depth=1, should still find the innermost symbol
        let result = run(
            &location,
            Some(&project),
            OutputMode::Framed,
            false,
            Some(1),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
