//! TypeScript/JavaScript symbol extraction from tree-sitter ASTs.
//!
//! Handles both TypeScript (.ts/.tsx) and JavaScript (.js/.jsx) files with a
//! single extractor. The AST structures are nearly identical — TypeScript adds
//! type annotations, interfaces, type aliases, and enums on top of the
//! JavaScript base.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// TypeScript/JavaScript language extractor.
///
/// Implements [`LanguageExtractor`] for both TypeScript and JavaScript sources.
/// The tree-sitter grammars produce similar AST shapes; the main difference is
/// that TypeScript has `interface_declaration`, `type_alias_declaration`, and
/// `enum_declaration` nodes that JavaScript lacks.
pub struct TypeScriptExtractor;

impl LanguageExtractor for TypeScriptExtractor {
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

/// Extract the full source body of a symbol's AST node.
///
/// Returns the complete source text between the node's start and end bytes.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a TypeScript/JavaScript symbol.
///
/// The signature varies by symbol kind:
/// - **Function**: declaration line up to the opening `{`, trimmed
/// - **Class**: `class Name` header (or `class Name extends Base`)
/// - **Method**: declaration line up to the opening `{`, trimmed
/// - **Interface**: the full interface text
/// - **Type**: the full type alias line
/// - **Enum**: the full enum text
/// - **Const**: the full declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method => extract_fn_signature(body_text),
        SymbolKind::Class => extract_class_signature(body_text),
        SymbolKind::Type | SymbolKind::Const => extract_single_line_signature(body_text),
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        // No brace — possibly an arrow function expression or abstract method
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract class signature: `class Name` or `class Name extends Base`.
fn extract_class_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        let header = body[..brace_pos].trim();
        format!("{header} {{ ... }}")
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract a single-line signature for const and type alias declarations.
fn extract_single_line_signature(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Find the position of the first top-level `{` in source text.
///
/// Skips braces inside generics (`<...>`) and parentheses to avoid false matches.
fn find_top_level_brace(source: &str) -> Option<usize> {
    let mut angle_depth: u32 = 0;
    let mut paren_depth: u32 = 0;
    for (i, ch) in source.char_indices() {
        match ch {
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' if angle_depth > 0 => angle_depth = angle_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' if angle_depth == 0 && paren_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract a top-level symbol from a node.
///
/// Handles both direct declarations (e.g., `function foo()`) and
/// `export_statement` wrappers. Pushes extracted symbols into `symbols`.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "export_statement" => {
            // export wraps a declaration — extract the inner declaration as Public
            if let Some(decl) = node.child_by_field_name("declaration") {
                extract_declaration(decl, source, file, Visibility::Public, symbols);
            }
        }
        _ => {
            // Direct (non-exported) declarations are Private
            extract_declaration(node, source, file, Visibility::Private, symbols);
        }
    }
}

/// Extract a symbol from a declaration node with the given visibility.
fn extract_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(sym) = extract_function(node, source, file, visibility) {
                symbols.push(sym);
            }
        }
        "class_declaration" => {
            if let Some(sym) = extract_class(node, source, file, visibility) {
                symbols.push(sym);
            }
        }
        "interface_declaration" => {
            if let Some(sym) = extract_interface(node, source, file, visibility) {
                symbols.push(sym);
            }
        }
        "type_alias_declaration" => {
            if let Some(sym) = extract_type_alias(node, source, file, visibility) {
                symbols.push(sym);
            }
        }
        "enum_declaration" => {
            if let Some(sym) = extract_enum(node, source, file, visibility) {
                symbols.push(sym);
            }
        }
        "lexical_declaration" => {
            extract_lexical_declaration(node, source, file, visibility, symbols);
        }
        _ => {}
    }
}

/// Extract a `function_declaration` node as a Function symbol.
fn extract_function(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Function);
    let doc = extract_doc_comment(node, source);
    Some(Symbol {
        name,
        kind: SymbolKind::Function,
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

/// Extract a `class_declaration` node as a Class symbol with Method children.
fn extract_class(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let children = extract_class_methods(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Class);
    let doc = extract_doc_comment(node, source);
    Some(Symbol {
        name,
        kind: SymbolKind::Class,
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

/// Extract an `interface_declaration` node as an Interface symbol (TS only).
fn extract_interface(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Interface);
    let doc = extract_doc_comment(node, source);
    Some(Symbol {
        name,
        kind: SymbolKind::Interface,
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

/// Extract a `type_alias_declaration` node as a Type symbol (TS only).
fn extract_type_alias(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Type);
    let doc = extract_doc_comment(node, source);
    Some(Symbol {
        name,
        kind: SymbolKind::Type,
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

/// Extract an `enum_declaration` node as an Enum symbol (TS only).
fn extract_enum(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Enum);
    let doc = extract_doc_comment(node, source);
    Some(Symbol {
        name,
        kind: SymbolKind::Enum,
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

/// Extract symbols from a `lexical_declaration` node.
///
/// Only `const` declarations at module level are extracted. Each
/// `variable_declarator` child becomes a Const symbol. If the value is an
/// `arrow_function`, the symbol kind is Function instead of Const.
fn extract_lexical_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    visibility: Visibility,
    symbols: &mut Vec<Symbol>,
) {
    // Only extract `const` declarations, not `let`/`var`
    let mut cursor = node.walk();
    let is_const = node.children(&mut cursor).any(|c| c.kind() == "const");
    if !is_const {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(name_text) = name.utf8_text(source.as_bytes()) else {
            continue;
        };

        // Determine if the value is an arrow function
        let is_arrow = child
            .child_by_field_name("value")
            .is_some_and(|v| v.kind() == "arrow_function");

        let kind = if is_arrow {
            SymbolKind::Function
        } else {
            SymbolKind::Const
        };

        // Use the full lexical_declaration node for body/signature so the
        // output includes `const name = ...` rather than just the declarator.
        let body = extract_body(source, &node);
        let signature = extract_signature(source, &node, kind);
        let doc = extract_doc_comment(node, source);

        symbols.push(Symbol {
            name: name_text.to_string(),
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
        });
    }
}

/// Extract methods from a class body.
fn extract_class_methods(
    class_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut methods = Vec::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return methods;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        if child.kind() == "method_definition" {
            let Some(name) = node_field_text(child, "name", source) else {
                continue;
            };
            let method_body = extract_body(source, &child);
            let method_sig = extract_signature(source, &child, SymbolKind::Method);
            let method_vis = extract_method_visibility(child, source);
            let doc = extract_doc_comment(child, source);

            methods.push(Symbol {
                name,
                kind: SymbolKind::Method,
                file: file.to_path_buf(),
                line: child.start_position().row + 1,
                column: child.start_position().column,
                end_line: child.end_position().row + 1,
                visibility: method_vis,
                children: vec![],
                doc,
                body: Some(method_body),
                signature: Some(method_sig),
            });
        }
    }

    methods
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract method visibility from an `accessibility_modifier` child.
///
/// In TypeScript, methods can have `public`, `private`, or `protected`
/// modifiers. Without a modifier, methods default to public in JS/TS
/// convention, but we report them as Private (no explicit `export`).
fn extract_method_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                match text {
                    "public" => return Visibility::Public,
                    "private" | "protected" => return Visibility::Private,
                    _ => {}
                }
            }
        }
    }
    // No explicit modifier — treat as public (default in JS/TS)
    Visibility::Public
}

/// Extract doc comments preceding a definition node.
///
/// Looks for a `comment` sibling immediately before the node that
/// starts with `/**` (JSDoc-style) or `//`.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    // For nodes inside export_statement, we need to check the parent's prev sibling
    let check_node = if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            parent
        } else {
            node
        }
    } else {
        node
    };

    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = check_node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with("///")
                    || trimmed.starts_with("/**")
                    || trimmed.starts_with("//")
                {
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

    /// Helper: parse TypeScript source and extract symbols.
    fn parse_ts(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        TypeScriptExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: parse JavaScript source and extract symbols.
    fn parse_js(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::JavaScript).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        TypeScriptExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture TypeScript project source directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/typescript_project/src")
    }

    /// Helper: parse a fixture file (TypeScript) and extract symbols.
    fn extract_ts_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = TypeScriptExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    /// Helper: parse a fixture file (JavaScript) and extract symbols.
    fn extract_js_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::JavaScript).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = TypeScriptExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract exported function -> Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_exported_function_is_function_public() {
        let (_, symbols) = extract_ts_fixture("index.ts");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract class with methods -> Class with Method children
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_class_with_methods_has_method_children() {
        let (_, symbols) = extract_ts_fixture("services.ts");
        let service = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("UserService not found");
        assert_eq!(service.kind, SymbolKind::Class);
        assert_eq!(service.visibility, Visibility::Public);

        let method_names: Vec<&str> = service.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"constructor"));
        assert!(method_names.contains(&"addUser"));
        assert!(method_names.contains(&"getUser"));
        assert!(method_names.contains(&"validate"));

        for child in &service.children {
            assert_eq!(child.kind, SymbolKind::Method);
        }
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract interface -> Interface (TS)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_interface_is_interface_public() {
        let (_, symbols) = extract_ts_fixture("models.ts");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User interface not found");
        assert_eq!(user.kind, SymbolKind::Interface);
        assert_eq!(user.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract type alias -> Type (TS)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_type_alias_is_type_public() {
        let (_, symbols) = extract_ts_fixture("models.ts");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserId")
            .expect("UserId not found");
        assert_eq!(userid.kind, SymbolKind::Type);
        assert_eq!(userid.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Extract enum -> Enum (TS)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_enum_is_enum_public() {
        let (_, symbols) = extract_ts_fixture("models.ts");
        let role = symbols
            .iter()
            .find(|s| s.name == "Role")
            .expect("Role enum not found");
        assert_eq!(role.kind, SymbolKind::Enum);
        assert_eq!(role.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Extract arrow function const -> Function
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_arrow_function_const_is_function() {
        let (_, symbols) = extract_ts_fixture("index.ts");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Extract plain JS function -> Function
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_plain_js_function_is_function() {
        let (_, symbols) = extract_js_fixture("utils.js");
        let format = symbols
            .iter()
            .find(|s| s.name == "formatName")
            .expect("formatName not found");
        assert_eq!(format.kind, SymbolKind::Function);
        assert_eq!(format.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 8: Extract JS class -> Class
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_js_class_is_class() {
        let (_, symbols) = extract_js_fixture("utils.js");
        let logger = symbols
            .iter()
            .find(|s| s.name == "Logger")
            .expect("Logger not found");
        assert_eq!(logger.kind, SymbolKind::Class);
        assert_eq!(logger.visibility, Visibility::Private);

        let method_names: Vec<&str> = logger.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"constructor"));
        assert!(method_names.contains(&"log"));
    }

    // -----------------------------------------------------------------------
    // Scenario 9: Body extraction works
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_extraction_contains_source_text() {
        let source =
            "export function greet(name: string): string {\n    return `Hello, ${name}!`;\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("function greet"));
        assert!(body.contains("return `Hello, ${name}!`"));
        assert!(body.ends_with('}'));
    }

    // -----------------------------------------------------------------------
    // Scenario 10: Signature extraction works
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_extraction_for_function() {
        let source =
            "export function greet(name: string): string {\n    return `Hello, ${name}!`;\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "function greet(name: string): string");
    }

    #[test]
    fn test_signature_extraction_for_class() {
        let source = "class Foo {\n    bar(): void {}\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let sig = foo.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "class Foo { ... }");
    }

    #[test]
    fn test_signature_extraction_for_interface() {
        let source = "interface User {\n    name: string;\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("signature should be Some");
        assert!(sig.contains("interface User"));
        assert!(sig.contains("name: string"));
    }

    #[test]
    fn test_signature_extraction_for_const() {
        let source = "const MAX_RETRIES = 3;\n";
        let symbols = parse_ts(source, "test.ts");
        let max = symbols
            .iter()
            .find(|s| s.name == "MAX_RETRIES")
            .expect("MAX_RETRIES not found");
        let sig = max.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "const MAX_RETRIES = 3");
    }

    #[test]
    fn test_signature_extraction_for_type_alias() {
        let source = "type UserId = string;\n";
        let symbols = parse_ts(source, "test.ts");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserId")
            .expect("UserId not found");
        let sig = userid
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "type UserId = string");
    }

    #[test]
    fn test_signature_extraction_for_enum() {
        let source = "enum Role {\n    Admin,\n    User,\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let role = symbols
            .iter()
            .find(|s| s.name == "Role")
            .expect("Role not found");
        let sig = role.signature.as_deref().expect("signature should be Some");
        assert!(sig.contains("enum Role"));
        assert!(sig.contains("Admin"));
    }

    // -----------------------------------------------------------------------
    // Scenario 11: Non-exported items are Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_non_exported_items_are_private() {
        let (_, symbols) = extract_ts_fixture("index.ts");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.visibility, Visibility::Private);

        let local = symbols
            .iter()
            .find(|s| s.name == "localConst")
            .expect("localConst not found");
        assert_eq!(local.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 12: extract_symbols dispatches correctly for TS and JS
    // (Already tested in extract.rs, but we verify the extractor itself)
    // -----------------------------------------------------------------------
    #[test]
    fn test_dispatch_typescript_extracts_symbols() {
        let source = "export function foo(): void {}\n";
        let symbols = parse_ts(source, "foo.ts");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_dispatch_javascript_extracts_symbols() {
        let source = "function foo() {}\n";
        let symbols = parse_js(source, "foo.js");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    // -----------------------------------------------------------------------
    // Additional: line numbers, empty/broken, private methods
    // -----------------------------------------------------------------------
    #[test]
    fn test_line_numbers_are_1_based() {
        let source = "function first() {}\nfunction second() {}\n";
        let symbols = parse_ts(source, "test.ts");
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

    #[test]
    fn test_empty_source_returns_empty_vec() {
        let symbols = parse_ts("", "empty.ts");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_broken_source_returns_partial_results() {
        let source = "function good() {}\nfunction broken( {}\nclass S {}\n";
        let symbols = parse_ts(source, "broken.ts");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_private_method_has_private_visibility() {
        let (_, symbols) = extract_ts_fixture("services.ts");
        let service = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("UserService not found");
        let validate = service
            .children
            .iter()
            .find(|c| c.name == "validate")
            .expect("validate not found");
        assert_eq!(validate.visibility, Visibility::Private);
    }

    #[test]
    fn test_non_exported_class_is_private() {
        let (_, symbols) = extract_ts_fixture("services.ts");
        let internal = symbols
            .iter()
            .find(|s| s.name == "InternalService")
            .expect("InternalService not found");
        assert_eq!(internal.kind, SymbolKind::Class);
        assert_eq!(internal.visibility, Visibility::Private);
    }

    #[test]
    fn test_methods_are_children_not_top_level() {
        let source = "class Foo {\n    bar(): void {}\n    baz(): void {}\n}\n";
        let symbols = parse_ts(source, "test.ts");
        assert!(
            !symbols.iter().any(|s| s.kind == SymbolKind::Method),
            "methods must not appear top-level"
        );
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        assert_eq!(foo.children.len(), 2);
        assert_eq!(foo.children[0].name, "bar");
        assert_eq!(foo.children[1].name, "baz");
    }

    #[test]
    fn test_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["index.ts", "models.ts", "services.ts"] {
            let (_, symbols) = extract_ts_fixture(fixture);
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

    #[test]
    fn test_js_fixture_symbols_have_body_and_signature() {
        let (_, symbols) = extract_js_fixture("utils.js");
        for sym in &symbols {
            assert!(sym.body.is_some(), "symbol {} should have a body", sym.name);
            assert!(
                sym.signature.is_some(),
                "symbol {} should have a signature",
                sym.name
            );
            for child in &sym.children {
                assert!(
                    child.body.is_some(),
                    "child {} of {} should have a body",
                    child.name,
                    sym.name
                );
                assert!(
                    child.signature.is_some(),
                    "child {} of {} should have a signature",
                    child.name,
                    sym.name
                );
            }
        }
    }

    #[test]
    fn test_arrow_function_in_js_is_function() {
        let (_, symbols) = extract_js_fixture("utils.js");
        let double = symbols
            .iter()
            .find(|s| s.name == "double")
            .expect("double not found");
        assert_eq!(double.kind, SymbolKind::Function);
        assert_eq!(double.visibility, Visibility::Private);
    }

    #[test]
    fn test_exported_js_function_is_public() {
        let (_, symbols) = extract_js_fixture("utils.js");
        let exported = symbols
            .iter()
            .find(|s| s.name == "exported")
            .expect("exported not found");
        assert_eq!(exported.kind, SymbolKind::Function);
        assert_eq!(exported.visibility, Visibility::Public);
    }

    #[test]
    fn test_exported_js_class_is_public() {
        let (_, symbols) = extract_js_fixture("utils.js");
        let exported = symbols
            .iter()
            .find(|s| s.name == "ExportedLogger")
            .expect("ExportedLogger not found");
        assert_eq!(exported.kind, SymbolKind::Class);
        assert_eq!(exported.visibility, Visibility::Public);
    }

    #[test]
    fn test_doc_comment_extracted_for_exported_function() {
        let (_, symbols) = extract_ts_fixture("index.ts");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert!(greet.doc.is_some(), "greet should have a doc comment");
        assert!(greet.doc.as_deref().unwrap().contains("Greet someone"));
    }

    #[test]
    fn test_method_body_and_signature() {
        let source = "class Foo {\n    bar(x: number): number {\n        return x * 2;\n    }\n}\n";
        let symbols = parse_ts(source, "test.ts");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let bar = foo
            .children
            .iter()
            .find(|c| c.name == "bar")
            .expect("bar not found");
        let body = bar.body.as_deref().expect("body should be Some");
        assert!(body.contains("return x * 2"));
        let sig = bar.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "bar(x: number): number");
    }

    #[test]
    fn test_generic_type_alias_extracted() {
        let (_, symbols) = extract_ts_fixture("models.ts");
        let result = symbols
            .iter()
            .find(|s| s.name == "Result")
            .expect("Result not found");
        assert_eq!(result.kind, SymbolKind::Type);
        assert_eq!(result.visibility, Visibility::Public);
    }

    #[test]
    fn test_multiple_interfaces_extracted() {
        let (_, symbols) = extract_ts_fixture("models.ts");
        let interfaces: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 2);
        let names: Vec<&str> = interfaces.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"User"));
        assert!(names.contains(&"Serializable"));
    }
}
