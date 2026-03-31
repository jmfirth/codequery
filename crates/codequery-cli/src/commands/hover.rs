//! Hover command: show type info, docs, and signature at a source location.
//!
//! Parses a `file:line[:col]` location, runs AST-level type extraction and
//! symbol lookup, and emits a [`HoverInfo`] in the requested output mode.
//!
//! The LSP / daemon tier is not implemented here; this is the pure AST fallback.

use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, Completeness, HoverInfo, QueryResult, Resolution,
};
use codequery_parse::{extract_symbols, extract_type_at_position, Parser};
use serde::Serialize;
use std::io::IsTerminal;

use crate::args::{ExitCode, OutputMode};

/// Run the hover command.
///
/// Parses the `file:line[:col]` location, extracts type info from the AST,
/// looks up the enclosing symbol for doc/signature data, builds a
/// [`HoverInfo`], and formats the result.
///
/// # Errors
///
/// Returns an error if the parser cannot be created (language grammar failure).
pub fn run(
    location: &str,
    project: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    _use_semantic: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Parse file:line[:col] argument
    let Some((file_str, target_line, target_col)) = parse_location(location) else {
        eprintln!("error: invalid location format, expected file:line[:col]");
        return Ok(ExitCode::UsageError);
    };

    let file = Path::new(file_str);

    // 2. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 3. Resolve absolute path
    let absolute_file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };

    if !absolute_file.exists() {
        eprintln!("error: file not found: {}", file.display());
        return Ok(ExitCode::ProjectError);
    }

    // 4. Detect language
    let Some(language) = language_for_file(&absolute_file) else {
        eprintln!("error: unsupported file type: {}", absolute_file.display());
        return Ok(ExitCode::ProjectError);
    };

    // 5. Compute relative path for display
    let relative_path = absolute_file
        .canonicalize()?
        .strip_prefix(project_root.canonicalize()?)
        .map_or_else(|_| file.to_path_buf(), Path::to_path_buf);

    // 6. Parse the file
    let mut parser = Parser::for_language(language)?;
    let (source, tree) = match parser.parse_file(&absolute_file) {
        Ok(result) => result,
        Err(codequery_parse::ParseError::Io(e)) => {
            eprintln!("error: cannot read file: {e}");
            return Ok(ExitCode::ProjectError);
        }
        Err(e) => return Err(e.into()),
    };

    let has_parse_errors = tree.root_node().has_error();

    // 7. AST type extraction
    let type_info = extract_type_at_position(&source, &tree, target_line, target_col, language);

    // 8. Symbol lookup: find the innermost symbol whose range contains the position
    let symbols = extract_symbols(&source, &tree, &relative_path, language);
    let enclosing = find_enclosing_symbol(&symbols, target_line);

    let docs = enclosing.and_then(|s| s.doc.clone());
    let signature = enclosing.and_then(|s| s.signature.clone());

    // 9. Bail if there's nothing to show
    if type_info.is_none() && docs.is_none() && signature.is_none() {
        if mode == OutputMode::Json {
            // Emit a minimal JSON result so callers get well-formed output
            let info = HoverInfo {
                type_info: None,
                docs: None,
                signature: None,
                file: relative_path,
                line: target_line,
                column: target_col,
            };
            let output = format_hover_output(&info, mode, pretty);
            println!("{output}");
        }
        return Ok(ExitCode::NoResults);
    }

    // 10. Build HoverInfo
    let info = HoverInfo {
        type_info,
        docs,
        signature,
        file: relative_path,
        line: target_line,
        column: target_col,
    };

    // 11. Format and output
    let output = format_hover_output(&info, mode, pretty);
    println!("{output}");

    if has_parse_errors {
        Ok(ExitCode::ParseWarning)
    } else {
        Ok(ExitCode::Success)
    }
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `hover` command.
#[derive(Debug, Serialize)]
pub struct HoverResult {
    /// The hover information at the queried location.
    pub hover: HoverInfo,
}

/// Format hover results in the requested output mode.
pub fn format_hover_output(info: &HoverInfo, mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_hover_framed(info),
        OutputMode::Json => format_hover_json(info, pretty),
        OutputMode::Raw => format_hover_raw(info),
    }
}

/// Format hover info as framed output.
///
/// Each non-empty field gets its own `@@ ... @@` section header.
/// Sections are separated by blank lines.
fn format_hover_framed(info: &HoverInfo) -> String {
    let total = [&info.type_info, &info.signature, &info.docs]
        .iter()
        .filter(|f| f.is_some())
        .count();
    let meta = format!(
        "@@ meta resolution={} completeness={} total={} @@",
        Resolution::Syntactic,
        Completeness::Exhaustive,
        total,
    );

    let mut sections: Vec<String> = Vec::new();

    if let Some(ref t) = info.type_info {
        sections.push(format!(
            "@@ {}:{}:{} type @@\n{}",
            info.file.display(),
            info.line,
            info.column,
            t,
        ));
    }

    if let Some(ref sig) = info.signature {
        sections.push(format!(
            "@@ {}:{}:{} signature @@\n{}",
            info.file.display(),
            info.line,
            info.column,
            sig,
        ));
    }

    if let Some(ref doc) = info.docs {
        sections.push(format!(
            "@@ {}:{}:{} docs @@\n{}",
            info.file.display(),
            info.line,
            info.column,
            doc,
        ));
    }

    let content = sections.join("\n\n");
    if content.is_empty() {
        meta
    } else {
        format!("{meta}\n\n{content}")
    }
}

/// Format hover info as JSON wrapped in `QueryResult`.
fn format_hover_json(info: &HoverInfo, force_pretty: bool) -> String {
    let data = HoverResult {
        hover: info.clone(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    let use_pretty = force_pretty || std::io::stdout().is_terminal();
    if use_pretty {
        serde_json::to_string_pretty(&result).unwrap_or_default()
    } else {
        serde_json::to_string(&result).unwrap_or_default()
    }
}

/// Format hover info as raw text.
///
/// Emits the type info if available, otherwise the signature, otherwise the
/// doc comment. Returns an empty string if all fields are `None`.
fn format_hover_raw(info: &HoverInfo) -> String {
    let total = [&info.type_info, &info.signature, &info.docs]
        .iter()
        .filter(|f| f.is_some())
        .count();
    let meta = format!(
        "# meta resolution={} completeness={} total={}",
        Resolution::Syntactic,
        Completeness::Exhaustive,
        total,
    );

    let content = if let Some(ref t) = info.type_info {
        t.clone()
    } else if let Some(ref sig) = info.signature {
        sig.clone()
    } else if let Some(ref doc) = info.docs {
        doc.clone()
    } else {
        String::new()
    };

    if content.is_empty() {
        meta
    } else {
        format!("{meta}\n{content}")
    }
}

/// Parse a `file:line` or `file:line:col` location string.
///
/// For `file:line`, column defaults to 0.
/// For `file:line:col`, all three parts are returned.
///
/// Uses `rfind` on `:` to handle Windows-style drive letters (e.g. `C:\...`).
/// Line is 1-based; returns `None` if line == 0 or any part is invalid.
pub(crate) fn parse_location(location: &str) -> Option<(&str, usize, usize)> {
    // Try file:line:col first (rfind gives us the last colon — the column part)
    let last_colon = location.rfind(':')?;
    if last_colon == 0 {
        return None;
    }

    let last_part = &location[last_colon + 1..];

    // If the last segment is a valid number, check whether the preceding part
    // is itself a `file:line` pair (three-part) or just a file path (two-part).
    if let Ok(last_num) = last_part.parse::<usize>() {
        let rest = &location[..last_colon];

        // Try three-part: file:line:col
        //
        // If `rest` contains a colon and the part after it is a number, treat
        // this as `file:line:col`. We reject (return None) even if the line is
        // zero — a zero-line three-part location is always invalid.
        if let Some(second_colon) = rest.rfind(':') {
            if second_colon > 0 {
                let line_part = &rest[second_colon + 1..];
                if let Ok(line) = line_part.parse::<usize>() {
                    // This is unambiguously a three-part location; validate it.
                    if line == 0 {
                        return None; // zero line is invalid
                    }
                    let file_part = &rest[..second_colon];
                    if file_part.is_empty() {
                        return None;
                    }
                    return Some((file_part, line, last_num));
                }
                // line_part is not numeric — `rest` is not a `file:line` pair.
                // Fall through to two-part interpretation.
            }
        }

        // Two-part: file:line (col defaults to 0)
        if last_num == 0 {
            return None;
        }
        if rest.is_empty() {
            return None;
        }
        return Some((rest, last_num, 0));
    }

    // Last segment is not numeric — not a valid location
    None
}

/// Find the innermost symbol whose line range contains `target_line`.
///
/// Walks `symbols` and their `children` recursively, returning the deepest
/// symbol that contains the target line.
fn find_enclosing_symbol(
    symbols: &[codequery_core::Symbol],
    target_line: usize,
) -> Option<&codequery_core::Symbol> {
    let mut best: Option<&codequery_core::Symbol> = None;
    find_enclosing_recursive(symbols, target_line, &mut best);
    best
}

fn find_enclosing_recursive<'a>(
    symbols: &'a [codequery_core::Symbol],
    target_line: usize,
    best: &mut Option<&'a codequery_core::Symbol>,
) {
    for symbol in symbols {
        if symbol.line <= target_line && target_line <= symbol.end_line {
            // This symbol contains the target; it may be deeper than current best.
            // Since we recurse into children after updating, children always win.
            *best = Some(symbol);
            find_enclosing_recursive(&symbol.children, target_line, best);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{Symbol, SymbolKind, Visibility};
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Helper: create a minimal Symbol for testing.
    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        line: usize,
        end_line: usize,
        children: Vec<Symbol>,
        doc: Option<&str>,
        signature: Option<&str>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from("test.rs"),
            line,
            column: 0,
            end_line,
            visibility: Visibility::Public,
            children,
            doc: doc.map(String::from),
            body: None,
            signature: signature.map(String::from),
        }
    }

    // -----------------------------------------------------------------------
    // parse_location tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hover_parse_location_file_line() {
        let result = parse_location("src/main.rs:42");
        assert_eq!(result, Some(("src/main.rs", 42, 0)));
    }

    #[test]
    fn test_hover_parse_location_file_line_col() {
        let result = parse_location("src/main.rs:42:8");
        assert_eq!(result, Some(("src/main.rs", 42, 8)));
    }

    #[test]
    fn test_hover_parse_location_windows_path_line_col() {
        // Last colon is the column; second-to-last is the line separator
        let result = parse_location("C:/src/main.rs:10:4");
        assert_eq!(result, Some(("C:/src/main.rs", 10, 4)));
    }

    #[test]
    fn test_hover_parse_location_windows_path_line_only() {
        let result = parse_location("C:/src/main.rs:10");
        assert_eq!(result, Some(("C:/src/main.rs", 10, 0)));
    }

    #[test]
    fn test_hover_parse_location_no_colon_returns_none() {
        let result = parse_location("src/main.rs");
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_parse_location_zero_line_returns_none() {
        let result = parse_location("src/main.rs:0");
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_parse_location_zero_line_with_col_returns_none() {
        let result = parse_location("src/main.rs:0:5");
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_parse_location_empty_file_part_returns_none() {
        let result = parse_location(":42");
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_parse_location_non_numeric_returns_none() {
        let result = parse_location("src/main.rs:abc");
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // find_enclosing_symbol tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hover_enclosing_finds_function() {
        let symbols = vec![
            make_symbol("foo", SymbolKind::Function, 1, 5, vec![], None, None),
            make_symbol("bar", SymbolKind::Function, 10, 15, vec![], None, None),
        ];
        let result = find_enclosing_symbol(&symbols, 3);
        assert_eq!(result.map(|s| s.name.as_str()), Some("foo"));
    }

    #[test]
    fn test_hover_enclosing_prefers_innermost() {
        let method = make_symbol(
            "do_thing",
            SymbolKind::Method,
            5,
            8,
            vec![],
            Some("Does the thing."),
            Some("fn do_thing(&self) -> bool"),
        );
        let impl_block = make_symbol(
            "MyStruct",
            SymbolKind::Impl,
            3,
            10,
            vec![method],
            None,
            None,
        );
        let symbols = vec![impl_block];
        let result = find_enclosing_symbol(&symbols, 6);
        assert_eq!(result.map(|s| s.name.as_str()), Some("do_thing"));
    }

    #[test]
    fn test_hover_enclosing_returns_none_outside_symbols() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            5,
            10,
            vec![],
            None,
            None,
        )];
        let result = find_enclosing_symbol(&symbols, 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_enclosing_at_start_boundary() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            5,
            10,
            vec![],
            None,
            None,
        )];
        let result = find_enclosing_symbol(&symbols, 5);
        assert_eq!(result.map(|s| s.name.as_str()), Some("foo"));
    }

    #[test]
    fn test_hover_enclosing_at_end_boundary() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            5,
            10,
            vec![],
            None,
            None,
        )];
        let result = find_enclosing_symbol(&symbols, 10);
        assert_eq!(result.map(|s| s.name.as_str()), Some("foo"));
    }

    #[test]
    fn test_hover_enclosing_doc_propagated() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            1,
            5,
            vec![],
            Some("A greeting function."),
            Some("fn greet(name: &str) -> String"),
        )];
        let sym = find_enclosing_symbol(&symbols, 2).unwrap();
        assert_eq!(sym.doc.as_deref(), Some("A greeting function."));
        assert_eq!(
            sym.signature.as_deref(),
            Some("fn greet(name: &str) -> String")
        );
    }

    // -----------------------------------------------------------------------
    // Integration tests against fixture project
    // -----------------------------------------------------------------------

    // Test: position on `greet` function — should find it and return Success
    #[test]
    fn test_hover_fixture_function_with_doc_succeeds() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        // Line 9 is `pub fn greet(name: &str) -> String {`
        let location = format!("{}:9:7", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: position on `is_adult` in services.rs — method with doc
    #[test]
    fn test_hover_fixture_method_with_doc_succeeds() {
        let project = fixture_project();
        let file = project.join("src/services.rs");
        // Line 16 is `pub fn is_adult(&self) -> bool {`
        let location = format!("{}:16", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: JSON output
    #[test]
    fn test_hover_fixture_json_mode() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:9:7", file.display());
        let result = run(&location, Some(&project), OutputMode::Json, true, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: Raw output
    #[test]
    fn test_hover_fixture_raw_mode() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let location = format!("{}:9:7", file.display());
        let result = run(&location, Some(&project), OutputMode::Raw, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: position with no type info and no enclosing symbol returns NoResults
    #[test]
    fn test_hover_fixture_no_results_at_comment_line() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        // Line 1 is the module doc comment, outside any function
        let location = format!("{}:1", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test: nonexistent file returns ProjectError
    #[test]
    fn test_hover_nonexistent_file_returns_project_error() {
        let project = fixture_project();
        let file = project.join("src/nonexistent.rs");
        let location = format!("{}:10", file.display());
        let result = run(&location, Some(&project), OutputMode::Framed, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    // Test: invalid location string returns UsageError
    #[test]
    fn test_hover_invalid_location_returns_usage_error() {
        let project = fixture_project();
        let result = run(
            "not-a-location",
            Some(&project),
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::UsageError);
    }
}
