//! Framed plain text output formatting for cq commands.
//!
//! This module turns `Symbol` data from codequery-core into the human-readable
//! framed text output defined in SPECIFICATION.md section 9.1. It is pure formatting:
//! no I/O, no parsing, only string construction from typed symbol data.

use codequery_core::Symbol;
use std::path::Path;

/// Format symbol definitions for the `def` command output.
///
/// Each symbol produces one frame header line: `@@ file:line:column kind name @@`
/// Multiple results are separated by blank lines.
/// Returns empty string if symbols is empty.
pub fn format_def_results(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format_frame_header(symbol));
    }
    output
}

/// Format a file's symbol outline.
///
/// Produces a file-level header followed by an indented symbol list
/// with nesting for children (e.g., methods inside impl blocks).
pub fn format_outline(file: &Path, symbols: &[Symbol]) -> String {
    let mut output = format!("@@ {} @@", file.display());
    for symbol in symbols {
        output.push('\n');
        format_outline_symbol(symbol, 1, &mut output);
    }
    output
}

/// Format a single frame header line.
fn format_frame_header(symbol: &Symbol) -> String {
    format!(
        "@@ {}:{}:{} {} {} @@",
        symbol.file.display(),
        symbol.line,
        symbol.column,
        symbol.kind,
        symbol.name,
    )
}

/// Format a symbol for the outline, at a given indent level.
fn format_outline_entry(symbol: &Symbol, indent: usize) -> String {
    let spaces = " ".repeat(indent * 2);
    format!(
        "{spaces}{} ({}, {}) :{}",
        symbol.name, symbol.kind, symbol.visibility, symbol.line,
    )
}

/// Recursively format a symbol and its children.
fn format_outline_symbol(symbol: &Symbol, indent: usize, output: &mut String) {
    output.push_str(&format_outline_entry(symbol, indent));
    for child in &symbol.children {
        output.push('\n');
        format_outline_symbol(child, indent + 1, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{SymbolKind, Visibility};
    use std::path::PathBuf;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        visibility: Visibility,
        children: Vec<Symbol>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility,
            children,
            doc: None,
        }
    }

    // Test 1: Single def result produces correct frame header
    #[test]
    fn test_def_single_result_produces_correct_frame_header() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def_results(&symbols);
        assert_eq!(output, "@@ src/lib.rs:1:0 function foo @@");
    }

    // Test 2: Multiple def results separated by blank line
    #[test]
    fn test_def_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "bar",
                SymbolKind::Function,
                "src/main.rs",
                10,
                4,
                Visibility::Private,
                vec![],
            ),
        ];
        let output = format_def_results(&symbols);
        assert_eq!(
            output,
            "@@ src/lib.rs:1:0 function foo @@\n\n@@ src/main.rs:10:4 function bar @@"
        );
    }

    // Test 3: Empty def results returns empty string
    #[test]
    fn test_def_empty_results_returns_empty_string() {
        let output = format_def_results(&[]);
        assert_eq!(output, "");
    }

    // Test 4: Outline with flat symbols (no children) produces correct output
    #[test]
    fn test_outline_flat_symbols_produces_correct_output() {
        let symbols = vec![
            make_symbol(
                "greet",
                SymbolKind::Function,
                "src/lib.rs",
                10,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MAX_RETRIES",
                SymbolKind::Const,
                "src/lib.rs",
                20,
                0,
                Visibility::Public,
                vec![],
            ),
        ];
        let output = format_outline(Path::new("src/lib.rs"), &symbols);
        assert_eq!(
            output,
            "@@ src/lib.rs @@\n  greet (function, pub) :10\n  MAX_RETRIES (const, pub) :20"
        );
    }

    // Test 5: Outline with nested symbols (impl -> methods) produces correct indentation
    #[test]
    fn test_outline_nested_symbols_produces_correct_indentation() {
        let method = make_symbol(
            "new",
            SymbolKind::Method,
            "src/lib.rs",
            22,
            4,
            Visibility::Public,
            vec![],
        );
        let impl_block = make_symbol(
            "Router",
            SymbolKind::Impl,
            "src/lib.rs",
            20,
            0,
            Visibility::Private,
            vec![method],
        );
        let func = make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        );
        let symbols = vec![func, impl_block];
        let output = format_outline(Path::new("src/lib.rs"), &symbols);
        let expected = "@@ src/lib.rs @@\n  greet (function, pub) :10\n  Router (impl, priv) :20\n    new (method, pub) :22";
        assert_eq!(output, expected);
    }

    // Test 6: Outline file header is @@ path @@
    #[test]
    fn test_outline_file_header_format() {
        let output = format_outline(Path::new("src/api/routes.rs"), &[]);
        assert!(output.starts_with("@@ src/api/routes.rs @@"));
    }

    // Test 7: Different visibility values display correctly in outline
    #[test]
    fn test_outline_visibility_values_display_correctly() {
        let symbols = vec![
            make_symbol(
                "public_fn",
                SymbolKind::Function,
                "lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "private_fn",
                SymbolKind::Function,
                "lib.rs",
                5,
                0,
                Visibility::Private,
                vec![],
            ),
            make_symbol(
                "crate_fn",
                SymbolKind::Function,
                "lib.rs",
                10,
                0,
                Visibility::Crate,
                vec![],
            ),
        ];
        let output = format_outline(Path::new("lib.rs"), &symbols);
        assert!(output.contains("(function, pub) :1"));
        assert!(output.contains("(function, priv) :5"));
        assert!(output.contains("(function, pub(crate)) :10"));
    }

    // Test 8: Different symbol kinds display correctly in frame header and outline
    #[test]
    fn test_different_symbol_kinds_display_correctly() {
        let symbols = vec![
            make_symbol(
                "MyStruct",
                SymbolKind::Struct,
                "lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MyTrait",
                SymbolKind::Trait,
                "lib.rs",
                10,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MyEnum",
                SymbolKind::Enum,
                "lib.rs",
                20,
                0,
                Visibility::Public,
                vec![],
            ),
        ];

        // Check def output
        let def_output = format_def_results(&symbols);
        assert!(def_output.contains("struct MyStruct"));
        assert!(def_output.contains("trait MyTrait"));
        assert!(def_output.contains("enum MyEnum"));

        // Check outline output
        let outline_output = format_outline(Path::new("lib.rs"), &symbols);
        assert!(outline_output.contains("MyStruct (struct, pub)"));
        assert!(outline_output.contains("MyTrait (trait, pub)"));
        assert!(outline_output.contains("MyEnum (enum, pub)"));
    }

    // Test 9: Outline with no symbols shows just the file header
    #[test]
    fn test_outline_no_symbols_shows_just_file_header() {
        let output = format_outline(Path::new("src/empty.rs"), &[]);
        assert_eq!(output, "@@ src/empty.rs @@");
    }

    // Test 10: Frame header uses 0-based column correctly
    #[test]
    fn test_frame_header_uses_zero_based_column() {
        let symbols = vec![make_symbol(
            "indented",
            SymbolKind::Function,
            "src/lib.rs",
            5,
            8,
            Visibility::Private,
            vec![],
        )];
        let output = format_def_results(&symbols);
        assert_eq!(output, "@@ src/lib.rs:5:8 function indented @@");
    }

    // Test 11: File paths in output match what was passed (no normalization)
    #[test]
    fn test_file_paths_not_normalized() {
        let symbols = vec![make_symbol(
            "func",
            SymbolKind::Function,
            "./src/../src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let def_output = format_def_results(&symbols);
        assert!(def_output.contains("./src/../src/lib.rs:1:0"));

        let outline_output = format_outline(Path::new("./weird/path/../file.rs"), &[]);
        assert!(outline_output.contains("./weird/path/../file.rs"));
    }
}
