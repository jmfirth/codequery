//! CSS-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts selectors, custom properties (CSS variables),
//! and media queries. This gives `cq outline` visibility into stylesheet
//! structure.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// CSS language extractor.
pub struct CssExtractor;

impl LanguageExtractor for CssExtractor {
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

/// Extract top-level CSS constructs.
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "rule_set" => {
            if let Some(sym) = extract_rule_set(node, source, file) {
                symbols.push(sym);
            }
        }
        "media_statement" => {
            symbols.push(extract_media_query(node, source, file));
        }
        "keyframes_statement" => {
            if let Some(sym) = extract_keyframes(node, source, file) {
                symbols.push(sym);
            }
        }
        "declaration" => {
            // Top-level custom property (CSS variable) in :root
            if let Some(sym) = extract_custom_property(node, source, file) {
                symbols.push(sym);
            }
        }
        _ => {}
    }

    // Recurse into rule_set blocks for nested custom properties (e.g. :root)
    if node.kind() == "rule_set" {
        if let Some(block) = find_child_by_kind(node, "block") {
            let mut cursor = block.walk();
            for child in block.children(&mut cursor) {
                if child.kind() == "declaration" {
                    if let Some(sym) = extract_custom_property(child, source, file) {
                        symbols.push(sym);
                    }
                }
            }
        }
    }
}

/// Extract a CSS rule set (selector + block).
fn extract_rule_set(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let selectors = find_child_by_kind(node, "selectors")?;
    let name = selectors
        .utf8_text(source.as_bytes())
        .ok()?
        .trim()
        .to_string();

    let body = &source[node.start_byte()..node.end_byte()];

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
        body: Some(body.to_string()),
        signature: Some(
            selectors
                .utf8_text(source.as_bytes())
                .ok()?
                .trim()
                .to_string(),
        ),
    })
}

/// Extract a `@media` query.
fn extract_media_query(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Symbol {
    let body_text = &source[node.start_byte()..node.end_byte()];
    // Extract the @media ... { part as signature
    let signature = if let Some(brace) = body_text.find('{') {
        body_text[..brace].trim().to_string()
    } else {
        body_text.lines().next().unwrap_or("@media").to_string()
    };

    Symbol {
        name: signature.clone(),
        kind: SymbolKind::Module,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body_text.to_string()),
        signature: Some(signature),
    }
}

/// Extract a `@keyframes` rule.
fn extract_keyframes(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name_node = find_child_by_kind(node, "keyframes_name")?;
    let name = name_node
        .utf8_text(source.as_bytes())
        .ok()?
        .trim()
        .to_string();
    let body_text = &source[node.start_byte()..node.end_byte()];

    Some(Symbol {
        name: format!("@keyframes {name}"),
        kind: SymbolKind::Type,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body_text.to_string()),
        signature: Some(format!("@keyframes {name}")),
    })
}

/// Extract a CSS custom property (variable) declaration.
fn extract_custom_property(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let prop_node = find_child_by_kind(node, "property_name")?;
    let name = prop_node.utf8_text(source.as_bytes()).ok()?;
    if !name.starts_with("--") {
        return None;
    }

    let body_text = &source[node.start_byte()..node.end_byte()];

    Some(Symbol {
        name: name.to_string(),
        kind: SymbolKind::Const,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children: vec![],
        doc: None,
        body: Some(body_text.to_string()),
        signature: Some(body_text.trim().to_string()),
    })
}

/// Find the first child of a node with the given kind.
fn find_child_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor).find(|c| c.kind() == kind);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Css).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        CssExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    #[test]
    fn test_extract_css_empty_returns_empty() {
        let symbols = parse_and_extract("", "empty.css");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_css_selector() {
        let source = ".container { display: flex; }";
        let symbols = parse_and_extract(source, "style.css");
        let selectors: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Type)
            .map(|s| s.name.as_str())
            .collect();
        assert!(selectors.contains(&".container"));
    }

    #[test]
    fn test_extract_css_custom_property() {
        let source = ":root {\n  --primary-color: #333;\n  --font-size: 16px;\n}\n";
        let symbols = parse_and_extract(source, "vars.css");
        let vars: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Const)
            .map(|s| s.name.as_str())
            .collect();
        assert!(vars.contains(&"--primary-color"));
        assert!(vars.contains(&"--font-size"));
    }

    #[test]
    fn test_extract_css_media_query() {
        let source = "@media (max-width: 768px) {\n  .container { flex-direction: column; }\n}\n";
        let symbols = parse_and_extract(source, "responsive.css");
        let media: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .map(|s| s.name.as_str())
            .collect();
        assert!(!media.is_empty());
        assert!(media[0].contains("@media"));
    }

    #[test]
    fn test_extract_css_line_numbers_are_1_based() {
        let source = "\n\n.test { color: red; }\n";
        let symbols = parse_and_extract(source, "test.css");
        let test = symbols.iter().find(|s| s.name == ".test").unwrap();
        assert_eq!(test.line, 3);
    }

    #[test]
    fn test_extract_css_has_body_and_signature() {
        let source = ".btn { padding: 8px; }";
        let symbols = parse_and_extract(source, "test.css");
        let btn = symbols.iter().find(|s| s.name == ".btn").unwrap();
        assert!(btn.body.is_some());
        assert!(btn.signature.is_some());
    }

    #[test]
    fn test_extract_css_multiple_selectors() {
        let source = "h1 { font-size: 2em; }\nh2 { font-size: 1.5em; }\n";
        let symbols = parse_and_extract(source, "headings.css");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"h1"));
        assert!(names.contains(&"h2"));
    }
}
