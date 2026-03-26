//! C++-specific symbol extraction from tree-sitter ASTs.
//!
//! Extends C extraction with support for classes, methods, namespaces,
//! and access specifiers (`public:`, `private:`, `protected:`). Uses a
//! separate extractor from C because C++ has fundamentally different
//! scoping and visibility semantics.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// C++ language extractor.
pub struct CppExtractor;

impl LanguageExtractor for CppExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        walk_children(&mut symbols, root, source, file, Visibility::Public);
        symbols
    }
}

/// Walk children of a node, descending into preprocessor blocks.
///
/// Preprocessor conditionals (`#ifdef`, `#ifndef`, `#if`, `#elif`) wrap
/// their content as children. We descend into them to find declarations.
fn walk_children(
    symbols: &mut Vec<Symbol>,
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    default_visibility: Visibility,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        match child.kind() {
            "preproc_ifdef" | "preproc_if" | "preproc_elif" => {
                walk_children(symbols, child, source, file, default_visibility);
            }
            _ => {
                extract_node(symbols, child, source, file, default_visibility);
            }
        }
    }
}

/// Extract the full source body of a symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a C++ symbol.
///
/// The signature varies by symbol kind:
/// - **Function/Method**: declaration line up to the opening `{`, trimmed
/// - **Class/Struct/Enum**: full definition (header + body)
/// - **Module (namespace)**: just the namespace header
/// - **Type**: the full typedef/using line
/// - **Const (macro)**: the `#define` line
/// - **Static (variable)**: the full declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Module => extract_namespace_signature(body_text),
        SymbolKind::Type | SymbolKind::Static | SymbolKind::Const => {
            extract_single_line_signature(body_text)
        }
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        // Declaration without body (pure virtual, etc.)
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Find the first top-level `{` in source text, skipping braces inside `<...>`.
fn find_top_level_brace(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '{' if angle_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract namespace signature: just the header before `{`.
fn extract_namespace_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract a single-line signature for typedefs, macros, and variables.
fn extract_single_line_signature(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Extract a symbol from a node at any level, dispatching by node type.
///
/// The `default_visibility` parameter carries the current access specifier
/// context when extracting children inside a class body.
#[allow(clippy::too_many_lines)]
// All node-type match arms for extraction; splitting would obscure the logic
fn extract_node(
    symbols: &mut Vec<Symbol>,
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    default_visibility: Visibility,
) {
    let kind_str = node.kind();
    match kind_str {
        "function_definition" => {
            let Some(name) = extract_function_name(node, source) else {
                return;
            };
            // Skip qualified names like `Dog::speak` — those are out-of-class method definitions,
            // already captured as declarations inside the class body
            if name.contains("::") {
                return;
            }
            let kind = SymbolKind::Function;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "class_specifier" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let children = extract_class_members(node, source, file);
            let kind = SymbolKind::Class;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children,
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "struct_specifier" => {
            if node.child_by_field_name("body").is_none() {
                return;
            }
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let kind = SymbolKind::Struct;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "enum_specifier" => {
            if node.child_by_field_name("body").is_none() {
                return;
            }
            let Some(name) = extract_enum_name(node, source) else {
                return;
            };
            let kind = SymbolKind::Enum;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "namespace_definition" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let mut children = Vec::new();
            if let Some(body_node) = node.child_by_field_name("body") {
                let mut cursor = body_node.walk();
                for child in body_node.children(&mut cursor) {
                    if child.is_error() || child.is_missing() {
                        continue;
                    }
                    extract_node(&mut children, child, source, file, Visibility::Public);
                }
            }
            let kind = SymbolKind::Module;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Module,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Public,
                children,
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "type_definition" => {
            let Some(name) = extract_typedef_name(node, source) else {
                return;
            };
            let kind = SymbolKind::Type;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "alias_declaration" => {
            // C++ `using Name = Type;`
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let kind = SymbolKind::Type;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "declaration" => {
            let Some(name) = extract_declaration_name(node, source) else {
                return;
            };
            let kind = SymbolKind::Static;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: default_visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "preproc_def" | "preproc_function_def" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let kind = SymbolKind::Const;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name,
                kind,
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
        _ => {}
    }
}

/// Extract members from a class body, tracking access specifier visibility.
///
/// C++ classes default to `private:` access. As `access_specifier` nodes
/// are encountered, the current visibility changes for subsequent members.
fn extract_class_members(
    class_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return members;
    };

    // C++ class members default to private
    let mut current_visibility = Visibility::Private;
    let mut cursor = body.walk();

    for child in body.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }

        match child.kind() {
            "access_specifier" => {
                current_visibility = parse_access_specifier(child, source);
            }
            "function_definition" => {
                let Some(name) = extract_function_name(child, source) else {
                    continue;
                };
                let kind = SymbolKind::Method;
                let method_body = extract_body(source, &child);
                let method_sig = extract_signature(source, &child, kind);
                members.push(Symbol {
                    name,
                    kind,
                    file: file.to_path_buf(),
                    line: child.start_position().row + 1,
                    column: child.start_position().column,
                    end_line: child.end_position().row + 1,
                    visibility: current_visibility,
                    children: vec![],
                    doc: extract_doc_comment(child, source),
                    body: Some(method_body),
                    signature: Some(method_sig),
                });
            }
            "declaration" | "field_declaration" => {
                // Method declarations (without body) or field declarations
                if let Some(method) =
                    extract_method_declaration(child, source, file, current_visibility)
                {
                    members.push(method);
                }
            }
            _ => {}
        }
    }

    members
}

/// Extract a method declaration from a `field_declaration` or `declaration`
/// node inside a class body.
///
/// Only extracts declarations that look like function declarations
/// (i.e., have a `function_declarator` inside them).
fn extract_method_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    // Look for a function_declarator child inside the declaration
    let declarator = node.child_by_field_name("declarator")?;
    let func_decl = find_function_declarator(declarator)?;
    let name_node = func_decl.child_by_field_name("declarator")?;
    let name = extract_identifier_name(name_node, source)?;

    let kind = SymbolKind::Method;
    let body = extract_body(source, &node);
    let signature = extract_single_line_signature(&body);
    Some(Symbol {
        name,
        kind,
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

/// Find a `function_declarator` anywhere in a declarator node subtree.
fn find_function_declarator(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.kind() == "function_declarator" {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_declarator(child) {
            return Some(found);
        }
    }
    None
}

/// Parse an `access_specifier` node into a `Visibility` value.
fn parse_access_specifier(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let text = node
        .utf8_text(source.as_bytes())
        .unwrap_or("")
        .trim_end_matches(':')
        .trim();
    match text {
        "public" => Visibility::Public,
        "protected" => Visibility::Crate, // Map protected to Crate as closest approximation
        _ => Visibility::Private,
    }
}

/// Extract the function name from a `function_definition` node.
///
/// The declarator chain can include `reference_declarator`, `pointer_declarator`,
/// etc. wrapping the `function_declarator`. We find the `function_declarator`
/// anywhere in the subtree and extract its name.
fn extract_function_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    let func_decl = find_function_declarator(declarator)?;
    let name_node = func_decl.child_by_field_name("declarator")?;
    extract_identifier_name(name_node, source)
}

/// Extract the identifier name from a node, handling qualified names.
///
/// Handles plain identifiers, field identifiers, qualified names (e.g., `Dog::speak`),
/// destructor names, and operator names. Falls back to raw node text.
fn extract_identifier_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract the enum name, handling both plain `enum` and `enum class`.
fn extract_enum_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    node_field_text(node, "name", source)
}

/// Extract the typedef name from a `type_definition` node.
fn extract_typedef_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    if declarator.kind() == "function_declarator" {
        let inner = declarator.child_by_field_name("declarator")?;
        let text = inner.utf8_text(source.as_bytes()).ok()?;
        let name = text
            .trim_start_matches('(')
            .trim_start_matches('*')
            .trim_end_matches(')');
        Some(name.to_string())
    } else {
        declarator
            .utf8_text(source.as_bytes())
            .ok()
            .map(String::from)
    }
}

/// Extract the variable name from a file-level `declaration` node.
fn extract_declaration_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    match declarator.kind() {
        "init_declarator" => {
            let name_node = declarator.child_by_field_name("declarator")?;
            extract_identifier_name(name_node, source)
        }
        _ => extract_identifier_name(declarator, source),
    }
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract doc comments preceding a definition node.
///
/// Looks for `comment` siblings immediately before the node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                doc_lines.push(text.trim_end().to_string());
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

    /// Helper: parse C++ source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Cpp).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        CppExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the C++ fixture project.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cpp_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(filename: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(filename);
        let mut parser = Parser::for_language(Language::Cpp).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = CppExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -------------------------------------------------------------------
    // Scenario 6: C++: Extract class with methods -> Class with Method children
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_class_with_methods() {
        let source = r#"class Animal {
public:
    void speak() const {}
    int get_age() const { return 0; }
private:
    int age_;
};"#;
        let symbols = parse_and_extract(source, "test.cpp");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        assert_eq!(animal.kind, SymbolKind::Class);

        // Should have method children
        let method_names: Vec<&str> = animal
            .children
            .iter()
            .filter(|c| c.kind == SymbolKind::Method)
            .map(|c| c.name.as_str())
            .collect();
        assert!(
            method_names.contains(&"speak"),
            "speak method not found, got: {method_names:?}"
        );
        assert!(
            method_names.contains(&"get_age"),
            "get_age method not found, got: {method_names:?}"
        );
    }

    #[test]
    fn test_extract_cpp_class_from_fixture() {
        let (_, symbols) = extract_fixture("models.hpp");
        // The namespace should contain Animal and Dog as children
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib" && s.kind == SymbolKind::Module)
            .expect("mylib namespace not found");

        let animal = ns
            .children
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found in mylib");
        assert_eq!(animal.kind, SymbolKind::Class);
        assert!(!animal.children.is_empty(), "Animal should have methods");
    }

    // -------------------------------------------------------------------
    // Scenario 7: C++: Extract namespace -> Module
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_namespace_as_module() {
        let source = "namespace mylib {\nvoid foo() {}\n}\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        assert_eq!(ns.kind, SymbolKind::Module);
        assert_eq!(ns.visibility, Visibility::Public);

        // Namespace children should contain the function
        let foo = ns
            .children
            .iter()
            .find(|c| c.name == "foo")
            .expect("foo not found in namespace");
        assert_eq!(foo.kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_cpp_namespace_from_fixture() {
        let (_, symbols) = extract_fixture("models.hpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        assert_eq!(ns.kind, SymbolKind::Module);
        assert!(!ns.children.is_empty(), "namespace should have children");
    }

    // -------------------------------------------------------------------
    // Scenario 8: C++: Access specifier visibility
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_access_specifier_visibility() {
        let source = r#"class Foo {
public:
    void pub_method() {}
private:
    void priv_method() {}
protected:
    void prot_method() {}
};"#;
        let symbols = parse_and_extract(source, "test.cpp");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");

        let pub_m = foo
            .children
            .iter()
            .find(|c| c.name == "pub_method")
            .expect("pub_method not found");
        assert_eq!(pub_m.visibility, Visibility::Public);

        let priv_m = foo
            .children
            .iter()
            .find(|c| c.name == "priv_method")
            .expect("priv_method not found");
        assert_eq!(priv_m.visibility, Visibility::Private);

        let prot_m = foo
            .children
            .iter()
            .find(|c| c.name == "prot_method")
            .expect("prot_method not found");
        assert_eq!(prot_m.visibility, Visibility::Crate); // protected maps to Crate
    }

    #[test]
    fn test_extract_cpp_class_default_visibility_is_private() {
        // Members before any access specifier in a class are private
        let source = "class Bar {\n    void hidden() {}\npublic:\n    void visible() {}\n};\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let bar = symbols
            .iter()
            .find(|s| s.name == "Bar")
            .expect("Bar not found");

        let hidden = bar
            .children
            .iter()
            .find(|c| c.name == "hidden")
            .expect("hidden not found");
        assert_eq!(hidden.visibility, Visibility::Private);

        let visible = bar
            .children
            .iter()
            .find(|c| c.name == "visible")
            .expect("visible not found");
        assert_eq!(visible.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_cpp_reference_return_function() {
        let source = r#"class Foo {
public:
    const std::string& get_name() const { return name_; }
    int get_age() const { return 0; }
};"#;
        let symbols = parse_and_extract(source, "test.cpp");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let child_names: Vec<&str> = foo.children.iter().map(|c| c.name.as_str()).collect();
        assert!(
            child_names.contains(&"get_name"),
            "get_name not found, children: {child_names:?}"
        );
        assert!(
            child_names.contains(&"get_age"),
            "get_age not found, children: {child_names:?}"
        );
    }

    // -------------------------------------------------------------------
    // Scenario: Access specifiers in fixture file
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_fixture_access_specifiers() {
        let (_, symbols) = extract_fixture("models.hpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        let animal = ns
            .children
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");

        // get_name should be public (after public: specifier)
        let get_name = animal
            .children
            .iter()
            .find(|c| c.name == "get_name")
            .expect("get_name not found");
        assert_eq!(get_name.visibility, Visibility::Public);

        // log_action should be protected (after protected: specifier)
        let log_action = animal
            .children
            .iter()
            .find(|c| c.name == "log_action")
            .expect("log_action not found");
        assert_eq!(log_action.visibility, Visibility::Crate); // protected -> Crate
    }

    // -------------------------------------------------------------------
    // Scenario 9b: Body and signature for C++ symbols
    // -------------------------------------------------------------------
    #[test]
    fn test_cpp_function_body_and_signature() {
        let source = "void greet() {\n    return;\n}\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");

        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("void greet()"));
        assert!(body.contains("return;"));

        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "void greet()");
    }

    #[test]
    fn test_cpp_class_signature_is_full_definition() {
        let source = "class Foo {\npublic:\n    void bar() {}\n};\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let sig = foo.signature.as_deref().expect("signature should be Some");
        assert!(sig.contains("class Foo"));
        assert!(sig.contains("void bar()"));
    }

    #[test]
    fn test_cpp_namespace_signature_is_header() {
        let source = "namespace mylib {\nvoid foo() {}\n}\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        let sig = ns.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "namespace mylib");
    }

    #[test]
    fn test_cpp_method_body_and_signature() {
        let source =
            "class Foo {\npublic:\n    int compute(int x) {\n        return x * 2;\n    }\n};\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let compute = foo
            .children
            .iter()
            .find(|c| c.name == "compute")
            .expect("compute not found");

        let body = compute.body.as_deref().expect("body should be Some");
        assert!(body.contains("return x * 2"));

        let sig = compute
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "int compute(int x)");
    }

    // -------------------------------------------------------------------
    // Enum class extraction
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_enum_class() {
        let source = "enum class Color {\n    Red,\n    Green,\n    Blue\n};\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        assert_eq!(color.kind, SymbolKind::Enum);
        let body = color.body.as_deref().expect("body should be Some");
        assert!(body.contains("Red"));
        assert!(body.contains("Green"));
        assert!(body.contains("Blue"));
    }

    #[test]
    fn test_extract_cpp_enum_class_from_fixture() {
        let (_, symbols) = extract_fixture("models.hpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        let color = ns
            .children
            .iter()
            .find(|c| c.name == "Color")
            .expect("Color not found in namespace");
        assert_eq!(color.kind, SymbolKind::Enum);
    }

    // -------------------------------------------------------------------
    // Free function outside namespace
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_free_function() {
        let (_, symbols) = extract_fixture("main.cpp");
        let free_fn = symbols
            .iter()
            .find(|s| s.name == "free_function")
            .expect("free_function not found");
        assert_eq!(free_fn.kind, SymbolKind::Function);
        assert_eq!(free_fn.visibility, Visibility::Public);
    }

    // -------------------------------------------------------------------
    // All fixture symbols have body and signature
    // -------------------------------------------------------------------
    #[test]
    fn test_all_cpp_fixture_symbols_have_body_and_signature() {
        for fixture in &["main.cpp", "models.hpp", "models.cpp"] {
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

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.cpp");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_cpp_broken_source_partial_results() {
        let source = "void good() {}\nclass Broken {\nvoid bad( {}\n};\n";
        let symbols = parse_and_extract(source, "broken.cpp");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_cpp_line_numbers_are_1_based() {
        let source = "void first() {}\nvoid second() {}\n";
        let symbols = parse_and_extract(source, "test.cpp");
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
        assert_eq!(second.line, 2);
    }

    // -------------------------------------------------------------------
    // Macro extraction (shared with C)
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_macro() {
        let source = "#define MAX_SIZE 1024\n#define SQUARE(x) ((x) * (x))\n";
        let symbols = parse_and_extract(source, "test.cpp");
        let max_size = symbols
            .iter()
            .find(|s| s.name == "MAX_SIZE")
            .expect("MAX_SIZE not found");
        assert_eq!(max_size.kind, SymbolKind::Const);

        let square = symbols
            .iter()
            .find(|s| s.name == "SQUARE")
            .expect("SQUARE not found");
        assert_eq!(square.kind, SymbolKind::Const);
    }

    // -------------------------------------------------------------------
    // Doc comments
    // -------------------------------------------------------------------
    #[test]
    fn test_extract_cpp_doc_comment() {
        let (_, symbols) = extract_fixture("models.hpp");
        let ns = symbols
            .iter()
            .find(|s| s.name == "mylib")
            .expect("mylib not found");
        let animal = ns
            .children
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        assert!(animal.doc.is_some());
        assert!(animal
            .doc
            .as_deref()
            .unwrap()
            .contains("Base class for all animals"));
    }
}
