//! Kotlin-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, classes,
//! objects, interfaces, data classes, enums, and methods within them.
//! Also provides body and signature extraction for each symbol kind.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Kotlin language extractor.
pub struct KotlinExtractor;

impl LanguageExtractor for KotlinExtractor {
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
/// - **Function/Method**: declaration up to the opening `{` or `=`, trimmed
/// - **Class/Interface/Enum/Struct**: header up to the opening `{`
/// - **Module (object)**: header up to the opening `{`
/// - **Const**: the full declaration
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class
        | SymbolKind::Interface
        | SymbolKind::Enum
        | SymbolKind::Struct
        | SymbolKind::Module => extract_header_signature(body_text),
        SymbolKind::Const => body_text.lines().next().unwrap_or("").trim().to_string(),
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{` or `=`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    // Try finding a top-level brace first
    if let Some(brace_pos) = find_top_level_brace(body) {
        return body[..brace_pos].trim().to_string();
    }
    // For expression-body functions (= ...), take everything up to the `=`
    if let Some(eq_pos) = find_top_level_eq(body) {
        return body[..eq_pos].trim().to_string();
    }
    // No brace or `=` found — abstract/interface method
    body.trim().trim_end_matches(';').trim().to_string()
}

/// Extract header signature for types: everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        // No body (e.g., `data class Point(val x: Double, val y: Double)`)
        body.trim().to_string()
    }
}

/// Find the position of the first top-level `{` in source text.
fn find_top_level_brace(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    let mut paren_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' if angle_depth == 0 && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the position of the first top-level `=` that is part of an expression body.
fn find_top_level_eq(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    let mut paren_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '=' if angle_depth == 0 && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract a top-level symbol from a node.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    match node.kind() {
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
        "object_declaration" => {
            let name = node_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let children = extract_class_body_members(node, source, file);
            let kind = SymbolKind::Module;
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

/// Extract a `class_declaration`, which covers class, interface, data class, and enum class.
fn extract_class_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let name = node_identifier(node, source)?;
    let visibility = extract_visibility(node, source);

    // Determine kind based on modifiers and keyword
    let mut is_data = false;
    let mut is_enum = false;
    let mut is_interface = false;

    let mut cursor = node.walk();
    let children_vec: Vec<_> = node.children(&mut cursor).collect();
    for child in &children_vec {
        match child.kind() {
            "modifiers" => {
                let mut mod_cursor = child.walk();
                let mods: Vec<_> = child.children(&mut mod_cursor).collect();
                for m in mods {
                    if m.kind() == "class_modifier" {
                        let text = m.utf8_text(source.as_bytes()).unwrap_or("");
                        if text.contains("data") {
                            is_data = true;
                        }
                        if text.contains("enum") {
                            is_enum = true;
                        }
                    }
                }
            }
            "interface" => is_interface = true,
            _ => {}
        }
    }

    let (kind, children) = if is_enum {
        (SymbolKind::Enum, vec![])
    } else if is_interface {
        let members = extract_class_body_members(node, source, file);
        (SymbolKind::Interface, members)
    } else if is_data {
        (SymbolKind::Struct, vec![])
    } else {
        let members = extract_class_body_members(node, source, file);
        (SymbolKind::Class, members)
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

/// Extract method members from a class/interface/object body.
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
                match body_child.kind() {
                    "function_declaration" => {
                        if let Some(method) = extract_method(body_child, source, file) {
                            members.push(method);
                        }
                    }
                    "property_declaration" => {
                        if let Some(prop) = extract_property(body_child, source, file) {
                            members.push(prop);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    members
}

/// Extract a method from a `function_declaration`.
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

/// Extract a val/var property declaration as a Const symbol.
fn extract_property(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    // Find the variable_declaration child which holds the name
    let mut name = None;
    for child in &children {
        if child.kind() == "variable_declaration" {
            name = child.utf8_text(source.as_bytes()).ok().map(String::from);
            break;
        }
    }

    let name = name?;
    let visibility = extract_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = body.lines().next().unwrap_or("").trim().to_string();

    Some(Symbol {
        name,
        kind: SymbolKind::Const,
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

/// Extract the `identifier` (name) from a Kotlin declaration node.
fn node_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    // Kotlin uses "identifier" children for names (not simple_identifier like Swift)
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "identifier" || child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Extract visibility from a Kotlin node by examining `modifiers/visibility_modifier`.
///
/// Kotlin visibility mapping:
/// - `public` (default) -> `Public`
/// - `private` -> `Private`
/// - `protected` -> `Crate`
/// - `internal` -> `Crate`
/// - no modifier -> `Public` (Kotlin default is `public`)
fn extract_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "modifiers" {
            return parse_modifiers_visibility(child, source);
        }
    }
    // No modifiers = public (Kotlin default)
    Visibility::Public
}

/// Parse visibility from a `modifiers` node.
fn parse_modifiers_visibility(modifiers: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = modifiers.walk();
    let children: Vec<_> = modifiers.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            if text.contains("private") {
                return Visibility::Private;
            }
            if text.contains("protected") || text.contains("internal") {
                return Visibility::Crate;
            }
            if text.contains("public") {
                return Visibility::Public;
            }
        }
    }
    // Modifiers present but no visibility modifier = public (Kotlin default)
    Visibility::Public
}

/// Extract doc comments (lines starting with `///` or `/**`) preceding a node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        if sib.kind() == "multiline_comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim();
                if trimmed.starts_with("/**") {
                    return Some(trimmed.to_string());
                }
            }
            break;
        }
        if sib.kind() == "line_comment" {
            sibling = sib.prev_sibling();
            continue;
        }
        break;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn grammar_available() -> bool {
        Parser::for_language(Language::Kotlin).is_ok()
    }

    /// Helper: parse source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let Ok(mut parser) = Parser::for_language(Language::Kotlin) else {
            eprintln!("skipping: Kotlin grammar not installed");
            return Vec::new();
        };
        let tree = parser.parse(source.as_bytes()).unwrap();
        KotlinExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    // -----------------------------------------------------------------------
    // Functions
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_function_default_public() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "fun greet(name: String): String = \"Hello\"";
        let symbols = parse_and_extract(source, "test.kt");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_kotlin_private_function() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "private fun helper(): Unit {}";
        let symbols = parse_and_extract(source, "test.kt");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_kotlin_internal_function() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "internal fun internalHelper(): Unit {}";
        let symbols = parse_and_extract(source, "test.kt");
        let f = symbols
            .iter()
            .find(|s| s.name == "internalHelper")
            .expect("internalHelper not found");
        assert_eq!(f.visibility, Visibility::Crate);
    }

    // -----------------------------------------------------------------------
    // Classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_class_with_methods() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "class Animal(val name: String) {\n  fun speak(): String = name\n}";
        let symbols = parse_and_extract(source, "test.kt");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.visibility, Visibility::Public);
        assert_eq!(animal.children.len(), 1);
        assert_eq!(animal.children[0].name, "speak");
        assert_eq!(animal.children[0].kind, SymbolKind::Method);
    }

    // -----------------------------------------------------------------------
    // Objects
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_object_with_members() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source =
            "object Singleton {\n  val instance = \"singleton\"\n  fun greet(): String = \"hi\"\n}";
        let symbols = parse_and_extract(source, "test.kt");
        let obj = symbols
            .iter()
            .find(|s| s.name == "Singleton")
            .expect("Singleton not found");
        assert_eq!(obj.kind, SymbolKind::Module);
        assert!(obj.children.len() >= 2);
    }

    // -----------------------------------------------------------------------
    // Interfaces
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_interface() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "interface Drawable {\n  fun draw()\n}";
        let symbols = parse_and_extract(source, "test.kt");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Interface);
        assert_eq!(drawable.children.len(), 1);
        assert_eq!(drawable.children[0].name, "draw");
    }

    // -----------------------------------------------------------------------
    // Data classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_data_class() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "data class Point(val x: Double, val y: Double)";
        let symbols = parse_and_extract(source, "test.kt");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    // -----------------------------------------------------------------------
    // Enum classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_enum_class() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "enum class Direction {\n  NORTH, SOUTH\n}";
        let symbols = parse_and_extract(source, "test.kt");
        let dir = symbols
            .iter()
            .find(|s| s.name == "Direction")
            .expect("Direction not found");
        assert_eq!(dir.kind, SymbolKind::Enum);
    }

    // -----------------------------------------------------------------------
    // Body and Signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_function_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "fun greet(name: String): String {\n  return name\n}";
        let symbols = parse_and_extract(source, "test.kt");
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
        assert!(sig.contains("fun greet(name: String): String"));
        assert!(!sig.contains('{'));
    }

    #[test]
    fn test_extract_kotlin_expression_body_signature() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "fun greet(name: String): String = \"Hello\"";
        let symbols = parse_and_extract(source, "test.kt");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(sig.contains("fun greet(name: String): String"));
        assert!(!sig.contains('='));
    }

    #[test]
    fn test_extract_kotlin_class_signature() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "class Animal(val name: String) {\n  fun speak(): String = name\n}";
        let symbols = parse_and_extract(source, "test.kt");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("Animal not found");
        let sig = animal.signature.as_ref().expect("sig should be present");
        assert!(sig.contains("class Animal(val name: String)"));
        assert!(!sig.contains('{'));
    }

    // -----------------------------------------------------------------------
    // Empty and broken source
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_empty_source_returns_empty() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let symbols = parse_and_extract("", "empty.kt");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_kotlin_broken_source_no_panic() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "fun good() {}\nfun broken( {}\nclass S {}";
        let symbols = parse_and_extract(source, "broken.kt");
        assert!(!symbols.is_empty());
    }

    // -----------------------------------------------------------------------
    // All symbols have body and signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_kotlin_all_symbols_have_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let source = "fun greet() {}\nclass Animal {\n  fun speak() {}\n}\nobject Singleton {}\ninterface Drawable {\n  fun draw()\n}\ndata class Point(val x: Double)\nenum class Direction {\n  NORTH\n}";
        let symbols = parse_and_extract(source, "test.kt");
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

    /// Helper: path to the fixture Kotlin project directory.
    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/kotlin_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let Ok(mut parser) = Parser::for_language(Language::Kotlin) else {
            eprintln!("skipping: Kotlin grammar not installed");
            return (String::new(), Vec::new());
        };
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = KotlinExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    #[test]
    fn test_fixture_kotlin_greet_function() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet" && s.kind == SymbolKind::Function)
            .expect("greet not found");
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_fixture_kotlin_animal_class_with_methods() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class)
            .expect("Animal not found");
        assert!(!animal.children.is_empty());
        let method_names: Vec<&str> = animal.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"speak"), "speak not found");
    }

    #[test]
    fn test_fixture_kotlin_config_object() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let config = symbols
            .iter()
            .find(|s| s.name == "Config")
            .expect("Config not found");
        assert_eq!(config.kind, SymbolKind::Module);
        assert!(!config.children.is_empty());
    }

    #[test]
    fn test_fixture_kotlin_drawable_interface() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Interface);
        assert!(!drawable.children.is_empty());
    }

    #[test]
    fn test_fixture_kotlin_point_data_class() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_fixture_kotlin_direction_enum() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let dir = symbols
            .iter()
            .find(|s| s.name == "Direction")
            .expect("Direction not found");
        assert_eq!(dir.kind, SymbolKind::Enum);
    }

    #[test]
    fn test_fixture_kotlin_private_helper() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.visibility, Visibility::Private);
    }

    #[test]
    fn test_fixture_kotlin_internal_helper() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
        let f = symbols
            .iter()
            .find(|s| s.name == "internalHelper")
            .expect("internalHelper not found");
        assert_eq!(f.visibility, Visibility::Crate);
    }

    #[test]
    fn test_fixture_kotlin_all_symbols_have_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Kotlin grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.kt");
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
