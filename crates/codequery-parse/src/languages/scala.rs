//! Scala-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — classes, traits,
//! objects, defs, vals, and case classes. Also provides body and signature
//! extraction for each symbol kind.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Scala language extractor.
pub struct ScalaExtractor;

impl LanguageExtractor for ScalaExtractor {
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
/// - **Function/Method**: declaration up to `=` or full text if abstract
/// - **Class/Trait**: header up to the opening `{`
/// - **Module (object)**: header up to the opening `{`
/// - **Const (val)**: the full declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Trait | SymbolKind::Module => {
            extract_header_signature(body_text)
        }
        SymbolKind::Const => body_text.lines().next().unwrap_or("").trim().to_string(),
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before `=` or `{`.
fn extract_fn_signature(body: &str) -> String {
    // For concrete defs with =, take everything before the =
    if let Some(eq_pos) = find_top_level_eq(body) {
        return body[..eq_pos].trim().to_string();
    }
    // For defs with braces
    if let Some(brace_pos) = find_top_level_brace(body) {
        return body[..brace_pos].trim().to_string();
    }
    // Abstract def (no body)
    body.trim().to_string()
}

/// Extract header signature for types: everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.trim().to_string()
    }
}

/// Find the position of the first top-level `{` in source text.
fn find_top_level_brace(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    let mut paren_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '[' => angle_depth = angle_depth.saturating_add(1),
            ']' => angle_depth = angle_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' if angle_depth == 0 && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the position of the first top-level `=` in source text.
fn find_top_level_eq(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    let mut paren_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '[' => angle_depth = angle_depth.saturating_add(1),
            ']' => angle_depth = angle_depth.saturating_sub(1),
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
        "class_definition" => extract_class_definition(node, source, file),
        "trait_definition" => {
            let name = node_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let children = extract_template_body_members(node, source, file);
            let kind = SymbolKind::Trait;
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
        "object_definition" => {
            let name = node_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let children = extract_template_body_members(node, source, file);
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
        "function_definition" => {
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
        "val_definition" => {
            let name = node_identifier(node, source)?;
            let visibility = extract_visibility(node, source);
            let kind = SymbolKind::Const;
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
        _ => None,
    }
}

/// Extract a `class_definition`, including case classes.
fn extract_class_definition(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let name = node_identifier(node, source)?;
    let visibility = extract_visibility(node, source);

    // Check for `case` keyword
    let mut is_case = false;
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in &children {
        if child.kind() == "case" {
            is_case = true;
            break;
        }
    }

    let (kind, sym_children) = if is_case {
        // Case class maps to Struct
        (SymbolKind::Struct, vec![])
    } else {
        let members = extract_template_body_members(node, source, file);
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
        children: sym_children,
        doc: extract_doc_comment(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract members (defs, vals) from a `template_body`.
fn extract_template_body_members(
    decl_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let mut cursor = decl_node.walk();
    let children: Vec<_> = decl_node.children(&mut cursor).collect();

    for child in children {
        if child.kind() == "template_body" {
            let mut body_cursor = child.walk();
            let body_children: Vec<_> = child.children(&mut body_cursor).collect();
            for body_child in body_children {
                if body_child.is_error() || body_child.is_missing() {
                    continue;
                }
                match body_child.kind() {
                    "function_definition" => {
                        if let Some(method) = extract_member_def(body_child, source, file) {
                            members.push(method);
                        }
                    }
                    "function_declaration" => {
                        // Abstract def (no body)
                        if let Some(method) = extract_member_def(body_child, source, file) {
                            members.push(method);
                        }
                    }
                    "val_definition" => {
                        if let Some(val) = extract_member_val(body_child, source, file) {
                            members.push(val);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    members
}

/// Extract a def as a Method symbol inside a class/trait/object body.
fn extract_member_def(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
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

/// Extract a val as a Const symbol inside a class/trait/object body.
fn extract_member_val(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = node_identifier(node, source)?;
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

/// Extract the `identifier` from a Scala declaration node.
fn node_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Extract visibility from a Scala node by examining `modifiers/access_modifier`.
///
/// Scala visibility mapping:
/// - default (no modifier) -> `Public`
/// - `private` -> `Private`
/// - `protected` -> `Crate`
fn extract_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "modifiers" {
            return parse_modifiers_visibility(child, source);
        }
    }
    // No modifiers = public (Scala default)
    Visibility::Public
}

/// Parse visibility from a `modifiers` node.
fn parse_modifiers_visibility(modifiers: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = modifiers.walk();
    let children: Vec<_> = modifiers.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "access_modifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            if text.contains("private") {
                return Visibility::Private;
            }
            if text.contains("protected") {
                return Visibility::Crate;
            }
        }
    }
    // Modifiers present but no access modifier = public
    Visibility::Public
}

/// Extract doc comments (Scaladoc `/** ... */`) preceding a node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let sib = node.prev_sibling()?;
    if sib.kind() == "comment" {
        if let Ok(text) = sib.utf8_text(source.as_bytes()) {
            let trimmed = text.trim();
            if trimmed.starts_with("/**") {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;

    fn grammar_available() -> bool {
        Parser::for_language(Language::Scala).is_ok()
    }

    /// Helper: parse source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let Ok(mut parser) = Parser::for_language(Language::Scala) else {
            eprintln!("skipping: Scala grammar not installed");
            return Vec::new();
        };
        let tree = parser.parse(source.as_bytes()).unwrap();
        ScalaExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    // -----------------------------------------------------------------------
    // Classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_class_with_methods() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "class Animal(val name: String) {\n  def speak(): String = name\n}";
        let symbols = parse_and_extract(source, "test.scala");
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
    // Traits
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_trait_with_methods() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "trait Drawable {\n  def draw(): Unit\n}";
        let symbols = parse_and_extract(source, "test.scala");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Trait);
        assert_eq!(drawable.children.len(), 1);
        assert_eq!(drawable.children[0].name, "draw");
    }

    // -----------------------------------------------------------------------
    // Objects
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_object_with_members() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "object Singleton {\n  val instance = \"singleton\"\n  def greet(name: String): String = name\n}";
        let symbols = parse_and_extract(source, "test.scala");
        let obj = symbols
            .iter()
            .find(|s| s.name == "Singleton")
            .expect("Singleton not found");
        assert_eq!(obj.kind, SymbolKind::Module);
        assert_eq!(obj.children.len(), 2);

        let val_sym = obj
            .children
            .iter()
            .find(|c| c.name == "instance")
            .expect("instance not found");
        assert_eq!(val_sym.kind, SymbolKind::Const);

        let def_sym = obj
            .children
            .iter()
            .find(|c| c.name == "greet")
            .expect("greet not found");
        assert_eq!(def_sym.kind, SymbolKind::Method);
    }

    // -----------------------------------------------------------------------
    // Case classes
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_case_class() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "case class Point(x: Double, y: Double)";
        let symbols = parse_and_extract(source, "test.scala");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    // -----------------------------------------------------------------------
    // Visibility
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_private_class() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "private class Secret {}";
        let symbols = parse_and_extract(source, "test.scala");
        let secret = symbols
            .iter()
            .find(|s| s.name == "Secret")
            .expect("Secret not found");
        assert_eq!(secret.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_scala_protected_trait() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "protected trait Guarded {}";
        let symbols = parse_and_extract(source, "test.scala");
        let guarded = symbols
            .iter()
            .find(|s| s.name == "Guarded")
            .expect("Guarded not found");
        assert_eq!(guarded.visibility, Visibility::Crate);
    }

    #[test]
    fn test_extract_scala_default_public() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "class Open {}";
        let symbols = parse_and_extract(source, "test.scala");
        let open = symbols
            .iter()
            .find(|s| s.name == "Open")
            .expect("Open not found");
        assert_eq!(open.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Body and Signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_def_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "object Main {\n  def greet(name: String): String = name\n}";
        let symbols = parse_and_extract(source, "test.scala");
        let main = symbols
            .iter()
            .find(|s| s.name == "Main")
            .expect("Main not found");
        let greet = main
            .children
            .iter()
            .find(|c| c.name == "greet")
            .expect("greet not found");

        let body = greet.body.as_ref().expect("body should be present");
        assert!(body.contains("name"));

        let sig = greet
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(sig.contains("def greet(name: String): String"));
        assert!(!sig.contains('='));
    }

    #[test]
    fn test_extract_scala_class_signature() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "class Animal(val name: String) {\n  def speak(): String = name\n}";
        let symbols = parse_and_extract(source, "test.scala");
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
    fn test_extract_scala_empty_source_returns_empty() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let symbols = parse_and_extract("", "empty.scala");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_scala_broken_source_no_panic() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "class Good {}\nclass Broken( {}\ntrait T {}";
        let symbols = parse_and_extract(source, "broken.scala");
        // Should extract at least something without panicking
        assert!(!symbols.is_empty());
    }

    // -----------------------------------------------------------------------
    // All symbols have body and signature
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_scala_all_symbols_have_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let source = "class Animal {\n  def speak(): String = \"hi\"\n}\ntrait Drawable {\n  def draw(): Unit\n}\nobject Singleton {\n  val x = 1\n}\ncase class Point(x: Double)";
        let symbols = parse_and_extract(source, "test.scala");
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

    /// Helper: path to the fixture Scala project directory.
    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/scala_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let Ok(mut parser) = Parser::for_language(Language::Scala) else {
            eprintln!("skipping: Scala grammar not installed");
            return (String::new(), Vec::new());
        };
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = ScalaExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    #[test]
    fn test_fixture_scala_animal_class_with_methods() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let animal = symbols
            .iter()
            .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class)
            .expect("Animal not found");
        assert!(!animal.children.is_empty());
        let method_names: Vec<&str> = animal.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"speak"), "speak not found");
    }

    #[test]
    fn test_fixture_scala_drawable_trait() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let drawable = symbols
            .iter()
            .find(|s| s.name == "Drawable")
            .expect("Drawable not found");
        assert_eq!(drawable.kind, SymbolKind::Trait);
        assert!(!drawable.children.is_empty());
    }

    #[test]
    fn test_fixture_scala_config_object() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let config = symbols
            .iter()
            .find(|s| s.name == "Config")
            .expect("Config not found");
        assert_eq!(config.kind, SymbolKind::Module);
        assert!(!config.children.is_empty());
    }

    #[test]
    fn test_fixture_scala_point_case_class() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_fixture_scala_private_class() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let secret = symbols
            .iter()
            .find(|s| s.name == "Secret")
            .expect("Secret not found");
        assert_eq!(secret.visibility, Visibility::Private);
    }

    #[test]
    fn test_fixture_scala_protected_trait() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
        let guarded = symbols
            .iter()
            .find(|s| s.name == "Guarded")
            .expect("Guarded not found");
        assert_eq!(guarded.visibility, Visibility::Crate);
    }

    #[test]
    fn test_fixture_scala_all_symbols_have_body_and_signature() {
        if !grammar_available() {
            eprintln!("skipping: Scala grammar not installed");
            return;
        }
        let (_, symbols) = extract_fixture("Main.scala");
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
