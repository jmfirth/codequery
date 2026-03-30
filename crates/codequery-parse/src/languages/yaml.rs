//! YAML-specific symbol extraction from tree-sitter ASTs.
//!
//! Extracts top-level keys and anchors from YAML documents. Gives `cq outline`
//! visibility into YAML document structure (e.g., docker-compose.yml, CI configs).

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// YAML language extractor.
pub struct YamlExtractor;

impl LanguageExtractor for YamlExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_from_node(root, source, file, &mut symbols, 0);
        symbols
    }
}

/// Maximum recursion depth.
const MAX_DEPTH: usize = 4;

/// Walk the YAML AST and extract symbols.
fn extract_from_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }

        match child.kind() {
            "block_mapping_pair" | "flow_pair" => {
                if let Some(sym) = extract_mapping_pair(child, source, file, depth) {
                    symbols.push(sym);
                }
            }
            "anchor" => {
                if let Some(sym) = extract_anchor(child, source, file) {
                    symbols.push(sym);
                }
            }
            _ => {
                // Recurse into container nodes
                extract_from_node(child, source, file, symbols, depth);
            }
        }
    }
}

/// Extract a YAML mapping pair (key: value) as a symbol.
fn extract_mapping_pair(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    depth: usize,
) -> Option<Symbol> {
    // Only extract top-level keys (depth 0-1 — stream/document/block_node wrappers)
    // and second-level keys (depth 2)
    if depth > MAX_DEPTH {
        return None;
    }

    let key_node = node.child_by_field_name("key")?;
    let key_text = key_node
        .utf8_text(source.as_bytes())
        .ok()?
        .trim()
        .to_string();

    if key_text.is_empty() {
        return None;
    }

    let value_node = node.child_by_field_name("value");
    let kind = value_node.map_or(SymbolKind::Const, |v| classify_yaml_value(v));

    let body = &source[node.start_byte()..node.end_byte()];

    Some(Symbol {
        name: key_text,
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

/// Classify a YAML value node into a symbol kind.
///
/// `block_node` wrappers require looking at the first named child to
/// determine whether the underlying value is a mapping or sequence.
fn classify_yaml_value(node: tree_sitter::Node<'_>) -> SymbolKind {
    match node.kind() {
        "block_mapping" | "flow_mapping" => SymbolKind::Module,
        "block_sequence" | "flow_sequence" => SymbolKind::Type,
        "block_node" => {
            // block_node wraps the actual value — check inner child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "block_mapping" | "flow_mapping" => return SymbolKind::Module,
                    "block_sequence" | "flow_sequence" => return SymbolKind::Type,
                    _ => {}
                }
            }
            SymbolKind::Module
        }
        _ => SymbolKind::Const,
    }
}

/// Extract a YAML anchor (&name) as a symbol.
fn extract_anchor(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let text = node.utf8_text(source.as_bytes()).ok()?;
    // Anchors start with &
    let name = text.strip_prefix('&')?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    Some(Symbol {
        name: format!("&{name}"),
        kind: SymbolKind::Const,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(text.to_string()),
        signature: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Yaml).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        YamlExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    #[test]
    fn test_extract_yaml_empty_returns_empty() {
        let symbols = parse_and_extract("", "empty.yaml");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_yaml_top_level_keys() {
        let source = "name: my-app\nversion: 1.0\ndescription: A test app\n";
        let symbols = parse_and_extract(source, "config.yaml");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"version"));
        assert!(names.contains(&"description"));
    }

    #[test]
    fn test_extract_yaml_nested_mapping_is_module() {
        let source = "services:\n  web:\n    image: nginx\n";
        let symbols = parse_and_extract(source, "docker-compose.yml");
        let services = symbols.iter().find(|s| s.name == "services").unwrap();
        assert_eq!(services.kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_yaml_sequence_is_type() {
        let source = "steps:\n  - run: echo hello\n  - run: echo world\n";
        let symbols = parse_and_extract(source, "ci.yml");
        let steps = symbols.iter().find(|s| s.name == "steps").unwrap();
        assert_eq!(steps.kind, SymbolKind::Type);
    }

    #[test]
    fn test_extract_yaml_scalar_is_const() {
        let source = "name: test\n";
        let symbols = parse_and_extract(source, "test.yaml");
        let name = symbols.iter().find(|s| s.name == "name").unwrap();
        assert_eq!(name.kind, SymbolKind::Const);
    }

    #[test]
    fn test_extract_yaml_line_numbers_are_1_based() {
        let source = "first: 1\nsecond: 2\n";
        let symbols = parse_and_extract(source, "test.yaml");
        let first = symbols.iter().find(|s| s.name == "first").unwrap();
        assert_eq!(first.line, 1);
        let second = symbols.iter().find(|s| s.name == "second").unwrap();
        assert_eq!(second.line, 2);
    }

    #[test]
    fn test_extract_yaml_has_body() {
        let source = "key: value\n";
        let symbols = parse_and_extract(source, "test.yaml");
        assert!(symbols[0].body.is_some());
    }
}
