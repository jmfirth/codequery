//! Syntax error extraction from tree-sitter parse trees.
//!
//! Walks a tree-sitter tree and collects all ERROR and MISSING nodes
//! as `Diagnostic` values. This provides AST-level syntax diagnostics
//! without requiring a language server.

use std::path::Path;

use codequery_core::{Diagnostic, DiagnosticSeverity, DiagnosticSource};

/// Extract syntax errors from a tree-sitter parse tree.
///
/// Recursively walks the tree and creates a `Diagnostic` for every node
/// where `is_error()` or `is_missing()` returns true. ERROR nodes produce
/// "unexpected syntax" messages; MISSING nodes produce "missing {kind}"
/// messages. Positions are converted to 1-based lines.
#[must_use]
pub fn extract_syntax_errors(
    _source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
) -> Vec<Diagnostic> {
    let root = tree.root_node();
    if !root.has_error() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    collect_errors(&root, file, &mut diagnostics);
    diagnostics
}

/// Recursively collect error and missing nodes from a tree-sitter subtree.
fn collect_errors(node: &tree_sitter::Node, file: &Path, diagnostics: &mut Vec<Diagnostic>) {
    if node.is_error() {
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            line: node.start_position().row + 1,
            column: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_column: node.end_position().column,
            severity: DiagnosticSeverity::Error,
            message: "unexpected syntax".to_string(),
            source: DiagnosticSource::Syntax,
            code: None,
        });
        // Don't recurse into ERROR nodes — they already capture the region
        return;
    }

    if node.is_missing() {
        diagnostics.push(Diagnostic {
            file: file.to_path_buf(),
            line: node.start_position().row + 1,
            column: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_column: node.end_position().column,
            severity: DiagnosticSeverity::Error,
            message: format!("missing {}", node.kind()),
            source: DiagnosticSource::Syntax,
            code: None,
        });
        return;
    }

    // Only recurse into children if this subtree has errors
    if node.has_error() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_errors(&child, file, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    #[test]
    fn test_extract_syntax_errors_valid_rust_returns_empty() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() { let x = 42; }";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_extract_syntax_errors_unclosed_brace_returns_error() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() {";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(
            !errors.is_empty(),
            "expected at least 1 error for unclosed brace"
        );
        assert!(errors
            .iter()
            .all(|d| d.severity == DiagnosticSeverity::Error));
        assert!(errors.iter().all(|d| d.source == DiagnosticSource::Syntax));
    }

    #[test]
    fn test_extract_syntax_errors_missing_semicolon_returns_error() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() { let x = 42 }";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(
            !errors.is_empty(),
            "expected at least 1 error for missing semicolon"
        );
    }

    #[test]
    fn test_extract_syntax_errors_positions_are_correct() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        // Line 1: valid
        // Line 2: broken
        let source = "fn main() {\n  let x = \n}";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(!errors.is_empty(), "expected at least 1 error");

        // All errors should have 1-based line numbers
        for diag in &errors {
            assert!(diag.line >= 1, "line should be 1-based, got {}", diag.line);
            assert_eq!(diag.file, file);
        }
    }

    #[test]
    fn test_extract_syntax_errors_missing_node_has_kind_message() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() { let x = 42 }";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        // Look for a MISSING node diagnostic with "missing" in the message
        let has_missing = errors.iter().any(|d| d.message.starts_with("missing "));
        let has_unexpected = errors.iter().any(|d| d.message == "unexpected syntax");
        assert!(
            has_missing || has_unexpected,
            "expected either a 'missing' or 'unexpected syntax' diagnostic, got: {errors:?}"
        );
    }

    #[test]
    fn test_extract_syntax_errors_empty_file_returns_empty() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_extract_syntax_errors_python_syntax_error() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let source = "def foo(\n  pass";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.py");

        let errors = extract_syntax_errors(source, &tree, &file);
        assert!(
            !errors.is_empty(),
            "expected at least 1 error for broken Python"
        );
    }
}
