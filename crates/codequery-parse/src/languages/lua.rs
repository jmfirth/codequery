//! Lua-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions (both local
//! and global), and table-assigned module functions (e.g., `function M.foo()`).
//! All symbols are Public by default; `local` declarations are Private.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Lua language extractor.
pub struct LuaExtractor;

impl LanguageExtractor for LuaExtractor {
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

/// Extract the type signature of a Lua symbol.
///
/// - **Function**: `function name(params)` or `local function name(params)`
/// - **Other**: first line of the declaration
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, _kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    extract_fn_signature(body_text)
}

/// Extract function signature: the first line of the definition.
///
/// For Lua, the signature is the `function name(params)` or
/// `local function name(params)` line.
fn extract_fn_signature(body: &str) -> String {
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Extract top-level symbols from a node, appending to `symbols`.
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            let is_local = has_local_child(node);
            let visibility = if is_local {
                Visibility::Private
            } else {
                Visibility::Public
            };

            let name = extract_function_name(node, source);
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
            // Check for `local M = {}` table assignments (module pattern)
            // We only extract these if they are table constructors
            extract_variable_symbol(node, source, file, symbols);
        }
        _ => {}
    }
}

/// Extract the function name from a `function_declaration` node.
///
/// Handles both simple names (`function foo()`) and dot-indexed names
/// (`function M.foo()`).
fn extract_function_name(node: tree_sitter::Node<'_>, source: &str) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        match name_node.kind() {
            // Both identifier and dot_index_expression (e.g., M.greet)
            // produce the full qualified name from their text content
            "identifier" | "dot_index_expression" => {
                return name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
            }
            _ => {}
        }
    }
    "anonymous".to_string()
}

/// Extract symbols from a local variable declaration.
///
/// Only extracts `local M = {}` table constructor assignments as Module symbols.
fn extract_variable_symbol(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    // Look for assignment_statement child with a table_constructor value
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "assignment_statement" {
            let (name, has_table) = parse_assignment_statement(child, source);
            if has_table {
                if let Some(name) = name {
                    if !name.is_empty() {
                        let body = extract_body(source, &node);
                        let signature = body.lines().next().unwrap_or("").trim_end().to_string();
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Module,
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
                }
            }
        }
    }
}

/// Parse an assignment statement to extract the variable name and whether
/// the value is a table constructor.
fn parse_assignment_statement(node: tree_sitter::Node<'_>, source: &str) -> (Option<String>, bool) {
    let mut name = None;
    let mut has_table = false;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "variable_list" => {
                // Get the identifier from variable_list
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "identifier" {
                        name = inner.utf8_text(source.as_bytes()).ok().map(String::from);
                    }
                }
            }
            "expression_list" => {
                // Check for table_constructor in expression_list
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "table_constructor" {
                        has_table = true;
                    }
                }
            }
            _ => {}
        }
    }

    (name, has_table)
}

/// Check whether a node has a `local` keyword child.
fn has_local_child(node: tree_sitter::Node<'_>) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "local" {
            return true;
        }
    }
    false
}

/// Extract doc comments preceding a definition node.
///
/// In Lua, comments use `--` syntax. Doc comments are consecutive `--` comments
/// immediately preceding the definition.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                let trimmed = text.trim_end();
                if trimmed.starts_with("--") {
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
        let mut parser = Parser::for_language(Language::Lua).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        LuaExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture lua project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/lua_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Lua).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = LuaExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract global function as Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_global_function_as_public() {
        let (_, symbols) = extract_fixture("main.lua");
        let global_fn = symbols
            .iter()
            .find(|s| s.name == "global_fn")
            .expect("global_fn not found");
        assert_eq!(global_fn.kind, SymbolKind::Function);
        assert_eq!(global_fn.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract local function as Function/Private
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_local_function_as_private() {
        let (_, symbols) = extract_fixture("main.lua");
        let private_helper = symbols
            .iter()
            .find(|s| s.name == "private_helper")
            .expect("private_helper not found");
        assert_eq!(private_helper.kind, SymbolKind::Function);
        assert_eq!(private_helper.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract module function (M.greet) as Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_module_function_as_public() {
        let (_, symbols) = extract_fixture("main.lua");
        let greet = symbols
            .iter()
            .find(|s| s.name == "M.greet")
            .expect("M.greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract local table assignment as Module
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_local_table_as_module() {
        let (_, symbols) = extract_fixture("main.lua");
        let m = symbols
            .iter()
            .find(|s| s.name == "M" && s.kind == SymbolKind::Module)
            .expect("M module not found");
        assert_eq!(m.visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Body and signature extraction
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_function_body_contains_source() {
        let (_, symbols) = extract_fixture("main.lua");
        let greet = symbols
            .iter()
            .find(|s| s.name == "M.greet")
            .expect("M.greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("function M.greet"));
        assert!(body.ends_with("end"));
    }

    #[test]
    fn test_extract_lua_function_signature() {
        let (_, symbols) = extract_fixture("main.lua");
        let greet = symbols
            .iter()
            .find(|s| s.name == "M.greet")
            .expect("M.greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "function M.greet(name)");
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Doc comment extracted
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_doc_comment() {
        let (_, symbols) = extract_fixture("main.lua");
        let greet = symbols
            .iter()
            .find(|s| s.name == "M.greet")
            .expect("M.greet not found");
        assert_eq!(greet.doc.as_deref(), Some("-- Greet a person by name."));
    }

    // -----------------------------------------------------------------------
    // Scenario 7: All symbols have body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_all_fixture_symbols_have_body_and_signature() {
        for fixture in &["main.lua", "utils.lua"] {
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
    // Scenario 8: require extraction (tested in utils.lua)
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_utils_functions() {
        let (_, symbols) = extract_fixture("utils.lua");
        let format_name = symbols
            .iter()
            .find(|s| s.name == "format_name")
            .expect("format_name not found");
        assert_eq!(format_name.kind, SymbolKind::Function);
        assert_eq!(format_name.visibility, Visibility::Private);

        let add = symbols
            .iter()
            .find(|s| s.name == "utils.add")
            .expect("utils.add not found");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_lua_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.lua");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_lua_broken_source_no_panic() {
        let source = "function good()\n  return 1\nend\nfunction broken(\nend\n";
        let symbols = parse_and_extract(source, "broken.lua");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_extract_lua_line_numbers_are_1_based() {
        let source = "function first()\nend\nfunction second()\nend\n";
        let symbols = parse_and_extract(source, "test.lua");
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
        assert_eq!(second.line, 3);
    }
}
