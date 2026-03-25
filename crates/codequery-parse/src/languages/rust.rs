//! Rust-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, structs,
//! enums, traits, impl blocks, methods, constants, type aliases, statics,
//! and modules. Also provides body and signature extraction for each symbol kind.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Rust language extractor.
pub struct RustExtractor;

impl LanguageExtractor for RustExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
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
///
/// Returns the complete source text between the node's start and end bytes.
/// Doc comments are separate preceding nodes and are NOT included — the body
/// starts at the definition keyword (e.g., `pub fn`, `struct`, etc.).
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a symbol.
///
/// The signature varies by symbol kind:
/// - **Function/Method**: declaration line up to the opening `{`, trimmed
/// - **Struct/Enum**: header + field/variant list (the full body is the signature)
/// - **Trait**: header + method signatures (the full body is the signature)
/// - **Impl**: the `impl ... for ...` header line
/// - **Type/Const/Static**: the full declaration line
/// - **Module**: `mod name` line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Test => {
            extract_fn_signature(body_text)
        }
        SymbolKind::Struct
        | SymbolKind::Enum
        | SymbolKind::Trait
        | SymbolKind::Class
        | SymbolKind::Interface => body_text.to_string(),
        SymbolKind::Impl => extract_impl_signature(body_text),
        SymbolKind::Type | SymbolKind::Const | SymbolKind::Static => {
            extract_single_line_signature(body_text)
        }
        SymbolKind::Module => extract_mod_signature(body_text),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    // Find the opening brace that starts the function body
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        // No brace found — might be a function declaration (e.g., in a trait)
        // Return the full text, trimmed of trailing semicolons and whitespace
        body.trim().trim_end_matches(';').trim().to_string()
    }
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

/// Extract impl signature: just the header line (`impl ... for ...`).
fn extract_impl_signature(body: &str) -> String {
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Extract a single-line signature for type aliases, consts, and statics.
fn extract_single_line_signature(body: &str) -> String {
    // For multi-line declarations, take the full text trimmed
    // For single-line, it's just the line
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Extract module signature: just the `mod name` portion.
fn extract_mod_signature(body: &str) -> String {
    // `mod name;` or `mod name { ... }`
    if let Some(brace_pos) = find_top_level_brace(body) {
        body[..brace_pos].trim().to_string()
    } else {
        body.lines()
            .next()
            .unwrap_or("")
            .trim_end_matches(';')
            .trim()
            .to_string()
    }
}

/// Extract a top-level symbol from a node, if it represents a definition.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let kind_str = node.kind();
    match kind_str {
        "function_item" => {
            let name = node_field_text(node, "name", source)?;
            let is_test = has_test_attribute(node, source);
            let kind = if is_test {
                SymbolKind::Test
            } else {
                SymbolKind::Function
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
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "struct_item" => {
            let name = node_field_text(node, "name", source)?;
            let kind = SymbolKind::Struct;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "enum_item" => {
            let name = node_field_text(node, "name", source)?;
            let kind = SymbolKind::Enum;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "trait_item" => {
            let name = node_field_text(node, "name", source)?;
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
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "impl_item" => {
            let impl_name = extract_impl_name(node, source)?;
            let children = extract_impl_methods(node, source, file);
            let kind = SymbolKind::Impl;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name: impl_name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Private,
                children,
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "type_item" => {
            let name = node_field_text(node, "name", source)?;
            let kind = SymbolKind::Type;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "const_item" => {
            let name = node_field_text(node, "name", source)?;
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
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "static_item" => {
            let name = node_field_text(node, "name", source)?;
            let kind = SymbolKind::Static;
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            Some(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "mod_item" => {
            let name = node_field_text(node, "name", source)?;
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
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        _ => None,
    }
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract visibility from a node by looking for a `visibility_modifier` child.
fn extract_visibility(node: tree_sitter::Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            if text.starts_with("pub(crate)")
                || text.starts_with("pub(super)")
                || text.starts_with("pub(in")
            {
                return Visibility::Crate;
            }
            if text == "pub" {
                return Visibility::Public;
            }
        }
    }
    Visibility::Private
}

/// Build the name for an `impl_item` node.
///
/// Simple impl: `impl Router { ... }` -> `"Router"`
/// Trait impl: `impl Display for Router { ... }` -> `"Display for Router"`
fn extract_impl_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let type_node = node.child_by_field_name("type")?;
    let type_name = type_node.utf8_text(source.as_bytes()).ok()?;

    if let Some(trait_node) = node.child_by_field_name("trait") {
        let trait_name = trait_node.utf8_text(source.as_bytes()).ok()?;
        Some(format!("{trait_name} for {type_name}"))
    } else {
        Some(type_name.to_string())
    }
}

/// Extract methods from an `impl_item` body (`declaration_list`).
fn extract_impl_methods(
    impl_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Vec<Symbol> {
    let mut methods = Vec::new();
    let Some(body) = impl_node.child_by_field_name("body") else {
        return methods;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        if child.kind() == "function_item" {
            let Some(name) = node_field_text(child, "name", source) else {
                continue;
            };
            let kind = SymbolKind::Method;
            let method_body = extract_body(source, &child);
            let method_sig = extract_signature(source, &child, kind);
            methods.push(Symbol {
                name,
                kind,
                file: file.to_path_buf(),
                line: child.start_position().row + 1,
                column: child.start_position().column,
                end_line: child.end_position().row + 1,
                visibility: extract_visibility(child, source),
                children: vec![],
                doc: extract_doc_comment(child, source),
                body: Some(method_body),
                signature: Some(method_sig),
            });
        }
    }

    methods
}

/// Check if a function has a `#[test]` attribute as a preceding sibling.
fn has_test_attribute(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        if sib.kind() == "attribute_item" {
            if attribute_contains_test(sib, source) {
                return true;
            }
            // Check further preceding siblings (there might be multiple attributes)
            sibling = sib.prev_sibling();
            continue;
        }
        // Also skip line_comment and block_comment siblings (doc comments between
        // attribute and function)
        if sib.kind() == "line_comment" || sib.kind() == "block_comment" {
            sibling = sib.prev_sibling();
            continue;
        }
        break;
    }
    false
}

/// Check if an `attribute_item` node contains a `test` identifier.
fn attribute_contains_test(attr_node: tree_sitter::Node<'_>, source: &str) -> bool {
    // Structure: attribute_item -> attribute -> identifier "test"
    let mut cursor = attr_node.walk();
    for child in attr_node.children(&mut cursor) {
        if child.kind() == "attribute" {
            // The attribute child's text should be "test" for #[test]
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "identifier" {
                    if let Ok(text) = inner.utf8_text(source.as_bytes()) {
                        if text == "test" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Extract doc comments preceding a definition node.
///
/// Looks for consecutive `line_comment` siblings starting with `///`
/// immediately before the node (no non-comment, non-attribute gap).
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    // Walk backward through preceding siblings, collecting doc comments.
    // Stop at any non-comment, non-attribute node.
    while let Some(sib) = sibling {
        if sib.kind() == "line_comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with("///") {
                    doc_lines.push(trimmed.to_string());
                    sibling = sib.prev_sibling();
                    continue;
                }
            }
            // Non-doc line comment — stop
            break;
        }
        if sib.kind() == "attribute_item" {
            // Skip attributes (like #[derive(...)])
            sibling = sib.prev_sibling();
        } else {
            break;
        }
    }

    if doc_lines.is_empty() {
        return None;
    }

    // Reverse because we collected back-to-front
    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RustParser;
    use std::path::PathBuf;

    /// Helper: parse source and extract symbols for the given file path.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        RustExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture rust project source directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project/src")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = RustParser::new().unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = RustExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract functions from lib.rs — finds `greet` as Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_function_from_lib_finds_greet_as_public() {
        let (_, symbols) = extract_fixture("lib.rs");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract structs from models.rs — finds `User` as Struct/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_struct_from_models_finds_user_as_public() {
        let (_, symbols) = extract_fixture("models.rs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.kind, SymbolKind::Struct);
        assert_eq!(user.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract enums from models.rs — finds `Role` as Enum/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_enum_from_models_finds_role_as_public() {
        let (_, symbols) = extract_fixture("models.rs");
        let role = symbols
            .iter()
            .find(|s| s.name == "Role")
            .expect("Role not found");
        assert_eq!(role.kind, SymbolKind::Enum);
        assert_eq!(role.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract traits from traits.rs — finds `Validate` and `Summary`
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_traits_from_traits_finds_validate_and_summary() {
        let (_, symbols) = extract_fixture("traits.rs");
        let validate = symbols
            .iter()
            .find(|s| s.name == "Validate")
            .expect("Validate not found");
        assert_eq!(validate.kind, SymbolKind::Trait);
        assert_eq!(validate.visibility, Visibility::Public);

        let summary = symbols
            .iter()
            .find(|s| s.name == "Summary")
            .expect("Summary not found");
        assert_eq!(summary.kind, SymbolKind::Trait);
        assert_eq!(summary.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Extract impl blocks from services.rs — finds `impl User` with methods
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_impl_block_from_services_finds_user_with_methods() {
        let (_, symbols) = extract_fixture("services.rs");
        let user_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "User")
            .expect("impl User not found");
        assert!(!user_impl.children.is_empty());
        let method_names: Vec<&str> = user_impl.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"new"));
        assert!(method_names.contains(&"is_adult"));
        assert!(method_names.contains(&"internal_helper"));
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Extract trait impl — finds `impl Validate for User`
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_trait_impl_finds_validate_for_user() {
        let (_, symbols) = extract_fixture("services.rs");
        let trait_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "Validate for User")
            .expect("impl Validate for User not found");
        assert_eq!(trait_impl.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Methods inside impl are SymbolKind::Method children
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_methods_inside_impl_are_method_kind_children() {
        let (_, symbols) = extract_fixture("services.rs");
        let user_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "User")
            .expect("impl User not found");
        for child in &user_impl.children {
            assert_eq!(child.kind, SymbolKind::Method);
        }
        // Methods should NOT appear at top level
        assert!(
            !symbols
                .iter()
                .any(|s| s.name == "new" && s.kind == SymbolKind::Method),
            "methods should not be top-level"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 8: Visibility — pub fn -> Public, fn -> Private, pub(crate) fn -> Crate
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_visibility_pub_private_and_crate() {
        let (_, symbols) = extract_fixture("utils/helpers.rs");
        let format_name = symbols
            .iter()
            .find(|s| s.name == "format_name")
            .expect("format_name not found");
        assert_eq!(format_name.visibility, Visibility::Public);

        let internal = symbols
            .iter()
            .find(|s| s.name == "internal_util")
            .expect("internal_util not found");
        assert_eq!(internal.visibility, Visibility::Crate);

        // Also check a private function inside an impl
        let (_, services) = extract_fixture("services.rs");
        let user_impl = services
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "User")
            .expect("impl User not found");
        let helper = user_impl
            .children
            .iter()
            .find(|c| c.name == "internal_helper")
            .expect("internal_helper not found");
        assert_eq!(helper.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 9: Extract const from lib.rs — finds MAX_RETRIES as Const/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_const_from_lib_finds_max_retries_as_public() {
        let (_, symbols) = extract_fixture("lib.rs");
        let max_retries = symbols
            .iter()
            .find(|s| s.name == "MAX_RETRIES")
            .expect("MAX_RETRIES not found");
        assert_eq!(max_retries.kind, SymbolKind::Const);
        assert_eq!(max_retries.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 10: Extract static from lib.rs — finds INSTANCE_COUNT
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_static_from_lib_finds_instance_count() {
        let (_, symbols) = extract_fixture("lib.rs");
        let count = symbols
            .iter()
            .find(|s| s.name == "INSTANCE_COUNT")
            .expect("INSTANCE_COUNT not found");
        assert_eq!(count.kind, SymbolKind::Static);
    }

    // -----------------------------------------------------------------------
    // Scenario 11: Extract type alias from models.rs — finds UserId as Type/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_type_alias_from_models_finds_userid_as_public() {
        let (_, symbols) = extract_fixture("models.rs");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserId")
            .expect("UserId not found");
        assert_eq!(userid.kind, SymbolKind::Type);
        assert_eq!(userid.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 12: Extract module declarations from lib.rs
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_modules_from_lib_finds_all_module_declarations() {
        let (_, symbols) = extract_fixture("lib.rs");
        let module_names: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .map(|s| s.name.as_str())
            .collect();
        assert!(module_names.contains(&"models"), "models module not found");
        assert!(module_names.contains(&"traits"), "traits module not found");
        assert!(
            module_names.contains(&"services"),
            "services module not found"
        );
        assert!(module_names.contains(&"utils"), "utils module not found");
    }

    // -----------------------------------------------------------------------
    // Scenario 13: Detect #[test] functions as SymbolKind::Test
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_test_functions_detected_as_test_kind() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/tests/integration.rs");
        let mut parser = RustParser::new().unwrap();
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();
        let symbols = RustExtractor::extract_symbols(&source, &tree, &fixture_path);

        let test_greet = symbols
            .iter()
            .find(|s| s.name == "test_greet")
            .expect("test_greet not found");
        assert_eq!(test_greet.kind, SymbolKind::Test);

        let test_greet_empty = symbols
            .iter()
            .find(|s| s.name == "test_greet_empty")
            .expect("test_greet_empty not found");
        assert_eq!(test_greet_empty.kind, SymbolKind::Test);
    }

    // -----------------------------------------------------------------------
    // Scenario 14: Extract doc comments — User struct has doc
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_doc_comments_on_user_struct() {
        let (_, symbols) = extract_fixture("models.rs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.doc.as_deref(), Some("/// A user in the system."));
    }

    // -----------------------------------------------------------------------
    // Scenario 15: Correct 1-based line numbers
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_line_numbers_are_1_based() {
        let source = "fn first() {}\nfn second() {}\n";
        let symbols = parse_and_extract(source, "test.rs");
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

    // -----------------------------------------------------------------------
    // Scenario 16: Correct nesting — methods are children, not top-level
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_nesting_methods_are_children_not_top_level() {
        let source = "impl Foo {\n    fn bar(&self) {}\n    fn baz(&self) {}\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        // No top-level Method symbols
        assert!(
            !symbols.iter().any(|s| s.kind == SymbolKind::Method),
            "methods must not appear top-level"
        );
        let impl_foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo impl not found");
        assert_eq!(impl_foo.children.len(), 2);
        assert_eq!(impl_foo.children[0].name, "bar");
        assert_eq!(impl_foo.children[1].name, "baz");
    }

    // -----------------------------------------------------------------------
    // Scenario 17: Empty/broken source returns empty vec (no panic)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.rs");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_broken_source_returns_partial_results_no_panic() {
        let source = "fn good() {}\nfn broken( {}\nstruct S {}\n";
        let symbols = parse_and_extract(source, "broken.rs");
        // Should extract at least something without panicking
        // The good function and struct should be present
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 18: Private helper method extracted with Visibility::Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_private_helper_method_has_private_visibility() {
        let (_, symbols) = extract_fixture("services.rs");
        let user_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "User")
            .expect("impl User not found");
        let helper = user_impl
            .children
            .iter()
            .find(|c| c.name == "internal_helper")
            .expect("internal_helper not found");
        assert_eq!(helper.visibility, Visibility::Private);
        assert_eq!(helper.kind, SymbolKind::Method);
    }

    // =======================================================================
    // Body and Signature Extraction Tests (Task 013)
    // =======================================================================

    // -----------------------------------------------------------------------
    // Scenario 19: Function body extraction returns complete source text
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_function_returns_complete_source_text() {
        let source = "/// A greeting function.\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert_eq!(
            body,
            "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 20: Function signature is just the declaration line (no body)
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_function_is_declaration_without_body() {
        let source = "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "pub fn greet(name: &str) -> String");
    }

    // -----------------------------------------------------------------------
    // Scenario 21: Struct body includes fields
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_struct_includes_fields() {
        let source = "pub struct User {\n    pub name: String,\n    pub age: u32,\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let body = user.body.as_deref().expect("body should be Some");
        assert!(body.contains("pub name: String"));
        assert!(body.contains("pub age: u32"));
    }

    // -----------------------------------------------------------------------
    // Scenario 22: Struct signature includes fields (full definition is the sig)
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_struct_includes_fields() {
        let source = "pub struct User {\n    pub name: String,\n    pub age: u32,\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("signature should be Some");
        assert_eq!(
            sig,
            "pub struct User {\n    pub name: String,\n    pub age: u32,\n}"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 23: Trait signature includes method declarations
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_trait_includes_method_declarations() {
        let source = "pub trait Validate {\n    fn is_valid(&self) -> bool;\n    fn errors(&self) -> Vec<String> { Vec::new() }\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let validate = symbols
            .iter()
            .find(|s| s.name == "Validate")
            .expect("Validate not found");
        let sig = validate
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert!(sig.contains("fn is_valid(&self) -> bool"));
        assert!(sig.contains("fn errors(&self) -> Vec<String>"));
    }

    // -----------------------------------------------------------------------
    // Scenario 24: Impl signature is the header line
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_impl_is_header_line() {
        let source = "impl User {\n    pub fn new() -> Self { Self {} }\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let user_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl)
            .expect("impl not found");
        let sig = user_impl
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "impl User");
    }

    // -----------------------------------------------------------------------
    // Scenario 25: Trait impl signature is the header line
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_trait_impl_is_header_line() {
        let source = "impl Validate for User {\n    fn is_valid(&self) -> bool { true }\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let trait_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl)
            .expect("impl not found");
        let sig = trait_impl
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "impl Validate for User");
    }

    // -----------------------------------------------------------------------
    // Scenario 26: Const signature is the full declaration
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_const_is_full_declaration() {
        let source = "pub const MAX_RETRIES: u32 = 3;\n";
        let symbols = parse_and_extract(source, "test.rs");
        let max_retries = symbols
            .iter()
            .find(|s| s.name == "MAX_RETRIES")
            .expect("MAX_RETRIES not found");
        let sig = max_retries
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "pub const MAX_RETRIES: u32 = 3");
    }

    // -----------------------------------------------------------------------
    // Scenario 27: Static signature is the full declaration
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_static_is_full_declaration() {
        let source = "static COUNTER: u32 = 0;\n";
        let symbols = parse_and_extract(source, "test.rs");
        let counter = symbols
            .iter()
            .find(|s| s.name == "COUNTER")
            .expect("COUNTER not found");
        let sig = counter
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "static COUNTER: u32 = 0");
    }

    // -----------------------------------------------------------------------
    // Scenario 28: Type alias signature is the full line
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_type_alias_is_full_line() {
        let source = "pub type UserId = u64;\n";
        let symbols = parse_and_extract(source, "test.rs");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserId")
            .expect("UserId not found");
        let sig = userid
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "pub type UserId = u64");
    }

    // -----------------------------------------------------------------------
    // Scenario 29: Module signature is just `mod name`
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_module_is_mod_name() {
        let source = "pub mod models;\n";
        let symbols = parse_and_extract(source, "test.rs");
        let models = symbols
            .iter()
            .find(|s| s.name == "models")
            .expect("models not found");
        let sig = models
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "pub mod models");
    }

    // -----------------------------------------------------------------------
    // Scenario 30: Doc comments are NOT part of the body
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_excludes_doc_comments() {
        let source = "/// This is a doc comment.\npub fn documented() -> bool {\n    true\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let documented = symbols
            .iter()
            .find(|s| s.name == "documented")
            .expect("documented not found");
        let body = documented.body.as_deref().expect("body should be Some");
        assert!(
            !body.starts_with("///"),
            "body should not start with doc comment"
        );
        assert!(body.starts_with("pub fn documented"));
    }

    // -----------------------------------------------------------------------
    // Scenario 31: All symbols in fixture project have non-None body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_all_fixture_symbols_have_body_and_signature() {
        for fixture in &[
            "lib.rs",
            "models.rs",
            "traits.rs",
            "services.rs",
            "utils/helpers.rs",
        ] {
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
                // Also check children (methods inside impl blocks)
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

    // -----------------------------------------------------------------------
    // Scenario 32: Method body and signature inside impl
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_and_signature_for_method_inside_impl() {
        let source = "impl Foo {\n    pub fn bar(&self) -> u32 {\n        42\n    }\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let impl_foo = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl)
            .expect("impl Foo not found");
        let bar = impl_foo
            .children
            .iter()
            .find(|c| c.name == "bar")
            .expect("bar not found");
        let body = bar.body.as_deref().expect("body should be Some");
        assert!(body.contains("42"));
        assert!(body.starts_with("pub fn bar"));

        let sig = bar.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "pub fn bar(&self) -> u32");
    }

    // -----------------------------------------------------------------------
    // Scenario 33: Enum body includes variants
    // -----------------------------------------------------------------------
    #[test]
    fn test_body_enum_includes_variants() {
        let source = "pub enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        let body = color.body.as_deref().expect("body should be Some");
        assert!(body.contains("Red"));
        assert!(body.contains("Green"));
        assert!(body.contains("Blue"));
    }

    // -----------------------------------------------------------------------
    // Scenario 34: Enum signature equals body (full definition is signature)
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_enum_equals_full_definition() {
        let source = "pub enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        let body = color.body.as_deref().expect("body should be Some");
        let sig = color.signature.as_deref().expect("sig should be Some");
        assert_eq!(body, sig);
    }

    // -----------------------------------------------------------------------
    // Scenario 35: Module with body has correct signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_signature_module_with_body_is_just_header() {
        let source = "mod inner {\n    fn hidden() {}\n}\n";
        let symbols = parse_and_extract(source, "test.rs");
        let inner = symbols
            .iter()
            .find(|s| s.name == "inner")
            .expect("inner not found");
        let sig = inner
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "mod inner");
    }

    // -----------------------------------------------------------------------
    // Scenario 36: Fixture greet function body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_greet_body_and_signature() {
        let (_, symbols) = extract_fixture("lib.rs");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("pub fn greet(name: &str) -> String"));
        assert!(body.contains("format!(\"Hello, {name}!\")"));
        assert!(body.ends_with('}'));

        let sig = greet.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "pub fn greet(name: &str) -> String");
    }

    // -----------------------------------------------------------------------
    // Scenario 37: Fixture User struct signature matches spec
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_user_struct_signature() {
        let (_, symbols) = extract_fixture("models.rs");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("sig should be Some");
        assert!(sig.contains("pub struct User"));
        assert!(sig.contains("pub name: String"));
        assert!(sig.contains("pub age: u32"));
    }

    // -----------------------------------------------------------------------
    // Scenario 38: Fixture Validate trait signature includes methods
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_validate_trait_signature_includes_methods() {
        let (_, symbols) = extract_fixture("traits.rs");
        let validate = symbols
            .iter()
            .find(|s| s.name == "Validate")
            .expect("Validate not found");
        let sig = validate.signature.as_deref().expect("sig should be Some");
        assert!(sig.contains("fn is_valid(&self) -> bool"));
        assert!(sig.contains("fn errors(&self) -> Vec<String>"));
    }

    // -----------------------------------------------------------------------
    // Scenario 39: Fixture impl User signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_impl_user_signature() {
        let (_, symbols) = extract_fixture("services.rs");
        let user_impl = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Impl && s.name == "User")
            .expect("impl User not found");
        let sig = user_impl.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "impl User");
    }

    // -----------------------------------------------------------------------
    // Scenario 40: Fixture UserId type alias signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_fixture_userid_type_alias_signature() {
        let (_, symbols) = extract_fixture("models.rs");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserId")
            .expect("UserId not found");
        let sig = userid.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "pub type UserId = u64");
    }
}
