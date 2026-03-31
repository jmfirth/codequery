//! Bash-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts function definitions — both the `function name()`
//! syntax and the `name()` syntax. All Bash functions are Public by default
//! since Bash has no visibility system.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Bash language extractor.
pub struct BashExtractor;

impl LanguageExtractor for BashExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            extract_top_level(child, source, file, &mut symbols);
        }

        symbols
    }
}

/// Extract the full source body of a symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a Bash symbol.
///
/// For functions, the signature is the first line of the definition
/// (e.g., `function greet()` or `say_hello()`).
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, _kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    extract_fn_signature(body_text)
}

/// Extract function signature: the first line up to the opening `{`.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim_end().to_string()
    }
}

/// Extract top-level symbols from a node, appending to `symbols`.
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    if node.kind() == "function_definition" {
        let Some(name) = node_field_text(node, "name", source) else {
            return;
        };
        let body = extract_body(source, &node);
        let signature = extract_signature(source, &node, SymbolKind::Function);
        symbols.push(Symbol {
            name,
            kind: SymbolKind::Function,
            file: file.to_path_buf(),
            line: node.start_position().row + 1,
            column: node.start_position().column,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
            children: vec![],
            doc: extract_doc_comment(node, source),
            body: Some(body),
            signature: Some(signature),
        });
    }
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract doc comments preceding a definition node.
///
/// In Bash, comments use `#` syntax. Doc comments are consecutive `#` comments
/// immediately preceding the definition.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with('#') {
                    doc_lines.push(trimmed.to_string());
                    sibling = sib.prev_sibling();
                    continue;
                }
            }
            break;
        }
        break;
    }

    if doc_lines.is_empty() {
        return None;
    }

    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    /// Helper: parse source and extract symbols for the given file path.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let Ok(mut parser) = Parser::for_language(Language::Bash) else {
            eprintln!("skipping: Bash grammar not installed");
            return;
        };
        let tree = parser.parse(source.as_bytes()).unwrap();
        BashExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture bash project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bash_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let Ok(mut parser) = Parser::for_language(Language::Bash) else {
            eprintln!("skipping: Bash grammar not installed");
            return;
        };
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = BashExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract function with `function` keyword
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_function_keyword_syntax() {
        let (_, symbols) = extract_fixture("main.sh");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract function with name() syntax
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_name_paren_syntax() {
        let (_, symbols) = extract_fixture("main.sh");
        let say_hello = symbols
            .iter()
            .find(|s| s.name == "say_hello")
            .expect("say_hello not found");
        assert_eq!(say_hello.kind, SymbolKind::Function);
        assert_eq!(say_hello.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: All functions are Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_all_functions_are_public() {
        let (_, symbols) = extract_fixture("main.sh");
        for sym in &symbols {
            assert_eq!(
                sym.visibility,
                Visibility::Public,
                "function {} should be public",
                sym.name
            );
        }
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Body and signature extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_function_body_contains_source() {
        let (_, symbols) = extract_fixture("main.sh");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.contains("echo"));
        assert!(body.ends_with('}'));
    }

    #[test]
    fn test_extract_bash_function_keyword_signature() {
        let (_, symbols) = extract_fixture("main.sh");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "function greet()");
    }

    #[test]
    fn test_extract_bash_name_paren_signature() {
        let (_, symbols) = extract_fixture("main.sh");
        let say_hello = symbols
            .iter()
            .find(|s| s.name == "say_hello")
            .expect("say_hello not found");
        let sig = say_hello
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "say_hello()");
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Doc comment extracted
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_doc_comment() {
        let (_, symbols) = extract_fixture("main.sh");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.doc.as_deref(), Some("# Greet a person by name."));
    }

    // -----------------------------------------------------------------------
    // Scenario 6: All symbols have body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["main.sh", "utils.sh"] {
            let (_, symbols) = extract_fixture(fixture);
            assert!(
                !symbols.is_empty(),
                "expected symbols in {fixture}, got none"
            );
            for sym in &symbols {
                assert!(
                    sym.body.is_some(),
                    "symbol {} in {} should have a body",
                    sym.name,
                    fixture
                );
                assert!(
                    sym.signature.is_some(),
                    "symbol {} in {} should have a signature",
                    sym.name,
                    fixture
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Multiple functions extracted from utils.sh
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_utils_functions() {
        let (_, symbols) = extract_fixture("utils.sh");
        assert_eq!(symbols.len(), 3);
        assert!(symbols.iter().any(|s| s.name == "log_info"));
        assert!(symbols.iter().any(|s| s.name == "log_error"));
        assert!(symbols.iter().any(|s| s.name == "cleanup"));
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_bash_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.sh");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_bash_no_functions_returns_empty_vec() {
        let source = "#!/bin/bash\necho hello\n";
        let symbols = parse_and_extract(source, "nofunc.sh");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_bash_broken_source_no_panic() {
        // Bash parser may absorb errors differently; just verify no panic
        let source = "function good() {\n  echo ok\n}\nfunction broken( {\n";
        let symbols = parse_and_extract(source, "broken.sh");
        // At minimum, we should not panic. The parser may or may not
        // recover "good" depending on how the error propagates.
        let _ = symbols;
    }

    #[test]
    fn test_extract_bash_line_numbers_are_1_based() {
        let source = "first() {\n  echo 1\n}\nsecond() {\n  echo 2\n}\n";
        let symbols = parse_and_extract(source, "test.sh");
        let first = symbols
            .iter()
            .find(|s| s.name == "first")
            .expect("first not found");
        assert_eq!(first.line, 1);
        assert_eq!(first.column, 0);
        let second = symbols
            .iter()
            .find(|s| s.name == "second")
            .expect("second not found");
        assert_eq!(second.line, 4);
    }
}
