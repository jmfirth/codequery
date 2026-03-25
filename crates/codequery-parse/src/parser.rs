//! Tree-sitter parser configured for Rust source code.

use std::path::Path;

use crate::error::{ParseError, Result};

/// A tree-sitter parser configured for Rust source code.
///
/// Wraps a `tree_sitter::Parser` with the Rust grammar pre-loaded.
/// The parser is reusable across multiple files — call `parse()` or
/// `parse_file()` repeatedly without recreating the parser.
pub struct RustParser {
    parser: tree_sitter::Parser,
}

impl RustParser {
    /// Create a new parser with the Rust grammar loaded.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the Rust grammar fails to load.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|e| ParseError::LanguageError(e.to_string()))?;
        Ok(Self { parser })
    }

    /// Parse source bytes into a tree-sitter tree.
    ///
    /// Tree-sitter always produces a tree, even for invalid syntax.
    /// Check `tree.root_node().has_error()` to detect parse errors
    /// in the resulting tree.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::ParseFailed` if tree-sitter returns `None`,
    /// which should only happen if the language is not set.
    pub fn parse(&mut self, source: &[u8]) -> Result<tree_sitter::Tree> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseFailed("unknown source".to_string()))
    }

    /// Read a file from disk and parse it.
    ///
    /// Returns the file contents as a `String` and the parsed tree.
    /// The caller needs the source string because tree-sitter nodes
    /// reference byte ranges in the original source.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Io` if the file cannot be read, or
    /// `ParseError::ParseFailed` if tree-sitter returns no tree.
    pub fn parse_file(&mut self, path: &Path) -> Result<(String, tree_sitter::Tree)> {
        let source = std::fs::read_to_string(path)?;
        let tree = self.parse(source.as_bytes())?;
        Ok((source, tree))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_succeeds_with_rust_grammar() {
        let parser = RustParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_valid_rust_returns_tree_without_errors() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main() {}").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_broken_rust_returns_tree_with_errors() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main( {}").unwrap();
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_parse_empty_source_returns_valid_tree() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"").unwrap();
        // Empty source is valid — tree-sitter produces a tree with a source_file root
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parser_is_reusable_across_multiple_sources() {
        let mut parser = RustParser::new().unwrap();

        let tree1 = parser.parse(b"fn foo() {}").unwrap();
        assert!(!tree1.root_node().has_error());

        let tree2 = parser.parse(b"struct Bar { x: i32 }").unwrap();
        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn test_parse_file_reads_and_parses_fixture() {
        let mut parser = RustParser::new().unwrap();
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/src/lib.rs");
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        assert!(!source.is_empty());
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_file_nonexistent_returns_io_error() {
        let mut parser = RustParser::new().unwrap();
        let result = parser.parse_file(Path::new("/nonexistent/path/file.rs"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::Io(_)));
    }

    #[test]
    fn test_parse_valid_rust_root_node_is_source_file() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main() {}").unwrap();
        assert_eq!(tree.root_node().kind(), "source_file");
    }
}
