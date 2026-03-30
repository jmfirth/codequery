//! JSON-specific symbol extraction from tree-sitter ASTs.
//!
//! Extracts top-level keys from JSON objects. Gives `cq outline` visibility
//! into JSON document structure (e.g., package.json, tsconfig.json).

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// JSON language extractor.
pub struct JsonExtractor;

impl LanguageExtractor for JsonExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();

        // JSON root is typically a document node containing a single value
        let value_node = if root.kind() == "document" {
            let mut cursor = root.walk();
            let result = root
                .children(&mut cursor)
                .find(|c| c.kind() == "object" || c.kind() == "array");
            result
        } else if root.kind() == "object" || root.kind() == "array" {
            Some(root)
        } else {
            None
        };

        if let Some(obj) = value_node {
            if obj.kind() == "object" {
                extract_object_keys(obj, source, file, &mut symbols);
            }
        }

        symbols
    }
}

/// Extract key-value pairs from a JSON object node.
fn extract_object_keys(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "pair" {
            if let Some(sym) = extract_pair(child, source, file) {
                symbols.push(sym);
            }
        }
    }
}

/// Extract a single key-value pair as a symbol.
fn extract_pair(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let key_node = node.child_by_field_name("key")?;
    let key_text = key_node.utf8_text(source.as_bytes()).ok()?;
    // Strip quotes from key
    let name = key_text.trim_matches('"').to_string();

    let value_node = node.child_by_field_name("value")?;
    let kind = match value_node.kind() {
        "object" => SymbolKind::Module,
        "array" => SymbolKind::Type,
        _ => SymbolKind::Const,
    };

    let body = &source[node.start_byte()..node.end_byte()];

    Some(Symbol {
        name,
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body.to_string()),
        signature: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Json).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        JsonExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    #[test]
    fn test_extract_json_empty_object() {
        let symbols = parse_and_extract("{}", "empty.json");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_json_top_level_keys() {
        let source = r#"{"name": "cq", "version": "1.0.0", "dependencies": {}}"#;
        let symbols = parse_and_extract(source, "package.json");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"version"));
        assert!(names.contains(&"dependencies"));
    }

    #[test]
    fn test_extract_json_object_value_is_module_kind() {
        let source = r#"{"scripts": {"build": "cargo build"}}"#;
        let symbols = parse_and_extract(source, "package.json");
        let scripts = symbols.iter().find(|s| s.name == "scripts").unwrap();
        assert_eq!(scripts.kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_json_array_value_is_type_kind() {
        let source = r#"{"keywords": ["cli", "code"]}"#;
        let symbols = parse_and_extract(source, "package.json");
        let kw = symbols.iter().find(|s| s.name == "keywords").unwrap();
        assert_eq!(kw.kind, SymbolKind::Type);
    }

    #[test]
    fn test_extract_json_scalar_value_is_const_kind() {
        let source = r#"{"name": "test"}"#;
        let symbols = parse_and_extract(source, "test.json");
        let name = symbols.iter().find(|s| s.name == "name").unwrap();
        assert_eq!(name.kind, SymbolKind::Const);
    }

    #[test]
    fn test_extract_json_line_numbers_are_1_based() {
        let source = "{\n  \"first\": 1,\n  \"second\": 2\n}\n";
        let symbols = parse_and_extract(source, "test.json");
        let first = symbols.iter().find(|s| s.name == "first").unwrap();
        assert_eq!(first.line, 2);
    }

    #[test]
    fn test_extract_json_has_body() {
        let source = r#"{"key": "value"}"#;
        let symbols = parse_and_extract(source, "test.json");
        assert!(symbols[0].body.is_some());
    }

    #[test]
    fn test_extract_json_empty_string() {
        let symbols = parse_and_extract("", "empty.json");
        assert!(symbols.is_empty());
    }
}
