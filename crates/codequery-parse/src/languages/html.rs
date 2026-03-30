//! HTML-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts tags with `id` or `class` attributes as symbols.
//! This gives `cq outline` visibility into HTML document structure.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// HTML language extractor.
pub struct HtmlExtractor;

impl LanguageExtractor for HtmlExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_recursive(root, source, file, &mut symbols, 0);
        symbols
    }
}

/// Maximum depth to recurse into the HTML tree.
const MAX_DEPTH: usize = 64;

/// Recursively extract symbols from HTML elements.
fn extract_recursive(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }

    if node.kind() == "element" || node.kind() == "self_closing_tag" {
        if let Some(sym) = extract_element(node, source, file) {
            symbols.push(sym);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        extract_recursive(child, source, file, symbols, depth + 1);
    }
}

/// Try to extract a symbol from an HTML element node.
///
/// Extracts elements that have `id` or `class` attributes, using the id
/// or class value as the symbol name. Plain elements without identifying
/// attributes are skipped.
fn extract_element(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    // Find the start_tag or self_closing_tag child
    let tag_node = find_child_by_kind(node, "start_tag")
        .or_else(|| find_child_by_kind(node, "self_closing_tag"))
        .unwrap_or(node);

    // Get the tag name
    let tag_name = find_child_by_kind(tag_node, "tag_name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("unknown");

    // Look for id or class attribute
    let id_value = find_attribute_value(tag_node, "id", source);
    let class_value = find_attribute_value(tag_node, "class", source);

    // Only emit symbols for elements with id or class, or structural tags
    let (name, kind) = if let Some(id) = &id_value {
        (format!("{tag_name}#{id}"), SymbolKind::Type)
    } else if let Some(class) = &class_value {
        (format!("{tag_name}.{class}"), SymbolKind::Type)
    } else if is_structural_tag(tag_name) {
        (tag_name.to_string(), SymbolKind::Module)
    } else {
        return None;
    };

    let body_text = &source[node.start_byte()..node.end_byte()];
    let signature = extract_tag_signature(tag_node, source);

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
        body: Some(body_text.to_string()),
        signature: Some(signature),
    })
}

/// Check if a tag name is a structural HTML element worth extracting.
fn is_structural_tag(tag: &str) -> bool {
    matches!(
        tag,
        "html"
            | "head"
            | "body"
            | "header"
            | "footer"
            | "main"
            | "nav"
            | "section"
            | "article"
            | "aside"
            | "form"
            | "table"
            | "script"
            | "style"
            | "template"
    )
}

/// Find the value of a named attribute on a tag node.
fn find_attribute_value(
    tag_node: tree_sitter::Node<'_>,
    attr_name: &str,
    source: &str,
) -> Option<String> {
    let mut cursor = tag_node.walk();
    for child in tag_node.children(&mut cursor) {
        if child.kind() == "attribute" {
            let name_node = find_child_by_kind(child, "attribute_name")?;
            let name = name_node.utf8_text(source.as_bytes()).ok()?;
            if name == attr_name {
                let value_node = find_child_by_kind(child, "quoted_attribute_value")
                    .or_else(|| find_child_by_kind(child, "attribute_value"))?;
                let value = value_node.utf8_text(source.as_bytes()).ok()?;
                // Strip quotes if present
                let trimmed = value.trim_matches('"').trim_matches('\'');
                return Some(trimmed.to_string());
            }
        }
    }
    None
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

/// Extract the opening tag as the signature.
fn extract_tag_signature(tag_node: tree_sitter::Node<'_>, source: &str) -> String {
    source[tag_node.start_byte()..tag_node.end_byte()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Html).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        HtmlExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    #[test]
    fn test_extract_html_empty_returns_empty() {
        let symbols = parse_and_extract("", "empty.html");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_html_structural_tags() {
        let source = "<html><head></head><body><main></main></body></html>";
        let symbols = parse_and_extract(source, "index.html");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"html"));
        assert!(names.contains(&"head"));
        assert!(names.contains(&"body"));
        assert!(names.contains(&"main"));
    }

    #[test]
    fn test_extract_html_id_attribute() {
        let source = r#"<div id="app">content</div>"#;
        let symbols = parse_and_extract(source, "test.html");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "div#app");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
    }

    #[test]
    fn test_extract_html_class_attribute() {
        let source = r#"<div class="container">content</div>"#;
        let symbols = parse_and_extract(source, "test.html");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "div.container");
    }

    #[test]
    fn test_extract_html_id_takes_precedence_over_class() {
        let source = r#"<div id="main" class="wrapper">content</div>"#;
        let symbols = parse_and_extract(source, "test.html");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "div#main");
    }

    #[test]
    fn test_extract_html_plain_div_skipped() {
        let source = "<div>content</div>";
        let symbols = parse_and_extract(source, "test.html");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_html_line_numbers_are_1_based() {
        let source = "<html>\n<body>\n<div id=\"app\">hi</div>\n</body>\n</html>";
        let symbols = parse_and_extract(source, "test.html");
        let app = symbols.iter().find(|s| s.name == "div#app").unwrap();
        assert_eq!(app.line, 3);
    }

    #[test]
    fn test_extract_html_has_body_and_signature() {
        let source = r#"<div id="test">inner</div>"#;
        let symbols = parse_and_extract(source, "test.html");
        assert!(symbols[0].body.is_some());
        assert!(symbols[0].signature.is_some());
    }
}
