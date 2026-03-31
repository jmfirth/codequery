//! C#-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — classes, structs,
//! interfaces, methods, properties, enums, and namespaces. C# visibility
//! is determined by explicit keywords (`public`/`private`/`protected`/`internal`).
//! Default is Private.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// C# language extractor.
pub struct CSharpExtractor;

impl LanguageExtractor for CSharpExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_from_node(root, source, file, &mut symbols, true);
        symbols
    }
}

/// Extract the full source body of a C# symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the signature of a C# symbol.
///
/// - **Method**: declaration line up to the opening `{`
/// - **Class/Struct/Interface/Enum**: header line up to the opening `{`
/// - **Const**: the full declaration line
/// - **Module** (namespace): the namespace declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface | SymbolKind::Enum => {
            extract_header_signature(body_text)
        }
        SymbolKind::Module => extract_namespace_signature(body_text),
        _ => body_text.lines().next().unwrap_or("").to_string(),
    }
}

/// Extract method/constructor signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        // Abstract methods end with `;`
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract class/struct/interface/enum header: everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract namespace signature.
fn extract_namespace_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract C# visibility from a node's `modifier` children.
///
/// Returns `Private` if no modifier is present (C# default).
fn csharp_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "modifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                match text {
                    "public" => return Visibility::Public,
                    "internal" => return Visibility::Crate,
                    "private" | "protected" => return Visibility::Private,
                    _ => {}
                }
            }
        }
    }
    Visibility::Private
}

/// Recursively extract symbols from a node and its children.
///
/// C# nests types inside namespaces and other types, so we need to
/// recurse into `declaration_list` and `namespace_declaration` nodes.
fn extract_from_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
    top_level: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        match child.kind() {
            "namespace_declaration" => {
                if let Some(sym) = extract_namespace(child, source, file) {
                    symbols.push(sym);
                }
                // Recurse into namespace body
                extract_namespace_body(child, source, file, symbols);
            }
            "class_declaration" => {
                if let Some(sym) = extract_class(child, source, file) {
                    symbols.push(sym);
                }
            }
            "struct_declaration" => {
                if let Some(sym) = extract_struct(child, source, file) {
                    symbols.push(sym);
                }
            }
            "interface_declaration" => {
                if let Some(sym) = extract_interface(child, source, file) {
                    symbols.push(sym);
                }
            }
            "enum_declaration" => {
                if let Some(sym) = extract_enum(child, source, file) {
                    symbols.push(sym);
                }
            }
            _ => {
                // Only recurse further at top level for compilation_unit children
                if top_level {
                    extract_from_node(child, source, file, symbols, false);
                }
            }
        }
    }
}

/// Recurse into a namespace's `declaration_list`.
fn extract_namespace_body(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            extract_from_node(child, source, file, symbols, false);
        }
    }
}

/// Extract a namespace declaration.
fn extract_namespace(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    // Find qualified_name or identifier child
    let mut cursor = node.walk();
    let name = node
        .children(&mut cursor)
        .find(|c| c.kind() == "qualified_name" || c.kind() == "identifier")
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

/// Extract a class declaration with its method/property children.
fn extract_class(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
    let children = extract_class_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Class);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Class,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a struct declaration with its members.
fn extract_struct(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
    let children = extract_class_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Struct);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Struct,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract an interface declaration with its method children.
fn extract_interface(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
    let children = extract_class_members(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Interface);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Interface,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract an enum declaration.
fn extract_enum(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Enum);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Enum,
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

/// Extract methods, constructors, and properties from a class/struct/interface `declaration_list`.
fn extract_class_members(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Vec<Symbol> {
    let mut members = Vec::new();

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
                        if let Some(sym) = extract_method(member, source, file) {
                            members.push(sym);
                        }
                    }
                    "constructor_declaration" => {
                        if let Some(sym) = extract_constructor(member, source, file) {
                            members.push(sym);
                        }
                    }
                    "property_declaration" => {
                        if let Some(sym) = extract_property(member, source, file) {
                            members.push(sym);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    members
}

/// Extract a method declaration.
fn extract_method(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
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

/// Extract a constructor declaration.
fn extract_constructor(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
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

/// Extract a property declaration.
fn extract_property(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = find_identifier(node, source)?;
    let visibility = csharp_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = body.lines().next().unwrap_or("").trim().to_string();
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Static,
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

/// Find the `identifier` child of a node.
fn find_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")
        .and_then(|c| c.utf8_text(source.as_bytes()).ok().map(String::from));
    result
}

/// Extract an XML doc comment preceding a definition node.
///
/// C# uses `///` XML doc comments. This looks for comment siblings
/// preceding the node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with("///") {
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

    /// Helper: parse C# source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let Ok(mut parser) = Parser::for_language(Language::CSharp) else {
            eprintln!("skipping: CSharp grammar not installed");
            return;
        };
        let tree = parser.parse(source.as_bytes()).unwrap();
        CSharpExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture C# project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/csharp_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let Ok(mut parser) = Parser::for_language(Language::CSharp) else {
            eprintln!("skipping: CSharp grammar not installed");
            return;
        };
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = CSharpExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // =======================================================================
    // Test Scenario 1: Extract namespace
    // =======================================================================
    #[test]
    fn test_extract_csharp_namespace() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let ns = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("namespace not found");
        assert!(ns.name.contains("Models"));
    }

    // =======================================================================
    // Test Scenario 2: Extract class with methods
    // =======================================================================
    #[test]
    fn test_extract_csharp_class() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        assert_eq!(user.visibility, Visibility::Public);
        assert!(!user.children.is_empty(), "User should have children");
    }

    #[test]
    fn test_extract_csharp_class_methods() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        let method_names: Vec<&str> = user
            .children
            .iter()
            .filter(|c| c.kind == SymbolKind::Method)
            .map(|c| c.name.as_str())
            .collect();
        assert!(method_names.contains(&"Greet"));
        assert!(method_names.contains(&"User")); // constructor
    }

    // =======================================================================
    // Test Scenario 3: Visibility keywords
    // =======================================================================
    #[test]
    fn test_extract_csharp_visibility_public() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        let greet = user
            .children
            .iter()
            .find(|c| c.name == "Greet")
            .expect("Greet not found");
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_csharp_visibility_private() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        let check = user
            .children
            .iter()
            .find(|c| c.name == "InternalCheck")
            .expect("InternalCheck not found");
        assert_eq!(check.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_csharp_visibility_protected() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        let validate = user
            .children
            .iter()
            .find(|c| c.name == "ValidateAge")
            .expect("ValidateAge not found");
        assert_eq!(validate.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_csharp_visibility_internal() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let helper = symbols
            .iter()
            .find(|s| s.name == "InternalHelper")
            .expect("InternalHelper not found");
        assert_eq!(helper.visibility, Visibility::Crate);
    }

    #[test]
    fn test_extract_csharp_default_visibility_private() {
        // C# default is private for class members without modifiers
        let source = "class Foo {\n    void Bar() {}\n}\n";
        let symbols = parse_and_extract(source, "test.cs");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let bar = foo
            .children
            .iter()
            .find(|c| c.name == "Bar")
            .expect("Bar not found");
        assert_eq!(bar.visibility, Visibility::Private);
    }

    // =======================================================================
    // Test Scenario 4: Extract interface
    // =======================================================================
    #[test]
    fn test_extract_csharp_interface() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let greeter = symbols
            .iter()
            .find(|s| s.name == "IGreeter")
            .expect("IGreeter not found");
        assert_eq!(greeter.kind, SymbolKind::Interface);
        assert_eq!(greeter.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 5: Extract struct
    // =======================================================================
    #[test]
    fn test_extract_csharp_struct() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
        assert_eq!(point.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 6: Extract enum
    // =======================================================================
    #[test]
    fn test_extract_csharp_enum() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        assert_eq!(color.kind, SymbolKind::Enum);
        assert_eq!(color.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 7: Extract property
    // =======================================================================
    #[test]
    fn test_extract_csharp_property() {
        let (_, symbols) = extract_fixture("src/Models.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User not found");
        let name_prop = user
            .children
            .iter()
            .find(|c| c.name == "Name")
            .expect("Name property not found");
        assert_eq!(name_prop.kind, SymbolKind::Static);
        assert_eq!(name_prop.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 8: Body and signature extraction
    // =======================================================================
    #[test]
    fn test_extract_csharp_method_body() {
        let source =
            "class Foo {\n    public string Greet() {\n        return \"Hello\";\n    }\n}\n";
        let symbols = parse_and_extract(source, "test.cs");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let greet = foo
            .children
            .iter()
            .find(|c| c.name == "Greet")
            .expect("Greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.contains("return \"Hello\""));
    }

    #[test]
    fn test_extract_csharp_method_signature() {
        let source =
            "class Foo {\n    public string Greet() {\n        return \"Hello\";\n    }\n}\n";
        let symbols = parse_and_extract(source, "test.cs");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let greet = foo
            .children
            .iter()
            .find(|c| c.name == "Greet")
            .expect("Greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "public string Greet()");
    }

    #[test]
    fn test_extract_csharp_class_signature() {
        let source = "public class User {\n}\n";
        let symbols = parse_and_extract(source, "test.cs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "public class User");
    }

    // =======================================================================
    // Test Scenario 9: Dispatch integration
    // =======================================================================
    #[test]
    fn test_extract_symbols_dispatch_csharp() {
        let source = "public class Foo {}\n";
        let Ok(mut parser) = Parser::for_language(Language::CSharp) else {
            eprintln!("skipping: CSharp grammar not installed");
            return;
        };
        let tree = parser.parse(source.as_bytes()).unwrap();
        let symbols = crate::extract_symbols(source, &tree, Path::new("test.cs"), Language::CSharp);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
    }

    // =======================================================================
    // Edge cases
    // =======================================================================
    #[test]
    fn test_extract_csharp_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.cs");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_csharp_broken_source_no_panic() {
        let source = "public class Good {}\npublic class Broken {\npublic class Valid {}\n";
        let symbols = parse_and_extract(source, "broken.cs");
        assert!(
            symbols.iter().any(|s| s.name == "Good"),
            "should find 'Good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_csharp_line_numbers_1_based() {
        let source = "public class First {}\npublic class Second {}\n";
        let symbols = parse_and_extract(source, "test.cs");
        let first = symbols
            .iter()
            .find(|s| s.name == "First")
            .expect("First not found");
        assert_eq!(first.line, 1);
        let second = symbols
            .iter()
            .find(|s| s.name == "Second")
            .expect("Second not found");
        assert_eq!(second.line, 2);
    }

    #[test]
    fn test_extract_csharp_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["src/Models.cs"] {
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
