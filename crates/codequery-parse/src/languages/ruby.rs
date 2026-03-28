//! Ruby-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions (top-level
//! `def`), methods (inside classes/modules), classes, modules, and constants
//! (`SCREAMING_CASE`). Ruby visibility is determined by convention: methods
//! starting with `_` are private, all others are public.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Ruby language extractor.
pub struct RubyExtractor;

impl LanguageExtractor for RubyExtractor {
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

/// Extract the full source body of a Ruby symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the signature of a Ruby symbol.
///
/// - **Function/Method**: the `def` line (first line of the definition)
/// - **Class**: the `class` header line
/// - **Module**: the `module` header line
/// - **Const**: the full assignment line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_def_signature(body_text),
        SymbolKind::Class => extract_class_or_module_signature(body_text, "class"),
        SymbolKind::Module => extract_class_or_module_signature(body_text, "module"),
        SymbolKind::Const => extract_const_signature(body_text),
        _ => body_text.lines().next().unwrap_or("").to_string(),
    }
}

/// Extract `def` line signature.
fn extract_def_signature(body: &str) -> String {
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Extract class/module header line.
fn extract_class_or_module_signature(body: &str, keyword: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(keyword) {
            return trimmed.trim_end().to_string();
        }
    }
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Extract constant signature: the full assignment line.
fn extract_const_signature(body: &str) -> String {
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Determine Ruby visibility by naming convention.
///
/// Methods starting with `_` are private; all others are public.
fn ruby_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
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
        "method" => {
            if let Some(sym) = extract_method(node, source, file, false) {
                symbols.push(sym);
            }
        }
        "singleton_method" => {
            if let Some(sym) = extract_singleton_method(node, source, file) {
                symbols.push(sym);
            }
        }
        "class" => {
            if let Some(sym) = extract_class(node, source, file) {
                symbols.push(sym);
            }
        }
        "module" => {
            if let Some(sym) = extract_module(node, source, file) {
                symbols.push(sym);
            }
        }
        "assignment" => {
            if let Some(sym) = extract_constant_assignment(node, source, file) {
                symbols.push(sym);
            }
        }
        _ => {}
    }
}

/// Extract a `method` node (Ruby `def`).
///
/// When `is_method` is true, marks as Method; otherwise Function.
fn extract_method(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    is_method: bool,
) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let kind = if is_method {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    Some(Symbol {
        name: name.clone(),
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: ruby_visibility(&name),
        children: vec![],
        doc: extract_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a `singleton_method` (Ruby `def self.name`).
fn extract_singleton_method(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let kind = SymbolKind::Function;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    Some(Symbol {
        name: name.clone(),
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: ruby_visibility(&name),
        children: vec![],
        doc: extract_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a class definition with its method children.
fn extract_class(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_constant_name(node, source)?;
    let children = extract_body_methods(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Class);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Class,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: ruby_visibility(&name),
        children,
        doc: extract_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a module definition with its method children.
fn extract_module(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_constant_name(node, source)?;
    let children = extract_body_methods(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Module);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Module,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: ruby_visibility(&name),
        children,
        doc: extract_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract methods from a class or module body.
fn extract_body_methods(parent: tree_sitter::Node<'_>, source: &str, file: &Path) -> Vec<Symbol> {
    let mut methods = Vec::new();

    // Find the body_statement child
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() == "body_statement" {
            let mut body_cursor = child.walk();
            for body_child in child.children(&mut body_cursor) {
                if body_child.is_error() || body_child.is_missing() {
                    continue;
                }
                match body_child.kind() {
                    "method" => {
                        if let Some(sym) = extract_method(body_child, source, file, true) {
                            methods.push(sym);
                        }
                    }
                    "singleton_method" => {
                        if let Some(sym) = extract_singleton_method(body_child, source, file) {
                            methods.push(sym);
                        }
                    }
                    "class" => {
                        if let Some(sym) = extract_class(body_child, source, file) {
                            methods.push(sym);
                        }
                    }
                    "assignment" => {
                        if let Some(sym) = extract_constant_assignment(body_child, source, file) {
                            methods.push(sym);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    methods
}

/// Extract a constant assignment (`SCREAMING_CASE` = value).
fn extract_constant_assignment(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    // The `assignment` node has a `constant` child on the left
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    // First child should be a `constant` node
    let left = children.first()?;
    if left.kind() != "constant" {
        return None;
    }
    let name = left.utf8_text(source.as_bytes()).ok()?.to_string();

    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Const);
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
        body: Some(body),
        signature: Some(signature),
    })
}

/// Find the `identifier` child of a method node.
fn find_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok().map(String::from));
    result
}

/// Find the `constant` child of a class/module node (the name).
fn find_constant_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|c| c.kind() == "constant" || c.kind() == "scope_resolution")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok().map(String::from));
    result
}

/// Extract a comment preceding a definition node.
///
/// In Ruby, comments are `#` lines. This looks for `comment` siblings
/// immediately preceding the node.
fn extract_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                doc_lines.push(trimmed.to_string());
                sibling = sib.prev_sibling();
                continue;
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

    /// Helper: parse Ruby source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Ruby).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        RubyExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture ruby project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ruby_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Ruby).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = RubyExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // =======================================================================
    // Test Scenario 1: Extract top-level function -> Function
    // =======================================================================
    #[test]
    fn test_extract_ruby_top_level_function() {
        let (_, symbols) = extract_fixture("lib/main.rb");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 2: Extract class with methods
    // =======================================================================
    #[test]
    fn test_extract_ruby_class_with_methods() {
        let (_, symbols) = extract_fixture("lib/models.rb");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.kind, SymbolKind::Class);
        assert_eq!(user.visibility, Visibility::Public);
        assert!(
            !user.children.is_empty(),
            "User should have method children"
        );

        let method_names: Vec<&str> = user.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"initialize"));
        assert!(method_names.contains(&"greet"));
    }

    #[test]
    fn test_extract_ruby_class_method_children_are_method_kind() {
        let (_, symbols) = extract_fixture("lib/models.rb");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        for child in &user.children {
            if child.kind != SymbolKind::Const && child.kind != SymbolKind::Class {
                assert_eq!(
                    child.kind,
                    SymbolKind::Method,
                    "child {} should be Method, got {:?}",
                    child.name,
                    child.kind
                );
            }
        }
    }

    // =======================================================================
    // Test Scenario 3: Extract module
    // =======================================================================
    #[test]
    fn test_extract_ruby_module() {
        let (_, symbols) = extract_fixture("lib/utils.rb");
        let utils = symbols
            .iter()
            .find(|s| s.name == "Utils")
            .expect("Utils not found");
        assert_eq!(utils.kind, SymbolKind::Module);
        assert_eq!(utils.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 4: Private method (underscore prefix)
    // =======================================================================
    #[test]
    fn test_extract_ruby_private_method() {
        let (_, symbols) = extract_fixture("lib/models.rb");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let priv_method = user
            .children
            .iter()
            .find(|c| c.name == "_internal_check")
            .expect("_internal_check not found");
        assert_eq!(priv_method.kind, SymbolKind::Method);
        assert_eq!(priv_method.visibility, Visibility::Private);
    }

    // =======================================================================
    // Test Scenario 5: Constants (SCREAMING_CASE)
    // =======================================================================
    #[test]
    fn test_extract_ruby_constant() {
        let (_, symbols) = extract_fixture("lib/models.rb");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let max_age = user
            .children
            .iter()
            .find(|c| c.name == "MAX_AGE")
            .expect("MAX_AGE not found in User");
        assert_eq!(max_age.kind, SymbolKind::Const);
    }

    // =======================================================================
    // Test Scenario 6: Body and signature extraction
    // =======================================================================
    #[test]
    fn test_extract_ruby_function_body() {
        let source = "def greet(name)\n  \"Hello, #{name}\"\nend\n";
        let symbols = parse_and_extract(source, "test.rb");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("def greet(name)"));
        assert!(body.ends_with("end"));
    }

    #[test]
    fn test_extract_ruby_function_signature() {
        let source = "def greet(name)\n  \"Hello, #{name}\"\nend\n";
        let symbols = parse_and_extract(source, "test.rb");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "def greet(name)");
    }

    #[test]
    fn test_extract_ruby_class_signature() {
        let source = "class User < Base\n  def initialize\n  end\nend\n";
        let symbols = parse_and_extract(source, "test.rb");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "class User < Base");
    }

    // =======================================================================
    // Test Scenario 7: Dispatch integration
    // =======================================================================
    #[test]
    fn test_extract_symbols_dispatch_ruby() {
        let source = "def foo\n  42\nend\n";
        let mut parser = crate::Parser::for_language(Language::Ruby).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let symbols = crate::extract_symbols(source, &tree, Path::new("test.rb"), Language::Ruby);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    // =======================================================================
    // Edge cases
    // =======================================================================
    #[test]
    fn test_extract_ruby_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.rb");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_ruby_broken_source_no_panic() {
        let source = "def good\n  42\nend\ndef broken(\nclass Valid\nend\n";
        let symbols = parse_and_extract(source, "broken.rb");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_ruby_line_numbers_1_based() {
        let source = "def first\nend\n\ndef second\nend\n";
        let symbols = parse_and_extract(source, "test.rb");
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

    #[test]
    fn test_extract_ruby_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["lib/main.rb", "lib/models.rb", "lib/utils.rb"] {
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
