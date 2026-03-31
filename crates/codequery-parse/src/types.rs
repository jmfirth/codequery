//! Type annotation extraction from tree-sitter ASTs.
//!
//! Given a source position, walks the AST to find the nearest enclosing
//! declaration and extracts its type annotation. Provides an AST-level
//! hover fallback when no language server is available.

use codequery_core::Language;

/// Extract the type annotation at a given source position.
///
/// Finds the deepest AST node at `(line, column)` and walks up to locate
/// a parent declaration with a type annotation. Returns the text of the
/// type annotation, or `None` if no type information is found.
///
/// `line` is 1-based, `column` is 0-based.
#[must_use]
pub fn extract_type_at_position(
    source: &str,
    tree: &tree_sitter::Tree,
    line: usize,
    column: usize,
    language: Language,
) -> Option<String> {
    // Convert 1-based line to 0-based row for tree-sitter
    let row = line.checked_sub(1)?;
    let point = tree_sitter::Point::new(row, column);

    let root = tree.root_node();
    let node = root.descendant_for_point_range(point, point)?;

    // Walk up from the deepest node to find a typed declaration
    let mut current = Some(node);
    while let Some(n) = current {
        if let Some(type_text) = extract_type_from_node(&n, source, language) {
            return Some(type_text);
        }
        current = n.parent();
    }
    None
}

/// Try to extract a type annotation from a specific AST node.
fn extract_type_from_node(
    node: &tree_sitter::Node,
    source: &str,
    language: Language,
) -> Option<String> {
    match language {
        Language::Rust => extract_rust_type(node, source),
        Language::Python => extract_python_type(node, source),
        Language::TypeScript | Language::JavaScript => extract_typescript_type(node, source),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

fn extract_rust_type(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "let_declaration" => {
            // `let x: Type = ...` — the type annotation is the `type` field
            let type_node = node.child_by_field_name("type")?;
            node_text(&type_node, source)
        }
        "parameter" => {
            // `fn foo(x: Type)` — parameter has a `type` field
            let type_node = node.child_by_field_name("type")?;
            node_text(&type_node, source)
        }
        "function_item" => {
            // `fn foo() -> ReturnType` — the return type field
            let ret = node.child_by_field_name("return_type")?;
            node_text(&ret, source)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn extract_python_type(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "typed_parameter" | "typed_default_parameter" => {
            let type_node = node.child_by_field_name("type")?;
            node_text(&type_node, source)
        }
        "function_definition" => {
            let ret = node.child_by_field_name("return_type")?;
            node_text(&ret, source)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript
// ---------------------------------------------------------------------------

fn extract_typescript_type(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "variable_declarator" => {
            // `const x: string = ...` — look for `type_annotation` child
            find_child_of_kind(node, "type_annotation")
                .and_then(|ann| type_annotation_text(&ann, source))
        }
        "required_parameter" | "optional_parameter" => find_child_of_kind(node, "type_annotation")
            .and_then(|ann| type_annotation_text(&ann, source)),
        "function_declaration" => {
            // Return type annotation
            find_child_of_kind(node, "return_type")
                .and_then(|ann| type_annotation_text(&ann, source))
                .or_else(|| {
                    find_child_of_kind(node, "type_annotation")
                        .and_then(|ann| type_annotation_text(&ann, source))
                })
        }
        _ => None,
    }
}

/// Extract the inner type from a `type_annotation` node (strips the leading `:`).
fn type_annotation_text(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // A type_annotation in TS typically has one named child — the actual type
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(text) = node_text(&child, source) {
            return Some(text);
        }
    }
    // Fallback: use the full text minus leading `: `
    let full = node_text(node, source)?;
    let trimmed = full.trim_start_matches(':').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Find the first child of a specific kind.
fn find_child_of_kind<'a>(
    node: &'a tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

/// Extract UTF-8 text from a tree-sitter node.
fn node_text(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    node.utf8_text(bytes)
        .ok()
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn test_rust_let_type_annotation() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() { let x: String = String::new(); }";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `x` — line 1, column 16
        let result = extract_type_at_position(source, &tree, 1, 16, Language::Rust);
        assert_eq!(result.as_deref(), Some("String"));
    }

    #[test]
    fn test_rust_parameter_type() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn foo(x: i32) {}";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `x` — line 1, column 7
        let result = extract_type_at_position(source, &tree, 1, 7, Language::Rust);
        assert_eq!(result.as_deref(), Some("i32"));
    }

    #[test]
    fn test_rust_function_return_type() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn foo() -> bool { true }";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position on `foo` — should walk up to function_item
        let result = extract_type_at_position(source, &tree, 1, 3, Language::Rust);
        assert_eq!(result.as_deref(), Some("bool"));
    }

    #[test]
    fn test_python_parameter_type() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let source = "def foo(x: int):\n    pass\n";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `x` — line 1, column 8
        let result = extract_type_at_position(source, &tree, 1, 8, Language::Python);
        assert_eq!(result.as_deref(), Some("int"));
    }

    #[test]
    fn test_python_return_type() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let source = "def foo() -> str:\n    return 'hello'\n";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position on `foo`
        let result = extract_type_at_position(source, &tree, 1, 4, Language::Python);
        assert_eq!(result.as_deref(), Some("str"));
    }

    #[test]
    fn test_typescript_variable_type() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let source = "const x: string = 'hello';";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `x` — line 1, column 6
        let result = extract_type_at_position(source, &tree, 1, 6, Language::TypeScript);
        assert_eq!(result.as_deref(), Some("string"));
    }

    #[test]
    fn test_typescript_parameter_type() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let source = "function foo(x: number): void {}";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `x` — line 1, column 13
        let result = extract_type_at_position(source, &tree, 1, 13, Language::TypeScript);
        assert_eq!(result.as_deref(), Some("number"));
    }

    #[test]
    fn test_no_type_annotation_returns_none() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() { let x = 42; }";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Position of `42` — no type annotation here
        let result = extract_type_at_position(source, &tree, 1, 20, Language::Rust);
        assert!(result.is_none());
    }

    #[test]
    fn test_zero_line_returns_none() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "fn main() {}";
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Line 0 is invalid (1-based), should return None via checked_sub
        let result = extract_type_at_position(source, &tree, 0, 0, Language::Rust);
        assert!(result.is_none());
    }
}
