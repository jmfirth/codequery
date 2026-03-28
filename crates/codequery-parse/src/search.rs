//! Structural search engine for AST pattern matching.
//!
//! Provides two approaches for searching parsed source files:
//! - **Structural patterns**: Parse a code pattern with the same grammar as
//!   the source, then walk the source AST looking for structurally matching
//!   subtrees. Supports `$NAME` metavariables (match any single node) and
//!   `$$$` (match zero or more nodes).
//! - **Raw S-expression queries**: Use tree-sitter's native query language
//!   directly for maximum control.

use std::path::{Path, PathBuf};

use codequery_core::Language;
use streaming_iterator::StreamingIterator;

use crate::error::{ParseError, Result};
use crate::parser::grammar_for_language;

/// A single match from a structural or raw search.
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

/// Prefix used for metavariable placeholder identifiers during pattern parsing.
const METAVAR_PREFIX: &str = "__cq_meta_";

/// Search a parsed file for AST nodes matching the structural pattern.
///
/// The pattern is parsed with the same tree-sitter grammar as the source.
/// `$NAME` metavariables in the pattern match any single named node.
/// `$$$` matches zero or more sibling nodes.
///
/// # Errors
///
/// Returns `ParseError::PatternError` if the pattern cannot be parsed, or
/// `ParseError::LanguageError` if the grammar fails to load.
pub fn search_file(
    pattern: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,
) -> Result<Vec<SearchMatch>> {
    let (rewritten, metavar_map) = rewrite_metavars(pattern);

    let grammar = grammar_for_language(language);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| ParseError::LanguageError(e.to_string()))?;

    let pattern_tree = parser.parse(rewritten.as_bytes(), None).ok_or_else(|| {
        ParseError::PatternError("tree-sitter returned no tree for pattern".to_string())
    })?;

    let pattern_root = pattern_tree.root_node();

    // If the pattern tree has errors, the pattern is not valid source code
    if pattern_root.has_error() {
        return Err(ParseError::PatternError(format!(
            "pattern contains syntax errors: {rewritten}"
        )));
    }

    // Find the first meaningful named child in the pattern tree — skip the
    // wrapper source_file / module / program node so we match the actual
    // structural pattern the user wrote.
    let pattern_node = find_pattern_node(pattern_root);

    let source_bytes = source.as_bytes();
    let mut matches = Vec::new();

    collect_structural_matches(
        tree.root_node(),
        pattern_node,
        source_bytes,
        rewritten.as_bytes(),
        &metavar_map,
        file,
        pattern,
        &mut matches,
    );

    matches.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    Ok(matches)
}

/// Search using a raw tree-sitter S-expression query.
///
/// Uses tree-sitter's `Query::new` to compile the S-expression and
/// `QueryCursor` to execute against the tree. The first capture in each
/// pattern (or the entire match if there are no captures) is returned.
///
/// # Errors
///
/// Returns `ParseError::QueryError` if the query string fails to compile.
pub fn search_file_raw(
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

/// Rewrite `$NAME` metavariables in a pattern to valid placeholder identifiers.
///
/// Returns the rewritten pattern string and a map from placeholder name to
/// original metavariable name. `$$$` is rewritten to a special marker
/// `__cq_meta_variadic`.
fn rewrite_metavars(pattern: &str) -> (String, Vec<(String, String)>) {
    let mut result = String::with_capacity(pattern.len());
    let mut map = Vec::new();
    let mut counter = 0usize;
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 2 < chars.len() && chars[i + 1] == '$' && chars[i + 2] == '$' {
                // $$$ — variadic match
                let placeholder = format!("{METAVAR_PREFIX}variadic");
                map.push((placeholder.clone(), "$$$".to_string()));
                result.push_str(&placeholder);
                i += 3;
            } else if i + 1 < chars.len() && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_') {
                // $NAME metavar
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                let name: String = chars[start..end].iter().collect();
                let placeholder = format!("{METAVAR_PREFIX}{counter}");
                map.push((placeholder.clone(), format!("${name}")));
                result.push_str(&placeholder);
                counter += 1;
                i = end;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    (result, map)
}

/// Find the first meaningful named child of a tree root node.
///
/// Tree-sitter wraps everything in a root node (e.g., `source_file` for Rust,
/// `module` for Python). We want to match against the actual pattern content.
fn find_pattern_node(root: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    if root.named_child_count() == 1 {
        if let Some(child) = root.named_child(0) {
            return child;
        }
    }
    root
}

/// Recursively walk the source tree looking for subtrees matching the pattern.
#[allow(clippy::too_many_arguments)]
// Structural match traversal requires passing multiple context values; splitting would obscure the recursive logic
fn collect_structural_matches(
    source_node: tree_sitter::Node<'_>,
    pattern_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    pattern_bytes: &[u8],
    metavar_map: &[(String, String)],
    file: &Path,
    original_pattern: &str,
    matches: &mut Vec<SearchMatch>,
) {
    if nodes_match(
        source_node,
        pattern_node,
        source_bytes,
        pattern_bytes,
        metavar_map,
    ) {
        let start = source_node.start_position();
        let end = source_node.end_position();
        let text = source_node
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
            pattern: original_pattern.to_string(),
        });
    }

    // Recurse into children even if this node matched, to find nested matches
    let mut cursor = source_node.walk();
    for child in source_node.named_children(&mut cursor) {
        collect_structural_matches(
            child,
            pattern_node,
            source_bytes,
            pattern_bytes,
            metavar_map,
            file,
            original_pattern,
            matches,
        );
    }
}

/// Check if a source node structurally matches a pattern node.
///
/// A pattern node matches a source node when:
/// - The pattern node is a metavar placeholder (matches any single node)
/// - Both nodes have the same `kind()` and their named children recursively match
fn nodes_match(
    source_node: tree_sitter::Node<'_>,
    pattern_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    pattern_bytes: &[u8],
    metavar_map: &[(String, String)],
) -> bool {
    // Check if pattern node text is a metavar placeholder
    if let Ok(text) = pattern_node.utf8_text(pattern_bytes) {
        if is_metavar_placeholder(text) {
            return true;
        }
    }

    // Node kinds must match
    if source_node.kind() != pattern_node.kind() {
        return false;
    }

    let pattern_children = named_children_vec(pattern_node);
    let source_children = named_children_vec(source_node);

    // Leaf pattern node — check if the text matches (for terminals like identifiers)
    if pattern_children.is_empty() {
        if source_children.is_empty() {
            // Both are leaves: compare text
            let p_text = pattern_node.utf8_text(pattern_bytes).unwrap_or("");
            let s_text = source_node.utf8_text(source_bytes).unwrap_or("");

            // If pattern text is a metavar placeholder, match anything
            if is_metavar_placeholder(p_text) {
                return true;
            }
            return p_text == s_text;
        }
        // Pattern is a leaf but source has children — no match for leaf nodes
        return false;
    }

    // Check for variadic ($$$) in pattern children
    if has_variadic(&pattern_children, pattern_bytes) {
        return match_with_variadic(
            &source_children,
            &pattern_children,
            source_bytes,
            pattern_bytes,
            metavar_map,
        );
    }

    // Non-variadic: children must match 1:1
    if source_children.len() != pattern_children.len() {
        return false;
    }

    source_children
        .iter()
        .zip(pattern_children.iter())
        .all(|(s, p)| nodes_match(*s, *p, source_bytes, pattern_bytes, metavar_map))
}

/// Check if a text value is one of our metavar placeholders.
fn is_metavar_placeholder(text: &str) -> bool {
    text.starts_with(METAVAR_PREFIX)
}

/// Collect named children of a node into a Vec.
fn named_children_vec(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Check if any pattern child is a variadic placeholder (`$$$`).
fn has_variadic(pattern_children: &[tree_sitter::Node<'_>], pattern_bytes: &[u8]) -> bool {
    pattern_children.iter().any(|child| {
        child
            .utf8_text(pattern_bytes)
            .is_ok_and(|t| t == format!("{METAVAR_PREFIX}variadic"))
    })
}

/// Match source children against pattern children that include a variadic (`$$$`).
///
/// The variadic matches zero or more consecutive source children. Non-variadic
/// pattern children before and after the variadic must match the corresponding
/// source children.
fn match_with_variadic(
    source_children: &[tree_sitter::Node<'_>],
    pattern_children: &[tree_sitter::Node<'_>],
    source_bytes: &[u8],
    pattern_bytes: &[u8],
    metavar_map: &[(String, String)],
) -> bool {
    // Find the index of the variadic in the pattern children
    let variadic_idx = pattern_children
        .iter()
        .position(|child| {
            child
                .utf8_text(pattern_bytes)
                .is_ok_and(|t| t == format!("{METAVAR_PREFIX}variadic"))
        })
        .expect("has_variadic was true but variadic not found");

    let before = &pattern_children[..variadic_idx];
    let after = &pattern_children[variadic_idx + 1..];

    // There must be enough source children to satisfy before + after
    if source_children.len() < before.len() + after.len() {
        return false;
    }

    // Match the prefix
    for (s, p) in source_children[..before.len()].iter().zip(before.iter()) {
        if !nodes_match(*s, *p, source_bytes, pattern_bytes, metavar_map) {
            return false;
        }
    }

    // Match the suffix
    let suffix_start = source_children.len() - after.len();
    for (s, p) in source_children[suffix_start..].iter().zip(after.iter()) {
        if !nodes_match(*s, *p, source_bytes, pattern_bytes, metavar_map) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    // -----------------------------------------------------------------------
    // Metavar rewriting
    // -----------------------------------------------------------------------

    #[test]
    fn test_rewrite_metavars_single_var() {
        let (rewritten, map) = rewrite_metavars("fn $NAME() {}");
        assert!(rewritten.contains("__cq_meta_0"));
        assert!(!rewritten.contains("$NAME"));
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].1, "$NAME");
    }

    #[test]
    fn test_rewrite_metavars_multiple_vars() {
        let (rewritten, map) = rewrite_metavars("fn $FUNC($ARG: $TYPE) {}");
        assert!(rewritten.contains("__cq_meta_0"));
        assert!(rewritten.contains("__cq_meta_1"));
        assert!(rewritten.contains("__cq_meta_2"));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn test_rewrite_metavars_variadic() {
        let (rewritten, map) = rewrite_metavars("fn foo($$$) {}");
        assert!(rewritten.contains("__cq_meta_variadic"));
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].1, "$$$");
    }

    #[test]
    fn test_rewrite_metavars_no_vars() {
        let (rewritten, map) = rewrite_metavars("fn foo() {}");
        assert_eq!(rewritten, "fn foo() {}");
        assert!(map.is_empty());
    }

    // -----------------------------------------------------------------------
    // Structural search — Rust
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_file_rust_finds_function_by_name() {
        let source = "fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "fn greet($ARGS) -> String { $BODY }",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line, 0);
        assert!(results[0].matched_text.contains("fn greet"));
    }

    #[test]
    fn test_search_file_rust_finds_simple_function() {
        let source = "fn main() {}\nfn helper() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "fn main() {}",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("fn main"));
    }

    #[test]
    fn test_search_file_rust_no_match() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "fn nonexistent() {}",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_file_rust_metavar_matches_any_function() {
        let source = "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "fn $NAME() {}",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_file_rust_struct_with_specific_field() {
        let source = "struct Foo {\n    x: i32,\n}\nstruct Bar {\n    y: String,\n}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Match struct with a specific field name and type
        let results = search_file(
            "struct Foo {\n    x: i32,\n}",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("struct Foo"));
    }

    #[test]
    fn test_search_file_rust_struct_metavar_name() {
        let source = "struct Foo {\n    x: i32,\n}\nstruct Bar {\n    y: String,\n}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Metavar for the struct name only — the field list must still match
        let results = search_file(
            "struct $NAME {\n    x: i32,\n}",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("struct Foo"));
    }

    // -----------------------------------------------------------------------
    // Structural search — Python
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_file_python_finds_function() {
        let source =
            "def greet(name):\n    return f\"Hello, {name}!\"\n\ndef helper():\n    pass\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "def greet(name):\n    return f\"Hello, {name}!\"",
            source,
            &tree,
            Path::new("test.py"),
            Language::Python,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("def greet"));
    }

    #[test]
    fn test_search_file_python_metavar_matches_any_function() {
        let source = "def foo():\n    pass\n\ndef bar():\n    pass\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "def $NAME():\n    pass",
            source,
            &tree,
            Path::new("test.py"),
            Language::Python,
        )
        .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_file_python_no_match() {
        let source = "def foo():\n    pass\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file(
            "def nonexistent():\n    pass",
            source,
            &tree,
            Path::new("test.py"),
            Language::Python,
        )
        .unwrap();

        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // Raw S-expression search
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_file_raw_rust_function_items() {
        let source = "fn main() {}\nfn helper() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file_raw(
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
    fn test_search_file_raw_python_function_def() {
        let source = "def greet(name):\n    return name\n\ndef add(a, b):\n    return a + b\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file_raw(
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

    #[test]
    fn test_search_file_raw_invalid_query_returns_error() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let result = search_file_raw(
            "(not_a_real_node @name)",
            source,
            &tree,
            Path::new("test.rs"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_search_file_raw_no_captures_returns_empty() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Query without captures — matches exist but no captures to extract
        let results =
            search_file_raw("(function_item)", source, &tree, Path::new("test.rs")).unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_file_raw_captures_full_node() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results =
            search_file_raw("(function_item) @func", source, &tree, Path::new("test.rs")).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].matched_text.contains("fn main"));
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_file_invalid_pattern_returns_error() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let result = search_file(
            "this is not valid rust {{{{",
            source,
            &tree,
            Path::new("test.rs"),
            Language::Rust,
        );

        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // SearchMatch fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_match_has_correct_position() {
        let source = "fn first() {}\nfn second() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file_raw(
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
        let results = search_file_raw("(function_item) @func", source, &tree, path).unwrap();

        assert_eq!(results[0].file, path);
    }

    #[test]
    fn test_search_match_pattern_stored() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let query = "(function_item) @func";
        let results = search_file_raw(query, source, &tree, Path::new("test.rs")).unwrap();

        assert_eq!(results[0].pattern, query);
    }

    // -----------------------------------------------------------------------
    // Fixture-based tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_file_raw_against_rust_fixture() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/src/lib.rs");
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        let results = search_file_raw(
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
    fn test_search_file_raw_against_python_fixture() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/python_project/src/main.py");
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        let results = search_file_raw(
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
    fn test_search_file_empty_source_returns_empty() {
        let source = "";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file_raw(
            "(function_item) @func",
            source,
            &tree,
            Path::new("empty.rs"),
        )
        .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_file_raw_multiple_captures_uses_first() {
        let source = "fn greet(name: &str) -> String { format!(\"Hello\") }\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        let results = search_file_raw(
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
