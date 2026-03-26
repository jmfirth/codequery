//! Go-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, methods
//! (with receiver), structs, interfaces, type aliases, constants, and
//! module-level variables. Go visibility is determined by capitalization:
//! uppercase first letter is public, lowercase is private.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Go language extractor.
pub struct GoExtractor;

impl LanguageExtractor for GoExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut cursor = root.walk();
        let is_test_file = file
            .file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|f| f.ends_with("_test.go"));

        for child in root.children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            extract_top_level(child, source, file, is_test_file, &mut symbols);
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

/// Extract the type signature of a Go symbol.
///
/// The signature varies by symbol kind:
/// - **Function/Method/Test**: declaration line up to the opening `{`, trimmed
/// - **Struct/Interface**: `type Name struct { ... }` or `type Name interface { ... }` header
/// - **Type**: `type Name = underlying` full declaration
/// - **Const/Static**: full declaration line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Test => {
            extract_fn_signature(body_text)
        }
        SymbolKind::Type | SymbolKind::Const | SymbolKind::Static => {
            extract_single_line_signature(body_text)
        }
        _ => body_text.to_string(),
    }
}

/// Extract function/method signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        body.trim().to_string()
    }
}

/// Extract a single-line signature for type aliases, consts, and vars.
fn extract_single_line_signature(body: &str) -> String {
    body.lines().next().unwrap_or("").trim().to_string()
}

/// Determine Go visibility by capitalization of the first character.
///
/// In Go, identifiers starting with an uppercase letter are exported (public),
/// while those starting with a lowercase letter are unexported (private).
fn go_visibility(name: &str) -> Visibility {
    name.chars().next().map_or(Visibility::Private, |c| {
        if c.is_uppercase() {
            Visibility::Public
        } else {
            Visibility::Private
        }
    })
}

/// Extract top-level symbols from a node, appending to `symbols`.
///
/// Some Go declarations (e.g., grouped `const (...)`) produce multiple symbols
/// from a single AST node, so this takes `&mut Vec` instead of returning `Option`.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    is_test_file: bool,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let kind = if is_test_file && name.starts_with("Test") {
                SymbolKind::Test
            } else {
                SymbolKind::Function
            };
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, kind);
            symbols.push(Symbol {
                name: name.clone(),
                kind,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: go_visibility(&name),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "method_declaration" => {
            let Some(name) = node_field_text(node, "name", source) else {
                return;
            };
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, SymbolKind::Method);
            symbols.push(Symbol {
                name: name.clone(),
                kind: SymbolKind::Method,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: go_visibility(&name),
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            });
        }
        "type_declaration" => {
            extract_type_declaration(node, source, file, symbols);
        }
        "const_declaration" => {
            extract_const_declaration(node, source, file, symbols);
        }
        "var_declaration" => {
            extract_var_declaration(node, source, file, symbols);
        }
        _ => {}
    }
}

/// Extract symbols from a `type_declaration` node.
///
/// A type declaration may contain one or more `type_spec` or `type_alias` children:
/// - `type_spec` with `struct_type` -> Struct
/// - `type_spec` with `interface_type` -> Interface
/// - `type_spec` with other types -> Type
/// - `type_alias` -> Type
fn extract_type_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_spec" => {
                let Some(name) = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .map(String::from)
                else {
                    continue;
                };

                let type_child = child.child_by_field_name("type");
                let kind = type_child.map_or(SymbolKind::Type, |t| match t.kind() {
                    "struct_type" => SymbolKind::Struct,
                    "interface_type" => SymbolKind::Interface,
                    _ => SymbolKind::Type,
                });

                let body = extract_body(source, &child);
                let signature = extract_signature(source, &child, kind);
                symbols.push(Symbol {
                    name: name.clone(),
                    kind,
                    file: file.to_path_buf(),
                    line: child.start_position().row + 1,
                    column: child.start_position().column,
                    end_line: child.end_position().row + 1,
                    visibility: go_visibility(&name),
                    children: vec![],
                    doc: extract_doc_comment(node, source),
                    body: Some(body),
                    signature: Some(signature),
                });
            }
            "type_alias" => {
                let Some(name) = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .map(String::from)
                else {
                    continue;
                };

                let kind = SymbolKind::Type;
                let body = extract_body(source, &child);
                let signature = extract_signature(source, &child, kind);
                symbols.push(Symbol {
                    name: name.clone(),
                    kind,
                    file: file.to_path_buf(),
                    line: child.start_position().row + 1,
                    column: child.start_position().column,
                    end_line: child.end_position().row + 1,
                    visibility: go_visibility(&name),
                    children: vec![],
                    doc: extract_doc_comment(node, source),
                    body: Some(body),
                    signature: Some(signature),
                });
            }
            _ => {}
        }
    }
}

/// Extract symbols from a `const_declaration` node.
///
/// Handles both single `const X = 1` and grouped `const ( X = 1; Y = 2 )` forms.
fn extract_const_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_spec" {
            extract_const_spec(child, node, source, file, symbols);
        }
    }
}

/// Extract a single const from a `const_spec` node.
fn extract_const_spec(
    spec: tree_sitter::Node<'_>,
    decl: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let Some(name) = spec
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(String::from)
    else {
        return;
    };

    let kind = SymbolKind::Const;
    let body = extract_body(source, &spec);
    let signature = extract_signature(source, &spec, kind);
    symbols.push(Symbol {
        name: name.clone(),
        kind,
        file: file.to_path_buf(),
        line: spec.start_position().row + 1,
        column: spec.start_position().column,
        end_line: spec.end_position().row + 1,
        visibility: go_visibility(&name),
        children: vec![],
        doc: extract_doc_comment(decl, source),
        body: Some(body),
        signature: Some(signature),
    });
}

/// Extract symbols from a `var_declaration` node.
///
/// Handles both single `var x int` and grouped `var ( x int; y string )` forms.
/// Module-level vars are mapped to `SymbolKind::Static`.
fn extract_var_declaration(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "var_spec" => {
                extract_var_spec(child, node, source, file, symbols);
            }
            "var_spec_list" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "var_spec" {
                        extract_var_spec(inner, node, source, file, symbols);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract a single var from a `var_spec` node.
fn extract_var_spec(
    spec: tree_sitter::Node<'_>,
    decl: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let Some(name) = spec
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(String::from)
    else {
        return;
    };

    let kind = SymbolKind::Static;
    let body = extract_body(source, &spec);
    let signature = extract_signature(source, &spec, kind);
    symbols.push(Symbol {
        name: name.clone(),
        kind,
        file: file.to_path_buf(),
        line: spec.start_position().row + 1,
        column: spec.start_position().column,
        end_line: spec.end_position().row + 1,
        visibility: go_visibility(&name),
        children: vec![],
        doc: extract_doc_comment(decl, source),
        body: Some(body),
        signature: Some(signature),
    });
}

/// Extract doc comments preceding a definition node.
///
/// In Go, doc comments are `//` comments immediately preceding a declaration
/// with no blank lines between them. This looks for `comment` siblings
/// preceding the node.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with("//") {
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

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    /// Helper: parse source and extract symbols for the given file path.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        GoExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture go project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/go_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = GoExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract function → Function
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_function_declaration_as_function() {
        let (_, symbols) = extract_fixture("main.go");
        let greet = symbols
            .iter()
            .find(|s| s.name == "Greet")
            .expect("Greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_go_private_function_is_private() {
        let (_, symbols) = extract_fixture("main.go");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract method with receiver → Method
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_method_with_receiver_as_method() {
        let (_, symbols) = extract_fixture("models.go");
        let full_name = symbols
            .iter()
            .find(|s| s.name == "FullName")
            .expect("FullName not found");
        assert_eq!(full_name.kind, SymbolKind::Method);
        assert_eq!(full_name.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_go_value_receiver_method() {
        let (_, symbols) = extract_fixture("models.go");
        let get_age = symbols
            .iter()
            .find(|s| s.name == "GetAge")
            .expect("GetAge not found");
        assert_eq!(get_age.kind, SymbolKind::Method);
        assert_eq!(get_age.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract struct type → Struct
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_struct_type_as_struct() {
        let (_, symbols) = extract_fixture("models.go");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.kind, SymbolKind::Struct);
        assert_eq!(user.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_go_private_struct() {
        let (_, symbols) = extract_fixture("models.go");
        let cfg = symbols
            .iter()
            .find(|s| s.name == "config")
            .expect("config not found");
        assert_eq!(cfg.kind, SymbolKind::Struct);
        assert_eq!(cfg.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract interface type → Interface
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_interface_type_as_interface() {
        let (_, symbols) = extract_fixture("models.go");
        let stringer = symbols
            .iter()
            .find(|s| s.name == "Stringer")
            .expect("Stringer not found");
        assert_eq!(stringer.kind, SymbolKind::Interface);
        assert_eq!(stringer.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_go_private_interface() {
        let (_, symbols) = extract_fixture("models.go");
        let validator = symbols
            .iter()
            .find(|s| s.name == "validator")
            .expect("validator not found");
        assert_eq!(validator.kind, SymbolKind::Interface);
        assert_eq!(validator.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Uppercase = Public, lowercase = Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_visibility_by_capitalization() {
        let (_, symbols) = extract_fixture("main.go");

        let greet = symbols
            .iter()
            .find(|s| s.name == "Greet")
            .expect("Greet not found");
        assert_eq!(greet.visibility, Visibility::Public);

        let helper = symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("helper not found");
        assert_eq!(helper.visibility, Visibility::Private);

        let max = symbols
            .iter()
            .find(|s| s.name == "MaxRetries")
            .expect("MaxRetries not found");
        assert_eq!(max.visibility, Visibility::Public);

        let min = symbols
            .iter()
            .find(|s| s.name == "minRetries")
            .expect("minRetries not found");
        assert_eq!(min.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Extract const and var declarations
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_const_declaration() {
        let (_, symbols) = extract_fixture("main.go");
        let max = symbols
            .iter()
            .find(|s| s.name == "MaxRetries")
            .expect("MaxRetries not found");
        assert_eq!(max.kind, SymbolKind::Const);
    }

    #[test]
    fn test_extract_go_var_declaration_as_static() {
        let (_, symbols) = extract_fixture("main.go");
        let counter = symbols
            .iter()
            .find(|s| s.name == "GlobalCounter")
            .expect("GlobalCounter not found");
        assert_eq!(counter.kind, SymbolKind::Static);
        assert_eq!(counter.visibility, Visibility::Public);

        let flag = symbols
            .iter()
            .find(|s| s.name == "localFlag")
            .expect("localFlag not found");
        assert_eq!(flag.kind, SymbolKind::Static);
        assert_eq!(flag.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Extract Test* functions as Test kind
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_test_functions_as_test_kind() {
        let (_, symbols) = extract_fixture("main_test.go");
        let test_greet = symbols
            .iter()
            .find(|s| s.name == "TestGreet")
            .expect("TestGreet not found");
        assert_eq!(test_greet.kind, SymbolKind::Test);

        let test_helper = symbols
            .iter()
            .find(|s| s.name == "TestHelper")
            .expect("TestHelper not found");
        assert_eq!(test_helper.kind, SymbolKind::Test);
    }

    #[test]
    fn test_extract_go_benchmark_not_test_kind() {
        let (_, symbols) = extract_fixture("main_test.go");
        let bench = symbols
            .iter()
            .find(|s| s.name == "BenchmarkGreet")
            .expect("BenchmarkGreet not found");
        // Benchmarks are not Test* prefixed in the Test sense
        assert_eq!(bench.kind, SymbolKind::Function);
    }

    // -----------------------------------------------------------------------
    // Scenario 8: Body and signature extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_function_body_contains_source() {
        let (_, symbols) = extract_fixture("main.go");
        let greet = symbols
            .iter()
            .find(|s| s.name == "Greet")
            .expect("Greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("func Greet(name string) string"));
        assert!(body.contains("Sprintf"));
        assert!(body.ends_with('}'));
    }

    #[test]
    fn test_extract_go_function_signature_no_body() {
        let (_, symbols) = extract_fixture("main.go");
        let greet = symbols
            .iter()
            .find(|s| s.name == "Greet")
            .expect("Greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "func Greet(name string) string");
    }

    #[test]
    fn test_extract_go_method_signature_includes_receiver() {
        let (_, symbols) = extract_fixture("models.go");
        let full_name = symbols
            .iter()
            .find(|s| s.name == "FullName")
            .expect("FullName not found");
        let sig = full_name
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "func (u *User) FullName() string");
    }

    #[test]
    fn test_extract_go_struct_body_includes_fields() {
        let (_, symbols) = extract_fixture("models.go");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let body = user.body.as_deref().expect("body should be Some");
        assert!(body.contains("Name string"));
        assert!(body.contains("Age  int"));
    }

    #[test]
    fn test_extract_go_const_signature() {
        let (_, symbols) = extract_fixture("main.go");
        let max = symbols
            .iter()
            .find(|s| s.name == "MaxRetries")
            .expect("MaxRetries not found");
        let sig = max.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "MaxRetries = 3");
    }

    #[test]
    fn test_extract_go_type_alias() {
        let (_, symbols) = extract_fixture("main.go");
        let userid = symbols
            .iter()
            .find(|s| s.name == "UserID")
            .expect("UserID not found");
        assert_eq!(userid.kind, SymbolKind::Type);
        assert_eq!(userid.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 9: extract_symbols dispatches correctly for Go
    // (Tested in extract.rs — but verify GoExtractor integration here too)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["main.go", "models.go", "utils.go", "main_test.go"] {
            let (_, symbols) = extract_fixture(fixture);
            assert!(
                !symbols.is_empty(),
                "expected symbols in {fixture}, got none"
            );
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
            }
        }
    }

    // -----------------------------------------------------------------------
    // Additional: doc comments
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_doc_comment() {
        let (_, symbols) = extract_fixture("main.go");
        let greet = symbols
            .iter()
            .find(|s| s.name == "Greet")
            .expect("Greet not found");
        assert_eq!(
            greet.doc.as_deref(),
            Some("// Greet returns a greeting for the given name.")
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_go_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.go");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_go_package_only_returns_empty_vec() {
        let symbols = parse_and_extract("package main\n", "pkg.go");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_go_broken_source_no_panic() {
        let source = "package main\nfunc good() {}\nfunc broken( {}\ntype S struct {}\n";
        let symbols = parse_and_extract(source, "broken.go");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_go_line_numbers_are_1_based() {
        let source = "package main\n\nfunc first() {}\nfunc second() {}\n";
        let symbols = parse_and_extract(source, "test.go");
        let first = symbols
            .iter()
            .find(|s| s.name == "first")
            .expect("first not found");
        assert_eq!(first.line, 3);
        assert_eq!(first.column, 0);
        let second = symbols
            .iter()
            .find(|s| s.name == "second")
            .expect("second not found");
        assert_eq!(second.line, 4);
    }
}
