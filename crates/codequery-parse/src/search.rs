//! Structural search engine using tree-sitter S-expression queries.
//!
//! Searches parsed source files using tree-sitter's native query language.
//! S-expressions provide precise, language-aware pattern matching against
//! the AST. Use `cq tree <file>` to explore node types for a language.

use std::path::{Path, PathBuf};

use streaming_iterator::StreamingIterator;

use crate::error::{ParseError, Result};

/// A single match from a structural search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Path to the file containing the match.
    pub file: PathBuf,
    /// Start line (0-indexed).
    pub line: usize,
    /// Start column (0-indexed).
    pub column: usize,
    /// End line (0-indexed).
    pub end_line: usize,
    /// End column (0-indexed).
    pub end_column: usize,
    /// The matched source text.
    pub matched_text: String,
    /// The pattern that produced this match.
    pub pattern: String,
}

/// Search a parsed file using a tree-sitter S-expression query.
///
/// Uses tree-sitter's `Query::new` to compile the S-expression and
/// `QueryCursor` to execute against the tree. The first capture in each
/// pattern (or the entire match if there are no captures) is returned.
///
/// # Errors
///
/// Returns `ParseError::QueryError` if the query string fails to compile.
pub fn search_file(
    query_str: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
) -> Result<Vec<SearchMatch>> {
    let language = tree.language();
    let query = tree_sitter::Query::new(&language, query_str)
        .map_err(|e| ParseError::QueryError(e.to_string()))?;

    let mut cursor = tree_sitter::QueryCursor::new();
    let source_bytes = source.as_bytes();

    let mut matches = Vec::new();
    let mut query_matches = cursor.matches(&query, tree.root_node(), source_bytes);

    while let Some(m) = query_matches.next() {
        // Use the first capture if available, otherwise skip patterns
        // without captures (they can't pinpoint a specific node).
        if let Some(capture) = m.captures.first() {
            let node = capture.node;
            let start = node.start_position();
            let end = node.end_position();
            let text = node
                .utf8_text(source_bytes)
                .unwrap_or("<invalid utf8>")
                .to_string();

            matches.push(SearchMatch {
                file: file.to_path_buf(),
                line: start.row,
                column: start.column,
                end_line: end.row,
                end_column: end.column,
                matched_text: text,
                pattern: query_str.to_string(),
            });
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    // -----------------------------------------------------------------------
    // S-expression search — Rust
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_rust_function_items() {
        let source = "fn main() {}\nfn helper() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_item name: (identifier) @name)",
            source,
            &tree,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|m| m.matched_text.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
    }

    #[test]
    fn test_search_rust_captures_full_node() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results =
            search_file("(function_item) @func", source, &tree, Path::new("test.rs")).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("fn main"));
    }

    #[test]
    fn test_search_rust_function_by_name() {
        let source = "fn greet() {}\nfn helper() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_item name: (identifier) @name (#eq? @name \"greet\"))",
            source,
            &tree,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "greet");
    }

    #[test]
    fn test_search_rust_struct_items() {
        let source = "struct Foo { x: i32 }\nstruct Bar { y: String }\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(struct_item name: (type_identifier) @name)",
            source,
            &tree,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|m| m.matched_text.as_str()).collect();
        assert!(names.contains(&"Foo"));
        assert!(names.contains(&"Bar"));
    }

    // -----------------------------------------------------------------------
    // S-expression search — Python
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_python_function_def() {
        let source = "def greet(name):\n    return name\n\ndef add(a, b):\n    return a + b\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_definition name: (identifier) @name)",
            source,
            &tree,
            Path::new("test.py"),
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|m| m.matched_text.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"add"));
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_invalid_query_returns_error() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let result = search_file(
            "(not_a_real_node @name)",
            source,
            &tree,
            Path::new("test.rs"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_search_no_captures_returns_empty() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Query without captures — matches exist but no captures to extract
        let results = search_file("(function_item)", source, &tree, Path::new("test.rs")).unwrap();

        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // SearchMatch fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_match_has_correct_position() {
        let source = "fn first() {}\nfn second() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_item name: (identifier) @name)",
            source,
            &tree,
            Path::new("test.rs"),
        )
        .unwrap();

        assert_eq!(results.len(), 2);

        // "first" is on line 0
        assert_eq!(results[0].line, 0);
        assert_eq!(results[0].matched_text, "first");

        // "second" is on line 1
        assert_eq!(results[1].line, 1);
        assert_eq!(results[1].matched_text, "second");
    }

    #[test]
    fn test_search_match_file_path_preserved() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let path = Path::new("/some/deep/path/main.rs");
        let results = search_file("(function_item) @func", source, &tree, path).unwrap();

        assert_eq!(results[0].file, path);
    }

    #[test]
    fn test_search_match_pattern_stored() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let query = "(function_item) @func";
        let results = search_file(query, source, &tree, Path::new("test.rs")).unwrap();

        assert_eq!(results[0].pattern, query);
    }

    // -----------------------------------------------------------------------
    // Fixture-based tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_against_rust_fixture() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/src/lib.rs");
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        let results = search_file(
            "(function_item name: (identifier) @name)",
            &source,
            &tree,
            &fixture_path,
        )
        .unwrap();

        let names: Vec<&str> = results.iter().map(|m| m.matched_text.as_str()).collect();
        assert!(names.contains(&"greet"));
    }

    #[test]
    fn test_search_against_python_fixture() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/python_project/src/main.py");
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        let results = search_file(
            "(function_definition name: (identifier) @name)",
            &source,
            &tree,
            &fixture_path,
        )
        .unwrap();

        let names: Vec<&str> = results.iter().map(|m| m.matched_text.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"add"));
        assert!(names.contains(&"_private_helper"));
    }

    // -----------------------------------------------------------------------
    // Empty / edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_empty_source_returns_empty() {
        let source = "";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_item) @func",
            source,
            &tree,
            Path::new("empty.rs"),
        )
        .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_multiple_captures_uses_first() {
        let source = "fn greet(name: &str) -> String { format!(\"Hello\") }\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "(function_item name: (identifier) @name body: (block) @body)",
            source,
            &tree,
            Path::new("test.rs"),
        )
        .unwrap();

        // Should use first capture (@name), which is "greet"
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "greet");
    }
}
