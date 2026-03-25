//! Language-aware tree-sitter parser.
//!
//! Wraps a `tree_sitter::Parser` with the appropriate grammar pre-loaded
//! for any Tier 1 language. The parser is reusable across multiple files
//! of the same language.

use std::path::Path;

use codequery_core::Language;

use crate::error::{ParseError, Result};

/// A tree-sitter parser configured for a specific source language.
///
/// Created via the [`Parser::for_language`] factory, which loads the
/// correct grammar. The parser is reusable across multiple files — call
/// `parse()` or `parse_file()` repeatedly without recreating the parser.
pub struct Parser {
    parser: tree_sitter::Parser,
    language: Language,
}

impl Parser {
    /// Create a parser for the given language.
    ///
    /// Loads the tree-sitter grammar corresponding to `language`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the grammar fails to load.
    pub fn for_language(language: Language) -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let grammar = grammar_for_language(language);
        parser
            .set_language(&grammar)
            .map_err(|e| ParseError::LanguageError(e.to_string()))?;
        Ok(Self { parser, language })
    }

    /// The language this parser is configured for.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
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

/// Select the tree-sitter grammar for a language.
fn grammar_for_language(language: Language) -> tree_sitter::Language {
    match language {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
    }
}

/// A Rust-specific parser — convenience alias for backward compatibility.
///
/// Equivalent to `Parser::for_language(Language::Rust)`.
pub struct RustParser;

impl RustParser {
    /// Create a new parser with the Rust grammar loaded.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the Rust grammar fails to load.
    #[allow(clippy::new_ret_no_self)]
    // Backward-compatibility wrapper — intentionally returns Parser, not Self
    pub fn new() -> Result<Parser> {
        Parser::for_language(Language::Rust)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Backward-compatible: RustParser still works
    // -----------------------------------------------------------------------
    #[test]
    fn test_rust_parser_new_succeeds() {
        let parser = RustParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_rust_parser_returns_rust_language() {
        let parser = RustParser::new().unwrap();
        assert_eq!(parser.language(), Language::Rust);
    }

    // -----------------------------------------------------------------------
    // Parser::for_language for all 8 variants
    // -----------------------------------------------------------------------
    #[test]
    fn test_for_language_rust_creates_parser() {
        let parser = Parser::for_language(Language::Rust);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_typescript_creates_parser() {
        let parser = Parser::for_language(Language::TypeScript);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_javascript_creates_parser() {
        let parser = Parser::for_language(Language::JavaScript);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_python_creates_parser() {
        let parser = Parser::for_language(Language::Python);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_go_creates_parser() {
        let parser = Parser::for_language(Language::Go);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_c_creates_parser() {
        let parser = Parser::for_language(Language::C);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_cpp_creates_parser() {
        let parser = Parser::for_language(Language::Cpp);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_java_creates_parser() {
        let parser = Parser::for_language(Language::Java);
        assert!(parser.is_ok());
    }

    // -----------------------------------------------------------------------
    // Parsing with non-Rust languages produces valid trees
    // -----------------------------------------------------------------------
    #[test]
    fn test_parse_typescript_source_produces_tree() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let tree = parser
            .parse(b"function greet(name: string): string { return name; }")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_python_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser
            .parse(b"def greet(name: str) -> str:\n    return name\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_go_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let tree = parser
            .parse(b"package main\n\nfunc greet(name string) string {\n\treturn name\n}\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_c_source_produces_tree() {
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(b"int main() { return 0; }\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_cpp_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Cpp).unwrap();
        let tree = parser
            .parse(b"class Foo { public: void bar(); };\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_java_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser
            .parse(b"public class Main { public static void main(String[] args) {} }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_javascript_source_produces_tree() {
        let mut parser = Parser::for_language(Language::JavaScript).unwrap();
        let tree = parser
            .parse(b"function greet(name) { return name; }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    // -----------------------------------------------------------------------
    // Existing parser behavior tests (migrated from RustParser)
    // -----------------------------------------------------------------------
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
