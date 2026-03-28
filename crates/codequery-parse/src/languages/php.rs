//! PHP-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, methods,
//! classes, interfaces, traits, and constants. PHP visibility is determined
//! by explicit keywords (`public`/`private`/`protected`). Default is Public.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// PHP language extractor.
pub struct PhpExtractor;

impl LanguageExtractor for PhpExtractor {
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

/// Extract the full source body of a PHP symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the signature of a PHP symbol.
///
/// - **Function/Method**: declaration line up to the opening `{`
/// - **Class/Interface/Trait**: header line up to the opening `{`
/// - **Const**: the full declaration line
/// - **Module**: the namespace declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait => {
            extract_header_signature(body_text)
        }
        SymbolKind::Const => extract_const_signature(body_text),
        SymbolKind::Module => extract_namespace_signature(body_text),
        _ => body_text.lines().next().unwrap_or("").to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        // Abstract methods end with `;`
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract class/interface/trait header: everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract constant signature: full declaration trimmed.
fn extract_const_signature(body: &str) -> String {
    body.trim_end_matches(';').trim().to_string()
}

/// Extract namespace signature.
fn extract_namespace_signature(body: &str) -> String {
    body.trim_end_matches(';').trim().to_string()
}

/// Extract PHP visibility from a node's `visibility_modifier` child.
///
/// Returns `Public` if no modifier is present (PHP default).
fn php_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|c| c.kind() == "visibility_modifier")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok())
        .map_or(Visibility::Public, |text| match text {
            "private" | "protected" => Visibility::Private,
            _ => Visibility::Public,
        });
    result
}

/// Extract a top-level symbol from a node, pushing into the symbols vec.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_definition" => {
            if let Some(sym) = extract_function(node, source, file) {
                symbols.push(sym);
            }
        }
        "class_declaration" => {
            if let Some(sym) = extract_class(node, source, file) {
                symbols.push(sym);
            }
        }
        "interface_declaration" => {
            if let Some(sym) = extract_interface(node, source, file) {
                symbols.push(sym);
            }
        }
        "trait_declaration" => {
            if let Some(sym) = extract_trait(node, source, file) {
                symbols.push(sym);
            }
        }
        "const_declaration" => {
            extract_const_declaration(node, source, file, symbols);
        }
        "namespace_definition" => {
            if let Some(sym) = extract_namespace(node, source, file) {
                symbols.push(sym);
            }
        }
        _ => {}
    }
}

/// Extract a top-level function definition.
fn extract_function(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_name(node, source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Function);
    Some(Symbol {
        name: name.clone(),
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
    })
}

/// Extract a class declaration with its method children.
fn extract_class(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_name(node, source)?;
    let children = extract_declaration_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Class);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Class,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract an interface declaration with its method children.
fn extract_interface(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_name(node, source)?;
    let children = extract_declaration_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Interface);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Interface,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a trait declaration with its method children.
fn extract_trait(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_name(node, source)?;
    let children = extract_declaration_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Trait);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Trait,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: Visibility::Public,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a top-level const declaration (outside class/interface/trait).
fn extract_const_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_element" {
            if let Some(name) = find_name(child, source) {
                let body = extract_body(source, &node);
                let signature = extract_signature(source, &node, SymbolKind::Const);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Const,
                    file: file.to_path_buf(),
                    line: node.start_position().row + 1,
                    column: node.start_position().column,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    children: vec![],
                    doc: None,
                    body: Some(body),
                    signature: Some(signature),
                });
            }
        }
    }
}

/// Extract a namespace definition.
fn extract_namespace(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    // Find namespace_name child
    let mut cursor = node.walk();
    let name = node
        .children(&mut cursor)
        .find(|c| c.kind() == "namespace_name")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok())
        .map(String::from)?;

    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Module);
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
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract methods and constants from a class/interface/trait `declaration_list`.
fn extract_declaration_members(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();

    // Find the declaration_list child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            let mut list_cursor = child.walk();
            for member in child.children(&mut list_cursor) {
                if member.is_error() || member.is_missing() {
                    continue;
                }
                match member.kind() {
                    "method_declaration" => {
                        if let Some(sym) = extract_method_declaration(member, source, file) {
                            members.push(sym);
                        }
                    }
                    "const_declaration" => {
                        extract_class_const(member, source, file, &mut members);
                    }
                    _ => {}
                }
            }
        }
    }

    members
}

/// Extract a method declaration inside a class/interface/trait.
fn extract_method_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let name = find_name(node, source)?;
    let visibility = php_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Method);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Method,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children: vec![],
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a const declaration inside a class (with visibility).
fn extract_class_const(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    members: &mut Vec<Symbol>,
) {
    let visibility = php_visibility(node, source);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_element" {
            if let Some(name) = find_name(child, source) {
                let body = extract_body(source, &node);
                let signature = extract_signature(source, &node, SymbolKind::Const);
                members.push(Symbol {
                    name,
                    kind: SymbolKind::Const,
                    file: file.to_path_buf(),
                    line: node.start_position().row + 1,
                    column: node.start_position().column,
                    end_line: node.end_position().row + 1,
                    visibility,
                    children: vec![],
                    doc: None,
                    body: Some(body),
                    signature: Some(signature),
                });
            }
        }
    }
}

/// Find the `name` child of a node.
fn find_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|c| c.kind() == "name")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok().map(String::from));
    result
}

/// Extract a doc comment preceding a definition node.
///
/// In PHP, doc comments are `/** ... */` style. This looks for `comment`
/// siblings preceding the node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let sibling = node.prev_sibling()?;
    if sibling.kind() == "comment" {
        let text = sibling.utf8_text(source.as_bytes()).ok()?;
        if text.starts_with("/**") {
            return Some(text.trim_end().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    /// Helper: parse PHP source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Php).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        PhpExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture PHP project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/php_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Php).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = PhpExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // =======================================================================
    // Test Scenario 1: Extract top-level function -> Function
    // =======================================================================
    #[test]
    fn test_extract_php_top_level_function() {
        let (_, symbols) = extract_fixture("src/main.php");
        let greet = symbols
            .iter()
            .find(|s| s.name == "globalFunction")
            .expect("globalFunction not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 2: Extract class with methods
    // =======================================================================
    #[test]
    fn test_extract_php_class_with_methods() {
        let (_, symbols) = extract_fixture("src/models.php");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.kind, SymbolKind::Class);

        let method_names: Vec<&str> = user
            .children
            .iter()
            .filter(|c| c.kind == SymbolKind::Method)
            .map(|c| c.name.as_str())
            .collect();
        assert!(method_names.contains(&"__construct"));
        assert!(method_names.contains(&"getName"));
    }

    // =======================================================================
    // Test Scenario 3: PHP visibility keywords
    // =======================================================================
    #[test]
    fn test_extract_php_visibility_public() {
        let (_, symbols) = extract_fixture("src/models.php");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let get_name = user
            .children
            .iter()
            .find(|c| c.name == "getName")
            .expect("getName not found");
        assert_eq!(get_name.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_php_visibility_private() {
        let (_, symbols) = extract_fixture("src/models.php");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let check = user
            .children
            .iter()
            .find(|c| c.name == "internalCheck")
            .expect("internalCheck not found");
        assert_eq!(check.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_php_visibility_protected() {
        let (_, symbols) = extract_fixture("src/models.php");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let validate = user
            .children
            .iter()
            .find(|c| c.name == "validate")
            .expect("validate not found");
        assert_eq!(validate.visibility, Visibility::Private);
    }

    // =======================================================================
    // Test Scenario 4: Extract interface
    // =======================================================================
    #[test]
    fn test_extract_php_interface() {
        let (_, symbols) = extract_fixture("src/models.php");
        let greeter = symbols
            .iter()
            .find(|s| s.name == "Greeter")
            .expect("Greeter not found");
        assert_eq!(greeter.kind, SymbolKind::Interface);
    }

    // =======================================================================
    // Test Scenario 5: Extract trait
    // =======================================================================
    #[test]
    fn test_extract_php_trait() {
        let (_, symbols) = extract_fixture("src/models.php");
        let loggable = symbols
            .iter()
            .find(|s| s.name == "Loggable")
            .expect("Loggable not found");
        assert_eq!(loggable.kind, SymbolKind::Trait);
    }

    // =======================================================================
    // Test Scenario 6: Extract constants
    // =======================================================================
    #[test]
    fn test_extract_php_global_constant() {
        let (_, symbols) = extract_fixture("src/main.php");
        let global = symbols
            .iter()
            .find(|s| s.name == "GLOBAL_CONST")
            .expect("GLOBAL_CONST not found");
        assert_eq!(global.kind, SymbolKind::Const);
    }

    #[test]
    fn test_extract_php_class_constant() {
        let (_, symbols) = extract_fixture("src/models.php");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let max_age = user
            .children
            .iter()
            .find(|c| c.name == "MAX_AGE")
            .expect("MAX_AGE not found");
        assert_eq!(max_age.kind, SymbolKind::Const);
        assert_eq!(max_age.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 7: Extract namespace
    // =======================================================================
    #[test]
    fn test_extract_php_namespace() {
        let (_, symbols) = extract_fixture("src/models.php");
        let ns = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("namespace not found");
        assert!(ns.name.contains("Models"));
    }

    // =======================================================================
    // Test Scenario 8: Body and signature extraction
    // =======================================================================
    #[test]
    fn test_extract_php_function_body() {
        let source =
            "<?php\nfunction greet(string $name): string {\n    return \"Hello, $name\";\n}\n";
        let symbols = parse_and_extract(source, "test.php");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("function greet(string $name)"));
        assert!(body.contains("return"));
    }

    #[test]
    fn test_extract_php_function_signature() {
        let source =
            "<?php\nfunction greet(string $name): string {\n    return \"Hello, $name\";\n}\n";
        let symbols = parse_and_extract(source, "test.php");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "function greet(string $name): string");
    }

    // =======================================================================
    // Test Scenario 9: Dispatch integration
    // =======================================================================
    #[test]
    fn test_extract_symbols_dispatch_php() {
        let source = "<?php\nfunction foo(): void {}\n";
        let mut parser = crate::Parser::for_language(Language::Php).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let symbols = crate::extract_symbols(source, &tree, Path::new("test.php"), Language::Php);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    // =======================================================================
    // Edge cases
    // =======================================================================
    #[test]
    fn test_extract_php_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.php");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_php_broken_source_no_panic() {
        let source = "<?php\nfunction good(): void {}\nfunction broken( {}\nclass Valid {}\n";
        let symbols = parse_and_extract(source, "broken.php");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_php_line_numbers_1_based() {
        let source = "<?php\nfunction first(): void {}\nfunction second(): void {}\n";
        let symbols = parse_and_extract(source, "test.php");
        let first = symbols
            .iter()
            .find(|s| s.name == "first")
            .expect("first not found");
        assert_eq!(first.line, 2);
        let second = symbols
            .iter()
            .find(|s| s.name == "second")
            .expect("second not found");
        assert_eq!(second.line, 3);
    }

    #[test]
    fn test_extract_php_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["src/main.php", "src/models.php"] {
            let (_, symbols) = extract_fixture(fixture);
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
                for child in &sym.children {
                    assert!(
                        child.body.is_some(),
                        "child {} of {} in {} should have a body",
                        child.name,
                        sym.name,
                        fixture
                    );
                    assert!(
                        child.signature.is_some(),
                        "child {} of {} in {} should have a signature",
                        child.name,
                        sym.name,
                        fixture
                    );
                }
            }
        }
    }
}
