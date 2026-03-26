//! Java-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — classes, interfaces,
//! enums, methods, constructors, fields, annotation types, and packages.
//! Also provides body and signature extraction for each symbol kind.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Java language extractor.
pub struct JavaExtractor;

impl LanguageExtractor for JavaExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            match child.kind() {
                "package_declaration" => {
                    if let Some(sym) = extract_package(child, source, file) {
                        symbols.push(sym);
                    }
                }
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration" => {
                    if let Some(sym) = extract_type_declaration(child, source, file) {
                        symbols.push(sym);
                    }
                }
                _ => {}
            }
        }

        symbols
    }
}

/// Extract the full source body of a symbol's AST node.
///
/// Returns the complete source text between the node's start and end bytes.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a symbol.
///
/// The signature varies by symbol kind:
/// - **Method**: declaration line up to the opening `{`, trimmed
/// - **Class**: header up to the opening `{`
/// - **Interface**: header up to the opening `{`
/// - **Enum**: header up to the opening `{`
/// - **Const/Static**: the full declaration line
/// - **Module**: the package declaration
/// - **Type**: the annotation type header
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Method => extract_method_signature(body_text),
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum | SymbolKind::Type => {
            extract_header_signature(body_text)
        }
        SymbolKind::Const | SymbolKind::Static => extract_field_signature(body_text),
        SymbolKind::Module => body_text.trim_end_matches(';').trim().to_string(),
        _ => body_text.to_string(),
    }
}

/// Extract method/constructor signature: everything before the opening `{`, trimmed.
/// For abstract/interface methods (no body), strip the trailing semicolon.
fn extract_method_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        // No brace — abstract or interface method
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract header signature for classes, interfaces, enums, annotation types:
/// everything before the opening `{`.
fn extract_header_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract field declaration signature: the first line, trimmed.
fn extract_field_signature(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Find the position of the first top-level `{` in source text.
///
/// Skips braces inside generics (`<...>`) to avoid matching on type parameters.
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

/// Extract a package declaration as a Module symbol.
fn extract_package(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let package_name = extract_package_name(node, source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Module);
    Some(Symbol {
        name: package_name,
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

/// Extract the package name from a `package_declaration` node.
///
/// Handles both `scoped_identifier` (e.g. `com.example`) and plain `identifier`.
fn extract_package_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Extract a type declaration (class, interface, enum, annotation type).
fn extract_type_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    let kind_str = node.kind();
    let name = node_name(node, source)?;
    let visibility = extract_visibility(node, source);

    let (kind, children) = match kind_str {
        "class_declaration" => {
            let methods = extract_class_members(node, source, file);
            (SymbolKind::Class, methods)
        }
        "interface_declaration" => {
            let methods = extract_interface_members(node, source, file);
            (SymbolKind::Interface, methods)
        }
        "enum_declaration" => (SymbolKind::Enum, vec![]),
        "annotation_type_declaration" => (SymbolKind::Type, vec![]),
        _ => return None,
    };

    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    let doc = extract_javadoc(node, source);

    Some(Symbol {
        name,
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children,
        doc,
        body: Some(body),
        signature: Some(signature),
    })
}

/// Get the name from a declaration node (identifier child).
fn node_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    // Java tree-sitter uses "name" field for class/interface/enum/method/constructor
    if let Some(name_node) = node.child_by_field_name("name") {
        return name_node
            .utf8_text(source.as_bytes())
            .ok()
            .map(String::from);
    }
    None
}

/// Extract visibility from a Java node by examining `modifiers` children.
///
/// Java visibility mapping:
/// - `public` -> `Public`
/// - `private` -> `Private`
/// - `protected` -> `Crate` (closest equivalent)
/// - no modifier (package-private) -> `Crate`
fn extract_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            return parse_modifiers_visibility(child, source);
        }
    }
    // No modifiers = package-private
    Visibility::Crate
}

/// Parse visibility from a `modifiers` node.
fn parse_modifiers_visibility(modifiers: tree_sitter::Node<'_>, _source: &str) -> Visibility {
    let mut cursor = modifiers.walk();
    for child in modifiers.children(&mut cursor) {
        match child.kind() {
            "public" => return Visibility::Public,
            "private" => return Visibility::Private,
            "protected" => return Visibility::Crate,
            _ => {}
        }
    }
    // No visibility modifier in modifiers = package-private
    Visibility::Crate
}

/// Check if a `field_declaration` has both `static` and `final` modifiers.
fn has_static_and_final(modifiers: tree_sitter::Node<'_>) -> (bool, bool) {
    let mut is_static = false;
    let mut is_final = false;
    let mut cursor = modifiers.walk();
    for child in modifiers.children(&mut cursor) {
        match child.kind() {
            "static" => is_static = true,
            "final" => is_final = true,
            _ => {}
        }
    }
    (is_static, is_final)
}

/// Extract members from a class body: methods, constructors, and static/const fields.
fn extract_class_members(
    class_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return members;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        match child.kind() {
            "method_declaration" | "constructor_declaration" => {
                if let Some(method) = extract_method(child, source, file) {
                    members.push(method);
                }
            }
            "field_declaration" => {
                if let Some(field) = extract_field(child, source, file) {
                    members.push(field);
                }
            }
            _ => {}
        }
    }

    members
}

/// Extract members from an interface body: method declarations.
fn extract_interface_members(
    iface_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut members = Vec::new();
    let Some(body) = iface_node.child_by_field_name("body") else {
        return members;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        if child.kind() == "method_declaration" {
            if let Some(method) = extract_method(child, source, file) {
                members.push(method);
            }
        }
    }

    members
}

/// Extract a method or constructor as a Method symbol.
fn extract_method(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = node_name(node, source)?;
    let visibility = extract_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Method);
    let doc = extract_javadoc(node, source);

    Some(Symbol {
        name,
        kind: SymbolKind::Method,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children: vec![],
        doc,
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a field declaration as Const (static final) or Static (static only).
///
/// Non-static fields are not extracted as top-level symbols.
fn extract_field(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    // Only extract static fields (static or static final)
    let mut cursor = node.walk();
    let mut modifiers_node = None;
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            modifiers_node = Some(child);
            break;
        }
    }

    let modifiers = modifiers_node?;
    let (is_static, is_final) = has_static_and_final(modifiers);

    if !is_static {
        return None;
    }

    let kind = if is_final {
        SymbolKind::Const
    } else {
        SymbolKind::Static
    };

    // Get the field name from the variable_declarator
    let name = extract_field_name(node, source)?;
    let visibility = extract_visibility(node, source);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    let doc = extract_javadoc(node, source);

    Some(Symbol {
        name,
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility,
        children: vec![],
        doc,
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract the field name from a `field_declaration` node.
///
/// The name lives in `variable_declarator` > `identifier`.
fn extract_field_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            // The name is the "name" field of variable_declarator
            if let Some(name_node) = child.child_by_field_name("name") {
                return name_node
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(String::from);
            }
        }
    }
    None
}

/// Extract Javadoc comment preceding a definition node.
///
/// Looks for a `block_comment` sibling starting with `/**` immediately before the node.
fn extract_javadoc(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        if sib.kind() == "block_comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim();
                if trimmed.starts_with("/**") {
                    return Some(trimmed.to_string());
                }
            }
            break;
        }
        if sib.kind() == "line_comment" {
            // Skip line comments between javadoc and declaration
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
    use std::path::PathBuf;

    /// Helper: parse source and extract symbols for the given file path.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        JavaExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture Java project source directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/java_project/src/main/java/com/example")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = JavaExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract class with methods → Class with Method children
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_class_with_methods_returns_class_with_method_children() {
        let source = r#"public class User {
    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }
}"#;
        let symbols = parse_and_extract(source, "User.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User class not found");
        assert_eq!(class.kind, SymbolKind::Class);
        assert_eq!(class.visibility, Visibility::Public);
        assert_eq!(class.children.len(), 2);
        assert!(class.children.iter().all(|c| c.kind == SymbolKind::Method));

        let get_name = class
            .children
            .iter()
            .find(|c| c.name == "getName")
            .expect("getName not found");
        assert_eq!(get_name.kind, SymbolKind::Method);
        assert_eq!(get_name.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract interface → Interface
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_interface_returns_interface_with_method_children() {
        let source = r#"public interface UserService {
    User findById(int id);
    void save(User user);
}"#;
        let symbols = parse_and_extract(source, "UserService.java");
        let iface = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("UserService not found");
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.visibility, Visibility::Public);
        assert_eq!(iface.children.len(), 2);

        let find_by_id = iface
            .children
            .iter()
            .find(|c| c.name == "findById")
            .expect("findById not found");
        assert_eq!(find_by_id.kind, SymbolKind::Method);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract enum → Enum
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_enum_returns_enum() {
        let source = r#"public enum Role {
    ADMIN,
    USER,
    GUEST
}"#;
        let symbols = parse_and_extract(source, "Role.java");
        let role = symbols
            .iter()
            .find(|s| s.name == "Role")
            .expect("Role not found");
        assert_eq!(role.kind, SymbolKind::Enum);
        assert_eq!(role.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract constructor → Method
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_constructor_returns_method_with_class_name() {
        let source = r#"public class User {
    public User(String name, int age) {
        this.name = name;
        this.age = age;
    }
}"#;
        let symbols = parse_and_extract(source, "User.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User class not found");
        let constructor = class
            .children
            .iter()
            .find(|c| c.name == "User" && c.kind == SymbolKind::Method)
            .expect("Constructor not found");
        assert_eq!(constructor.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Visibility: public/private/protected/package-private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_visibility_public_private_protected_package_private() {
        let source = r#"public class Vis {
    public void pubMethod() {}
    private void privMethod() {}
    protected void protMethod() {}
    void pkgMethod() {}
}"#;
        let symbols = parse_and_extract(source, "Vis.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Vis")
            .expect("Vis class not found");

        let pub_m = class
            .children
            .iter()
            .find(|c| c.name == "pubMethod")
            .expect("pubMethod not found");
        assert_eq!(pub_m.visibility, Visibility::Public);

        let priv_m = class
            .children
            .iter()
            .find(|c| c.name == "privMethod")
            .expect("privMethod not found");
        assert_eq!(priv_m.visibility, Visibility::Private);

        let prot_m = class
            .children
            .iter()
            .find(|c| c.name == "protMethod")
            .expect("protMethod not found");
        assert_eq!(prot_m.visibility, Visibility::Crate);

        let pkg_m = class
            .children
            .iter()
            .find(|c| c.name == "pkgMethod")
            .expect("pkgMethod not found");
        assert_eq!(pkg_m.visibility, Visibility::Crate);
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Extract static final field → Const
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_static_final_field_returns_const() {
        let source = r#"public class Config {
    public static final int MAX_AGE = 200;
    public static int counter = 0;
}"#;
        let symbols = parse_and_extract(source, "Config.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Config")
            .expect("Config not found");

        let max_age = class
            .children
            .iter()
            .find(|c| c.name == "MAX_AGE")
            .expect("MAX_AGE not found");
        assert_eq!(max_age.kind, SymbolKind::Const);
        assert_eq!(max_age.visibility, Visibility::Public);

        let counter = class
            .children
            .iter()
            .find(|c| c.name == "counter")
            .expect("counter not found");
        assert_eq!(counter.kind, SymbolKind::Static);
        assert_eq!(counter.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Body and signature extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_body_and_signature_for_method() {
        let source = r#"public class Foo {
    public String getName() {
        return name;
    }
}"#;
        let symbols = parse_and_extract(source, "Foo.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");

        let method = class
            .children
            .iter()
            .find(|c| c.name == "getName")
            .expect("getName not found");

        // Body includes the full method text
        let body = method.body.as_ref().expect("body should be present");
        assert!(body.contains("return name;"));
        assert!(body.contains("public String getName()"));

        // Signature is the declaration without the body
        let sig = method
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(sig.contains("public String getName()"));
        assert!(!sig.contains("return name;"));
    }

    #[test]
    fn test_extract_body_and_signature_for_class() {
        let source = r#"public class Bar extends Foo implements Baz {
    public void doStuff() {}
}"#;
        let symbols = parse_and_extract(source, "Bar.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Bar")
            .expect("Bar not found");

        let sig = class
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(sig.contains("public class Bar extends Foo implements Baz"));
        assert!(!sig.contains('{'));
    }

    // -----------------------------------------------------------------------
    // Scenario 8: extract_symbols dispatches correctly for Java
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_symbols_dispatches_correctly_for_java() {
        let source = r#"package com.example;

public class Main {
    public static void main(String[] args) {}
}"#;
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();

        // Use the extract dispatch
        let symbols = crate::extract_symbols(source, &tree, Path::new("Main.java"), Language::Java);
        assert!(!symbols.is_empty());

        let pkg = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("package not found");
        assert_eq!(pkg.name, "com.example");

        let class = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .expect("class not found");
        assert_eq!(class.name, "Main");
    }

    // -----------------------------------------------------------------------
    // Additional: package declaration → Module
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_package_declaration_returns_module() {
        let source = "package com.example.models;\n\npublic class User {}";
        let symbols = parse_and_extract(source, "User.java");
        let pkg = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("package not found");
        assert_eq!(pkg.name, "com.example.models");
        assert_eq!(pkg.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Additional: annotation type → Type
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_annotation_type_returns_type() {
        let source = r#"public @interface MyAnnotation {
    String value();
}"#;
        let symbols = parse_and_extract(source, "MyAnnotation.java");
        let ann = symbols
            .iter()
            .find(|s| s.name == "MyAnnotation")
            .expect("MyAnnotation not found");
        assert_eq!(ann.kind, SymbolKind::Type);
        assert_eq!(ann.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Fixture: User.java — class with fields, methods, constructor
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_user_class_has_expected_members() {
        let (_, symbols) = extract_fixture("models/User.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Class)
            .expect("User class not found");
        assert_eq!(class.visibility, Visibility::Public);

        // Should have constructor + methods + static final field
        let method_names: Vec<&str> = class.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"User"), "constructor not found");
        assert!(method_names.contains(&"getName"), "getName not found");
        assert!(method_names.contains(&"getAge"), "getAge not found");
        assert!(method_names.contains(&"MAX_AGE"), "MAX_AGE not found");
    }

    // -----------------------------------------------------------------------
    // Fixture: UserService.java — interface + implementation
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_user_service_interface_and_impl() {
        let (_, symbols) = extract_fixture("services/UserService.java");
        let iface = symbols
            .iter()
            .find(|s| s.name == "UserService" && s.kind == SymbolKind::Interface)
            .expect("UserService interface not found");
        assert_eq!(iface.visibility, Visibility::Public);
        assert!(!iface.children.is_empty());
    }

    // -----------------------------------------------------------------------
    // Javadoc extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_javadoc_comment_on_class() {
        let source = r#"/** This is a user class. */
public class User {}"#;
        let symbols = parse_and_extract(source, "User.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let doc = class.doc.as_ref().expect("doc should be present");
        assert!(doc.contains("This is a user class."));
    }

    // -----------------------------------------------------------------------
    // Non-static fields should NOT be extracted
    // -----------------------------------------------------------------------
    #[test]
    fn test_non_static_fields_not_extracted() {
        let source = r#"public class Foo {
    private String name;
    private int age;
}"#;
        let symbols = parse_and_extract(source, "Foo.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        assert!(
            class.children.is_empty(),
            "non-static fields should not be extracted"
        );
    }

    // -----------------------------------------------------------------------
    // Interface method signature (no body)
    // -----------------------------------------------------------------------
    #[test]
    fn test_interface_method_signature_strips_semicolon() {
        let source = r#"public interface Svc {
    User findById(int id);
}"#;
        let symbols = parse_and_extract(source, "Svc.java");
        let iface = symbols
            .iter()
            .find(|s| s.name == "Svc")
            .expect("Svc not found");
        let method = iface
            .children
            .iter()
            .find(|c| c.name == "findById")
            .expect("findById not found");
        let sig = method
            .signature
            .as_ref()
            .expect("signature should be present");
        assert!(!sig.ends_with(';'));
        assert!(sig.contains("User findById(int id)"));
    }
}
