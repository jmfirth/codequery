//! Swift-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, classes,
//! structs, protocols, enums, extensions, and methods within them.
//! Also provides body and signature extraction for each symbol kind.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Swift language extractor.
pub struct SwiftExtractor;

impl LanguageExtractor for SwiftExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        let children: Vec<_> = root.children(&mut cursor).collect();
        for child in children {
            if child.is_error() || child.is_missing() {
                continue;
            }
            if let Some(sym) = extract_top_level(child, source, file) {
                symbols.push(sym);
            }
        }

        symbols
    }
}

/// Extract the full source body of a symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a symbol.
///
/// - **Function/Method**: declaration up to the opening `{`, trimmed
/// - **Class/Struct/Enum/Interface**: header up to the opening `{`
/// - **Module (extension)**: the extension header
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Enum
        | SymbolKind::Interface
        | SymbolKind::Module => extract_header_signature(body_text),
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract header signature for types: everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Find the position of the first top-level `{` in source text.
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

/// Extract a top-level symbol from a node.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let kind_str = node.kind();
    match kind_str {
        "function_declaration" => {
            let name = node_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let kind = SymbolKind::Function;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
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
        "class_declaration" => extract_class_declaration(node, source, file),
        "protocol_declaration" => {
            let name = node_type_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let children = extract_protocol_members(node, source, file);
            let kind = SymbolKind::Interface;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name,
                kind,
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
        _ => None,
    }
}

/// Extract a `class_declaration`, which covers class, struct, enum, and extension in Swift.
fn extract_class_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    // Determine the actual keyword used
    let mut cursor = node.walk();
    let children_vec: Vec<_> = node.children(&mut cursor).collect();

    let mut keyword = None;
    let mut name = None;
    for child in &children_vec {
        match child.kind() {
            "class" => keyword = Some("class"),
            "struct" => keyword = Some("struct"),
            "enum" => keyword = Some("enum"),
            "extension" => keyword = Some("extension"),
            "type_identifier" => {
                if name.is_none() {
                    name = child.utf8_text(source.as_bytes()).ok().map(String::from);
                }
            }
            "user_type" => {
                // extension uses user_type for the extended type
                if name.is_none() {
                    name = child.utf8_text(source.as_bytes()).ok().map(String::from);
                }
            }
            _ => {}
        }
    }

    let name = name?;
    let visibility = extract_visibility(node, source);

    let (kind, children) = match keyword {
        Some("class") => {
            let members = extract_class_body_members(node, source, file);
            (SymbolKind::Class, members)
        }
        Some("struct") => {
            let members = extract_class_body_members(node, source, file);
            (SymbolKind::Struct, members)
        }
        Some("enum") => (SymbolKind::Enum, vec![]),
        Some("extension") => {
            let members = extract_class_body_members(node, source, file);
            (SymbolKind::Module, members)
        }
        _ => return None,
    };

    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);

    Some(Symbol {
        name,
        kind,
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

/// Extract methods from a class/struct/extension body.
fn extract_class_body_members(
    decl_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let mut cursor = decl_node.walk();
    let children: Vec<_> = decl_node.children(&mut cursor).collect();

    for child in children {
        if child.kind() == "class_body" {
            let mut body_cursor = child.walk();
            let body_children: Vec<_> = child.children(&mut body_cursor).collect();
            for body_child in body_children {
                if body_child.is_error() || body_child.is_missing() {
                    continue;
                }
                if body_child.kind() == "function_declaration" {
                    if let Some(method) = extract_method(body_child, source, file) {
                        members.push(method);
                    }
                }
            }
        }
    }

    members
}

/// Extract methods from a protocol body.
fn extract_protocol_members(
    proto_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let mut cursor = proto_node.walk();
    let children: Vec<_> = proto_node.children(&mut cursor).collect();

    for child in children {
        if child.kind() == "protocol_body" {
            let mut body_cursor = child.walk();
            let body_children: Vec<_> = child.children(&mut body_cursor).collect();
            for body_child in body_children {
                if body_child.is_error() || body_child.is_missing() {
                    continue;
                }
                if body_child.kind() == "protocol_function_declaration" {
                    if let Some(method) = extract_protocol_method(body_child, source, file) {
                        members.push(method);
                    }
                }
            }
        }
    }

    members
}

/// Extract a method from a `function_declaration` inside a class/struct/extension body.
fn extract_method(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = node_identifier(node, source)?;
    let visibility = extract_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Method);

    Some(Symbol {
        name,
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

/// Extract a protocol method declaration.
fn extract_protocol_method(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let name = node_identifier(node, source)?;
    let body = extract_body(source, &node);
    let signature = body.trim().trim_end_matches(';').trim().to_string();

    Some(Symbol {
        name,
        kind: SymbolKind::Method,
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

/// Extract the `simple_identifier` from a function declaration.
fn node_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Extract the `type_identifier` from a class/protocol declaration.
fn node_type_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "type_identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Extract visibility from a Swift node by examining `modifiers/visibility_modifier`.
///
/// Swift visibility mapping:
/// - `public` -> `Public`
/// - `private` -> `Private`
/// - `fileprivate` -> `Private`
/// - `internal` (default) -> `Crate`
/// - no modifier -> `Crate` (Swift default is `internal`)
fn extract_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "modifiers" {
            return parse_modifiers_visibility(child, source);
        }
    }
    // No modifiers = internal (default)
    Visibility::Crate
}

/// Parse visibility from a `modifiers` node.
fn parse_modifiers_visibility(modifiers: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = modifiers.walk();
    let children: Vec<_> = modifiers.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            if text.contains("public") {
                return Visibility::Public;
            }
            if text.contains("private") || text.contains("fileprivate") {
                return Visibility::Private;
            }
            if text.contains("internal") {
                return Visibility::Crate;
            }
        }
    }
    Visibility::Crate
}

/// Extract doc comments (lines starting with `///`) preceding a node.
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

    /// Helper: parse source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Swift).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        SwiftExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    // -----------------------------------------------------------------------
    // Functions
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_public_function() {
        let source = "public func greet(name: String) -> String { return name }";
        let symbols = parse_and_extract(source, "test.swift");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_swift_private_function() {
        let source = "private func helper() {}";
        let symbols = parse_and_extract(source, "test.swift");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_swift_fileprivate_function() {
        let source = "fileprivate func fileHelper() {}";
        let symbols = parse_and_extract(source, "test.swift");
        let fh = symbols
            .iter()
            .find(|s| s.name == "fileHelper")
            .expect("fileHelper not found");
        assert_eq!(fh.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_swift_internal_default_function() {
        let source = "func defaultVisibility() {}";
        let symbols = parse_and_extract(source, "test.swift");
        let f = symbols
            .iter()
            .find(|s| s.name == "defaultVisibility")
            .expect("defaultVisibility not found");
        assert_eq!(f.visibility, Visibility::Crate);
    }

    // -----------------------------------------------------------------------
    // Classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_class_with_methods() {
        let source = "class Animal {\n  func speak() -> String { return \"\" }\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.children.len(), 1);
        assert_eq!(animal.children[0].name, "speak");
        assert_eq!(animal.children[0].kind, SymbolKind::Method);
    }

    // -----------------------------------------------------------------------
    // Structs
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_struct() {
        let source = "struct Point {\n  var x: Double\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    // -----------------------------------------------------------------------
    // Protocols
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_protocol_with_methods() {
        let source = "protocol Drawable {\n  func draw()\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Interface);
        assert_eq!(drawable.children.len(), 1);
        assert_eq!(drawable.children[0].name, "draw");
    }

    // -----------------------------------------------------------------------
    // Enums
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_enum() {
        let source = "enum Direction {\n  case north\n  case south\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let dir = symbols
            .iter()
            .find(|s| s.name == "Direction")
            .expect("Direction not found");
        assert_eq!(dir.kind, SymbolKind::Enum);
    }

    // -----------------------------------------------------------------------
    // Extensions
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_extension() {
        let source = "extension String {\n  func greet() -> String { return \"Hello\" }\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let ext = symbols
            .iter()
            .find(|s| s.name == "String")
            .expect("String extension not found");
        assert_eq!(ext.kind, SymbolKind::Module);
        assert_eq!(ext.children.len(), 1);
        assert_eq!(ext.children[0].name, "greet");
    }

    // -----------------------------------------------------------------------
    // Body and Signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_function_body_and_signature() {
        let source = "public func greet(name: String) -> String {\n  return name\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");

        let body = greet.body.as_ref().expect("body should be present");
        assert!(body.contains("return name"));

        let sig = greet
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(sig.contains("public func greet(name: String) -> String"));
        assert!(!sig.contains('{'));
    }

    #[test]
    fn test_extract_swift_class_signature() {
        let source = "class Animal {\n  func speak() {}\n}";
        let symbols = parse_and_extract(source, "test.swift");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        let sig = animal.signature.as_ref().expect("sig should be present");
        assert_eq!(sig, "class Animal");
    }

    // -----------------------------------------------------------------------
    // Empty and broken source
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.swift");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_swift_broken_source_no_panic() {
        let source = "func good() {}\nfunc broken( {}\nstruct S {}";
        let symbols = parse_and_extract(source, "broken.swift");
        // Should extract at least something without panicking
        assert!(!symbols.is_empty());
    }

    // -----------------------------------------------------------------------
    // All symbols have body and signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_swift_all_symbols_have_body_and_signature() {
        let source = "public func greet() {}\nclass Animal {\n  func speak() {}\n}\nstruct Point {}\nprotocol Drawable {\n  func draw()\n}\nenum Direction {\n  case north\n}\nextension String {}";
        let symbols = parse_and_extract(source, "test.swift");
        for sym in &symbols {
            assert!(sym.body.is_some(), "symbol {} should have body", sym.name);
            assert!(
                sym.signature.is_some(),
                "symbol {} should have signature",
                sym.name
            );
            for child in &sym.children {
                assert!(
                    child.body.is_some(),
                    "child {} of {} should have body",
                    child.name,
                    sym.name
                );
                assert!(
                    child.signature.is_some(),
                    "child {} of {} should have signature",
                    child.name,
                    sym.name
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Fixture tests
    // -----------------------------------------------------------------------

    /// Helper: path to the fixture Swift project directory.
    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/swift_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Swift).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = SwiftExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    #[test]
    fn test_fixture_swift_greet_function() {
        let (_, symbols) = extract_fixture("main.swift");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_fixture_swift_animal_class_with_methods() {
        let (_, symbols) = extract_fixture("main.swift");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class)
            .expect("Animal not found");
        assert!(!animal.children.is_empty());
        let method_names: Vec<&str> = animal.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"speak"), "speak not found");
    }

    #[test]
    fn test_fixture_swift_point_struct() {
        let (_, symbols) = extract_fixture("main.swift");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_fixture_swift_drawable_protocol() {
        let (_, symbols) = extract_fixture("main.swift");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Interface);
        assert!(!drawable.children.is_empty());
    }

    #[test]
    fn test_fixture_swift_direction_enum() {
        let (_, symbols) = extract_fixture("main.swift");
        let dir = symbols
            .iter()
            .find(|s| s.name == "Direction")
            .expect("Direction not found");
        assert_eq!(dir.kind, SymbolKind::Enum);
    }

    #[test]
    fn test_fixture_swift_string_extension() {
        let (_, symbols) = extract_fixture("main.swift");
        let ext = symbols
            .iter()
            .find(|s| s.name == "String" && s.kind == SymbolKind::Module)
            .expect("String extension not found");
        assert!(!ext.children.is_empty());
    }

    #[test]
    fn test_fixture_swift_private_helper() {
        let (_, symbols) = extract_fixture("main.swift");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.visibility, Visibility::Private);
    }

    #[test]
    fn test_fixture_swift_all_symbols_have_body_and_signature() {
        let (_, symbols) = extract_fixture("main.swift");
        for sym in &symbols {
            assert!(sym.body.is_some(), "symbol {} should have body", sym.name);
            assert!(
                sym.signature.is_some(),
                "symbol {} should have signature",
                sym.name
            );
            for child in &sym.children {
                assert!(
                    child.body.is_some(),
                    "child {} of {} should have body",
                    child.name,
                    sym.name
                );
                assert!(
                    child.signature.is_some(),
                    "child {} of {} should have signature",
                    child.name,
                    sym.name
                );
            }
        }
    }
}
