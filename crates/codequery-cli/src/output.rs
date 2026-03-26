//! Output formatting for cq commands — framed, JSON, and raw modes.
//!
//! This module turns `Symbol` data from codequery-core into the three output
//! formats defined in SPECIFICATION.md section 9. It is pure formatting:
//! no I/O, no parsing, only string construction from typed symbol data.

use codequery_core::{Completeness, QueryResult, Resolution, Symbol};
use serde::Serialize;
use std::io::IsTerminal;
use std::path::Path;

use crate::args::OutputMode;

// ---------------------------------------------------------------------------
// JSON data structures
// ---------------------------------------------------------------------------

/// JSON payload for the `def` command.
#[derive(Debug, Serialize)]
pub struct DefResults {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Matching definitions.
    pub definitions: Vec<Symbol>,
    /// Total number of matches.
    pub total: usize,
}

/// JSON payload for the `body` command.
#[derive(Debug, Serialize)]
pub struct BodyResults {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Matching definitions with body text.
    pub definitions: Vec<Symbol>,
    /// Total number of matches.
    pub total: usize,
}

/// JSON payload for the `outline` command.
#[derive(Debug, Serialize)]
pub struct OutlineResult {
    /// The file that was outlined.
    pub file: String,
    /// Top-level symbols in the file.
    pub symbols: Vec<Symbol>,
}

// ---------------------------------------------------------------------------
// Def formatting
// ---------------------------------------------------------------------------

/// Format `def` results in the requested mode.
pub fn format_def(symbols: &[Symbol], symbol_name: &str, mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_def_results(symbols),
        OutputMode::Json => format_def_json(symbols, symbol_name, pretty),
        OutputMode::Raw => format_def_raw(symbols),
    }
}

/// Format symbol definitions for the `def` command — framed output.
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

/// Format `def` results as JSON wrapped in `QueryResult`.
fn format_def_json(symbols: &[Symbol], symbol_name: &str, force_pretty: bool) -> String {
    let data = DefResults {
        symbol: symbol_name.to_string(),
        definitions: symbols.to_vec(),
        total: symbols.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `def` results as raw text (no `@@` delimiters).
fn format_def_raw(symbols: &[Symbol]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    output
}

// ---------------------------------------------------------------------------
// Body formatting
// ---------------------------------------------------------------------------

/// Format `body` results in the requested mode.
pub fn format_body(
    symbols: &[Symbol],
    symbol_name: &str,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_body_framed(symbols),
        OutputMode::Json => format_body_json(symbols, symbol_name, pretty),
        OutputMode::Raw => format_body_raw(symbols),
    }
}

/// Format symbol bodies — framed output.
///
/// Each symbol produces a frame header followed by its body text.
/// Multiple results are separated by blank lines.
fn format_body_framed(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format_frame_header(symbol));
        if let Some(body) = &symbol.body {
            output.push('\n');
            output.push_str(body);
        }
    }
    output
}

/// Format `body` results as JSON wrapped in `QueryResult`.
fn format_body_json(symbols: &[Symbol], symbol_name: &str, force_pretty: bool) -> String {
    let data = BodyResults {
        symbol: symbol_name.to_string(),
        definitions: symbols.to_vec(),
        total: symbols.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `body` results as raw text — body text only, no framing.
fn format_body_raw(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        if let Some(body) = &symbol.body {
            output.push_str(body);
        }
    }
    output
}

// ---------------------------------------------------------------------------
// Outline formatting
// ---------------------------------------------------------------------------

/// Format `outline` results in the requested mode.
pub fn format_outline_output(
    file: &Path,
    symbols: &[Symbol],
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_outline(file, symbols),
        OutputMode::Json => format_outline_json(file, symbols, pretty),
        OutputMode::Raw => format_outline_raw(symbols),
    }
}

/// Format a file's symbol outline — framed output.
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

/// Format `outline` results as JSON wrapped in `QueryResult`.
fn format_outline_json(file: &Path, symbols: &[Symbol], force_pretty: bool) -> String {
    let data = OutlineResult {
        file: file.display().to_string(),
        symbols: symbols.to_vec(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `outline` results as raw text (no `@@` header).
fn format_outline_raw(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        format_outline_symbol(symbol, 0, &mut output);
    }
    output
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

/// Serialize a value to JSON, choosing pretty or compact based on TTY and flags.
fn serialize_json<T: Serialize>(value: &T, force_pretty: bool) -> String {
    let use_pretty = force_pretty || std::io::stdout().is_terminal();
    if use_pretty {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        serde_json::to_string(value).unwrap_or_default()
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
            body: None,
            signature: None,
        }
    }

    fn make_symbol_with_body(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        visibility: Visibility,
        body: &str,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility,
            children: vec![],
            doc: Some("A doc comment.".to_string()),
            body: Some(body.to_string()),
            signature: Some(format!("fn {name}()")),
        }
    }

    // -----------------------------------------------------------------------
    // Framed output tests (regression)
    // -----------------------------------------------------------------------

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

    #[test]
    fn test_def_empty_results_returns_empty_string() {
        let output = format_def_results(&[]);
        assert_eq!(output, "");
    }

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

    #[test]
    fn test_outline_file_header_format() {
        let output = format_outline(Path::new("src/api/routes.rs"), &[]);
        assert!(output.starts_with("@@ src/api/routes.rs @@"));
    }

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

        let def_output = format_def_results(&symbols);
        assert!(def_output.contains("struct MyStruct"));
        assert!(def_output.contains("trait MyTrait"));
        assert!(def_output.contains("enum MyEnum"));

        let outline_output = format_outline(Path::new("lib.rs"), &symbols);
        assert!(outline_output.contains("MyStruct (struct, pub)"));
        assert!(outline_output.contains("MyTrait (trait, pub)"));
        assert!(outline_output.contains("MyEnum (enum, pub)"));
    }

    #[test]
    fn test_outline_no_symbols_shows_just_file_header() {
        let output = format_outline(Path::new("src/empty.rs"), &[]);
        assert_eq!(output, "@@ src/empty.rs @@");
    }

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

    // -----------------------------------------------------------------------
    // JSON output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_def_json_produces_valid_json_with_metadata() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def(&symbols, "greet", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "greet");
        assert_eq!(json["total"], 1);
        assert!(json["definitions"].is_array());
        assert_eq!(json["definitions"][0]["name"], "greet");
        assert_eq!(json["definitions"][0]["kind"], "function");
    }

    #[test]
    fn test_def_json_empty_results_has_metadata() {
        let output = format_def(&[], "missing", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "missing");
        assert_eq!(json["total"], 0);
        assert_eq!(json["definitions"], serde_json::json!([]));
    }

    #[test]
    fn test_outline_json_produces_valid_json_with_metadata() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        let output =
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/lib.rs");
        assert!(json["symbols"].is_array());
        assert_eq!(json["symbols"][0]["name"], "greet");
    }

    #[test]
    fn test_outline_json_empty_symbols_has_metadata() {
        let output = format_outline_output(Path::new("src/empty.rs"), &[], OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/empty.rs");
        assert_eq!(json["symbols"], serde_json::json!([]));
    }

    // -----------------------------------------------------------------------
    // Raw output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_def_raw_strips_frame_delimiters() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "src/lib.rs:1:0 function foo");
    }

    #[test]
    fn test_def_raw_multiple_results_newline_separated() {
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
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                vec![],
            ),
        ];
        let output = format_def(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "src/lib.rs:1:0 function foo");
        assert_eq!(lines[1], "src/main.rs:10:0 function foo");
    }

    #[test]
    fn test_outline_raw_strips_frame_header() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        let output =
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert!(output.contains("greet (function, pub) :10"));
    }

    #[test]
    fn test_outline_raw_empty_symbols_is_empty() {
        let output = format_outline_output(Path::new("src/lib.rs"), &[], OutputMode::Raw, false);
        assert!(output.is_empty());
    }

    // -----------------------------------------------------------------------
    // Framed mode via format_def / format_outline_output (regression)
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_def_framed_matches_format_def_results() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        assert_eq!(
            format_def(&symbols, "foo", OutputMode::Framed, false),
            format_def_results(&symbols)
        );
    }

    #[test]
    fn test_format_outline_output_framed_matches_format_outline() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        assert_eq!(
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Framed, false),
            format_outline(Path::new("src/lib.rs"), &symbols)
        );
    }

    // -----------------------------------------------------------------------
    // Body framed output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_framed_single_result_with_body_text() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Framed, false);
        assert!(output.starts_with("@@ src/lib.rs:9:0 function greet @@\n"));
        assert!(output.contains("pub fn greet(name: &str) -> String {"));
        assert!(output.contains("format!(\"Hello, {name}!\")"));
    }

    #[test]
    fn test_body_framed_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                "fn foo() {}",
            ),
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                "fn foo() { 42 }",
            ),
        ];
        let output = format_body(&symbols, "foo", OutputMode::Framed, false);
        assert!(output.contains("@@ src/lib.rs:1:0 function foo @@\nfn foo() {}"));
        assert!(output.contains("\n\n@@ src/main.rs:10:0 function foo @@\nfn foo() { 42 }"));
    }

    #[test]
    fn test_body_framed_empty_results_returns_empty_string() {
        let output = format_body(&[], "missing", OutputMode::Framed, false);
        assert_eq!(output, "");
    }

    #[test]
    fn test_body_framed_symbol_without_body_shows_header_only() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_body(&symbols, "foo", OutputMode::Framed, false);
        assert_eq!(output, "@@ src/lib.rs:1:0 function foo @@");
    }

    // -----------------------------------------------------------------------
    // Body JSON output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_json_produces_valid_json_with_body_field() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet() {}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "greet");
        assert_eq!(json["total"], 1);
        assert!(json["definitions"].is_array());
        assert_eq!(json["definitions"][0]["name"], "greet");
        assert_eq!(json["definitions"][0]["body"], "pub fn greet() {}");
        assert_eq!(json["definitions"][0]["signature"], "fn greet()");
        assert_eq!(json["definitions"][0]["doc"], "A doc comment.");
    }

    #[test]
    fn test_body_json_empty_results_has_metadata() {
        let output = format_body(&[], "missing", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "missing");
        assert_eq!(json["total"], 0);
        assert_eq!(json["definitions"], serde_json::json!([]));
    }

    // -----------------------------------------------------------------------
    // Body raw output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_raw_outputs_body_text_only() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet() {\n    println!(\"hello\");\n}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "pub fn greet() {\n    println!(\"hello\");\n}");
    }

    #[test]
    fn test_body_raw_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                "fn foo() {}",
            ),
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                "fn foo() { 42 }",
            ),
        ];
        let output = format_body(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "fn foo() {}\n\nfn foo() { 42 }");
    }

    #[test]
    fn test_body_raw_empty_results_returns_empty_string() {
        let output = format_body(&[], "missing", OutputMode::Raw, false);
        assert_eq!(output, "");
    }

    #[test]
    fn test_body_raw_symbol_without_body_returns_empty_string() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_body(&symbols, "foo", OutputMode::Raw, false);
        assert_eq!(output, "");
    }
}
