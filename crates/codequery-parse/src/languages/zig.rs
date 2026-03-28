//! Zig-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, structs
//! (via `const Name = struct { ... }`), enums, unions, constants, and test
//! declarations. Visibility is determined by the presence of the `pub` keyword.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Zig language extractor.
pub struct ZigExtractor;

impl LanguageExtractor for ZigExtractor {
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
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a Zig symbol.
///
/// - **Function/Test**: declaration line up to the opening `{`, trimmed
/// - **Struct/Enum**: the full body (definition is the signature)
/// - **Const**: the full declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Test => extract_fn_signature(body_text),
        SymbolKind::Const => extract_single_line_signature(body_text),
        _ => body_text.to_string(),
    }
}

/// Extract function signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.trim().to_string()
    }
}

/// Extract a single-line signature for constants.
fn extract_single_line_signature(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Extract top-level symbols from a node, appending to `symbols`.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let visibility = if has_pub_child(node, source) {
                Visibility::Public
            } else {
                Visibility::Private
            };
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, SymbolKind::Function);
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Function,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "variable_declaration" => {
            extract_variable_declaration(node, source, file, symbols);
        }
        "test_declaration" => {
            let name = extract_test_name(node, source);
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, SymbolKind::Test);
            symbols.push(Symbol {
                name,
                kind: SymbolKind::Test,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: Visibility::Private,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        _ => {}
    }
}

/// Extract symbols from a Zig `variable_declaration` node.
///
/// In Zig, structs, enums, unions, and constants are all declared via
/// `const Name = <value>`. The kind is determined by the value:
/// - `struct_declaration` -> Struct
/// - `enum_declaration` -> Enum
/// - `union_declaration` -> Struct (mapped to Struct since `SymbolKind` has no Union)
/// - `builtin_function` (`@import`) -> skip (import, not a symbol)
/// - Other -> Const
fn extract_variable_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    // Find the identifier (name) child
    let Some(name) = find_identifier_child(node, source) else {
        return;
    };

    let visibility = if has_pub_child(node, source) {
        Visibility::Public
    } else {
        Visibility::Private
    };

    // Determine the kind by looking at the value expression
    let kind = determine_variable_kind(node);

    // Skip @import declarations — these are imports, not symbols
    if kind.is_none() {
        return;
    }
    let kind = kind.unwrap_or(SymbolKind::Const);

    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    symbols.push(Symbol {
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
    });
}

/// Determine the symbol kind for a variable declaration by inspecting the value.
///
/// Returns `None` if this is an `@import` (should be skipped).
fn determine_variable_kind(node: tree_sitter::Node<'_>) -> Option<SymbolKind> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "struct_declaration" | "union_declaration" => return Some(SymbolKind::Struct),
            "enum_declaration" => return Some(SymbolKind::Enum),
            "builtin_function" => return None, // @import
            _ => {}
        }
    }
    Some(SymbolKind::Const)
}

/// Find the `identifier` child of a node and return its text.
fn find_identifier_child(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok().map(String::from);
        }
    }
    None
}

/// Check whether a node has a `pub` child keyword.
fn has_pub_child(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "pub" {
            return true;
        }
        // Also check for visibility_qualifier which wraps pub in some grammar versions
        if child.kind() == "visibility_qualifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                if text.starts_with("pub") {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract the test name from a `test_declaration` node.
///
/// Zig test names are string literals: `test "name" { ... }`
fn extract_test_name(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            return text
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
        }
    }
    // Fallback: unnamed test
    "unnamed_test".to_string()
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract doc comments preceding a definition node.
///
/// In Zig, doc comments use `///` syntax. The tree-sitter zig grammar
/// represents these as `comment` nodes. This looks for consecutive
/// `comment` siblings starting with `///` immediately preceding the node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "doc_comment" || sib.kind() == "line_comment" || sib.kind() == "comment" {
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

    /// Helper: parse source and extract symbols for the given file path.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Zig).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        ZigExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture zig project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/zig_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Zig).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = ZigExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract pub fn as Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_pub_fn_as_public_function() {
        let (_, symbols) = extract_fixture("main.zig");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract fn without pub as Function/Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_private_fn_as_private_function() {
        let (_, symbols) = extract_fixture("main.zig");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract struct via const assignment
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_struct_declaration() {
        let (_, symbols) = extract_fixture("main.zig");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
        assert_eq!(point.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract enum via const assignment
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_enum_declaration() {
        let (_, symbols) = extract_fixture("main.zig");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        assert_eq!(color.kind, SymbolKind::Enum);
        assert_eq!(color.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Extract union via const assignment (mapped to Struct)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_union_declaration() {
        let (_, symbols) = extract_fixture("main.zig");
        let tagged = symbols
            .iter()
            .find(|s| s.name == "Tagged")
            .expect("Tagged not found");
        assert_eq!(tagged.kind, SymbolKind::Struct);
        assert_eq!(tagged.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Extract pub const as Const/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_pub_const_as_const_public() {
        let (_, symbols) = extract_fixture("main.zig");
        let max_size = symbols
            .iter()
            .find(|s| s.name == "MAX_SIZE")
            .expect("MAX_SIZE not found");
        assert_eq!(max_size.kind, SymbolKind::Const);
        assert_eq!(max_size.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Extract private const as Const/Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_private_const_as_const_private() {
        let (_, symbols) = extract_fixture("main.zig");
        let limit = symbols
            .iter()
            .find(|s| s.name == "internal_limit")
            .expect("internal_limit not found");
        assert_eq!(limit.kind, SymbolKind::Const);
        assert_eq!(limit.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 8: Extract test declarations as Test
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_test_declarations() {
        let (_, symbols) = extract_fixture("main.zig");
        let tests: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Test)
            .collect();
        assert_eq!(tests.len(), 2);
        assert!(tests.iter().any(|s| s.name == "basic greet"));
        assert!(tests.iter().any(|s| s.name == "helper works"));
    }

    // -----------------------------------------------------------------------
    // Scenario 9: @import declarations are skipped
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_import_declarations_skipped() {
        let (_, symbols) = extract_fixture("main.zig");
        assert!(
            !symbols.iter().any(|s| s.name == "std"),
            "std @import should not appear as a symbol"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "utils"),
            "utils @import should not appear as a symbol"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 10: Body and signature extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_function_body_contains_source() {
        let (_, symbols) = extract_fixture("main.zig");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("pub fn greet"));
        assert!(body.ends_with('}'));
    }

    #[test]
    fn test_extract_zig_function_signature_no_body() {
        let (_, symbols) = extract_fixture("main.zig");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert!(sig.starts_with("pub fn greet"));
        assert!(!sig.contains('{'));
    }

    #[test]
    fn test_extract_zig_const_signature() {
        let (_, symbols) = extract_fixture("main.zig");
        let max_size = symbols
            .iter()
            .find(|s| s.name == "MAX_SIZE")
            .expect("MAX_SIZE not found");
        let sig = max_size
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "pub const MAX_SIZE: usize = 1024");
    }

    // -----------------------------------------------------------------------
    // Scenario 11: Doc comments extracted
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_doc_comment() {
        let (_, symbols) = extract_fixture("main.zig");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.doc.as_deref(), Some("/// Greet a person by name."));
    }

    // -----------------------------------------------------------------------
    // Scenario 12: All symbols have body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_all_fixture_symbols_have_body_and_signature() {
        let (_, symbols) = extract_fixture("main.zig");
        assert!(
            !symbols.is_empty(),
            "expected symbols in main.zig, got none"
        );
        for sym in &symbols {
            assert!(sym.body.is_some(), "symbol {} should have a body", sym.name,);
            assert!(
                sym.signature.is_some(),
                "symbol {} should have a signature",
                sym.name,
            );
        }
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_zig_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.zig");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_zig_broken_source_no_panic() {
        let source = "pub fn good() void {}\npub fn broken( void {}\nconst X: u32 = 1;\n";
        let symbols = parse_and_extract(source, "broken.zig");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_zig_line_numbers_are_1_based() {
        let source = "fn first() void {}\nfn second() void {}\n";
        let symbols = parse_and_extract(source, "test.zig");
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
    fn test_extract_zig_struct_body_includes_fields() {
        let (_, symbols) = extract_fixture("main.zig");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        let body = point.body.as_deref().expect("body should be Some");
        assert!(body.contains("x: f64"));
        assert!(body.contains("y: f64"));
    }

    #[test]
    fn test_extract_zig_enum_body_includes_variants() {
        let (_, symbols) = extract_fixture("main.zig");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("Color not found");
        let body = color.body.as_deref().expect("body should be Some");
        assert!(body.contains("red"));
        assert!(body.contains("green"));
        assert!(body.contains("blue"));
    }
}
