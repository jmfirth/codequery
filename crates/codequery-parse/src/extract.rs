//! Symbol extraction from Rust tree-sitter ASTs.
//!
//! Given a parsed tree and source text, walks the AST and extracts all symbol
//! definitions — functions, structs, enums, traits, impl blocks, methods,
//! constants, type aliases, statics, and modules.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

/// Extract all symbol definitions from a parsed Rust source file.
///
/// Walks the AST and identifies definitions (functions, structs, enums, etc.),
/// their visibility, nesting structure, and doc comments.
///
/// # Arguments
/// * `source` — the source text (needed to extract node text via byte ranges)
/// * `tree` — the parsed tree-sitter tree
/// * `file` — the file path (stored in each Symbol for output)
#[must_use]
pub fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
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
            })
        }
        "struct_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Struct,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "enum_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Enum,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "trait_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Trait,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "impl_item" => {
            let impl_name = extract_impl_name(node, source)?;
            let children = extract_impl_methods(node, source, file);
            Some(Symbol {
                name: impl_name,
                kind: SymbolKind::Impl,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Private,
                children,
                doc: extract_doc_comment(node, source),
            })
        }
        "type_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Type,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "const_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Const,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "static_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Static,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
            })
        }
        "mod_item" => {
            let name = node_field_text(node, "name", source)?;
            Some(Symbol {
                name,
                kind: SymbolKind::Module,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: extract_visibility(node, source),
                children: vec![],
                doc: extract_doc_comment(node, source),
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
            methods.push(Symbol {
                name,
                kind: SymbolKind::Method,
                file: file.to_path_buf(),
                line: child.start_position().row + 1,
                column: child.start_position().column,
                end_line: child.end_position().row + 1,
                visibility: extract_visibility(child, source),
                children: vec![],
                doc: extract_doc_comment(child, source),
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
        extract_symbols(source, &tree, Path::new(file))
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
        let symbols = extract_symbols(&source, &tree, &path);
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
        let symbols = extract_symbols(&source, &tree, &fixture_path);

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
}
