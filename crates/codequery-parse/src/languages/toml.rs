//! TOML-specific symbol extraction from tree-sitter ASTs.
//!
//! Extracts tables and top-level key-value pairs from TOML documents. Gives
//! `cq outline` visibility into TOML structure (e.g., Cargo.toml, pyproject.toml).

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// TOML language extractor.
pub struct TomlExtractor;

impl LanguageExtractor for TomlExtractor {
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

/// Extract top-level TOML constructs.
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "table" => {
            if let Some(sym) = extract_table(node, source, file) {
                symbols.push(sym);
            }
        }
        "table_array_element" => {
            if let Some(sym) = extract_table_array(node, source, file) {
                symbols.push(sym);
            }
        }
        "pair" => {
            if let Some(sym) = extract_pair(node, source, file) {
                symbols.push(sym);
            }
        }
        _ => {}
    }
}

/// Extract a TOML table (`[section]`).
fn extract_table(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    // The table header is typically the first child
    let mut cursor = node.walk();
    let header = node.children(&mut cursor).find(|c| {
        // In tree-sitter-toml-ng, the header key lives under a node
        // that varies by grammar version. Try common patterns.
        c.kind().contains("key") || c.kind() == "dotted_key" || c.kind() == "bare_key"
    });

    let name = if let Some(h) = header {
        h.utf8_text(source.as_bytes()).ok()?.trim().to_string()
    } else {
        // Fallback: extract first line and get key between brackets
        let body_text = &source[node.start_byte()..node.end_byte()];
        let first_line = body_text.lines().next().unwrap_or("");
        let start = first_line.find('[')? + 1;
        let end = first_line.find(']')?;
        first_line[start..end].trim().to_string()
    };

    let body = &source[node.start_byte()..node.end_byte()];
    let signature = body.lines().next().unwrap_or("").to_string();

    Some(Symbol {
        name,
        kind: SymbolKind::Module,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body.to_string()),
        signature: Some(signature),
    })
}

/// Extract a TOML table array element (`[[section]]`).
fn extract_table_array(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let body_text = &source[node.start_byte()..node.end_byte()];
    let first_line = body_text.lines().next().unwrap_or("");
    // Extract key between double brackets
    let start = first_line.find("[[")? + 2;
    let end = first_line.find("]]")?;
    let name = first_line[start..end].trim().to_string();

    Some(Symbol {
        name,
        kind: SymbolKind::Type,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body_text.to_string()),
        signature: Some(first_line.to_string()),
    })
}

/// Extract a top-level key-value pair.
fn extract_pair(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let mut cursor = node.walk();
    let key_node = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("key") || c.kind() == "dotted_key" || c.kind() == "bare_key")?;
    let name = key_node
        .utf8_text(source.as_bytes())
        .ok()?
        .trim()
        .to_string();

    if name.is_empty() {
        return None;
    }

    let body = &source[node.start_byte()..node.end_byte()];

    Some(Symbol {
        name,
        kind: SymbolKind::Const,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body.to_string()),
        signature: Some(body.trim().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Toml).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        TomlExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    #[test]
    fn test_extract_toml_empty_returns_empty() {
        let symbols = parse_and_extract("", "empty.toml");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_toml_top_level_pair() {
        let source = "name = \"cq\"\nversion = \"0.1.0\"\n";
        let symbols = parse_and_extract(source, "Cargo.toml");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"version"));
    }

    #[test]
    fn test_extract_toml_table() {
        let source = "[package]\nname = \"cq\"\n";
        let symbols = parse_and_extract(source, "Cargo.toml");
        let pkg = symbols.iter().find(|s| s.name == "package");
        assert!(pkg.is_some());
        assert_eq!(pkg.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_toml_table_array() {
        let source = "[[bin]]\nname = \"cq\"\npath = \"src/main.rs\"\n";
        let symbols = parse_and_extract(source, "Cargo.toml");
        let bin = symbols.iter().find(|s| s.name == "bin");
        assert!(bin.is_some());
        assert_eq!(bin.unwrap().kind, SymbolKind::Type);
    }

    #[test]
    fn test_extract_toml_line_numbers_are_1_based() {
        let source = "[package]\nname = \"cq\"\n";
        let symbols = parse_and_extract(source, "Cargo.toml");
        let pkg = symbols.iter().find(|s| s.name == "package").unwrap();
        assert_eq!(pkg.line, 1);
    }

    #[test]
    fn test_extract_toml_has_body_and_signature() {
        let source = "[features]\ndefault = []\n";
        let symbols = parse_and_extract(source, "Cargo.toml");
        let features = symbols.iter().find(|s| s.name == "features").unwrap();
        assert!(features.body.is_some());
        assert!(features.signature.is_some());
    }
}
