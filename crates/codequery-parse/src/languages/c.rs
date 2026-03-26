//! C-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, structs,
//! enums, typedefs, file-level declarations (variables), and macros.
//! All C symbols have `Visibility::Public` because C has no visibility modifiers.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// C language extractor.
pub struct CExtractor;

impl LanguageExtractor for CExtractor {
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let root = tree.root_node();
        let mut symbols = Vec::new();
        walk_children(&mut symbols, root, source, file);
        symbols
    }
}

/// Walk children of a node, descending into preprocessor blocks.
///
/// Preprocessor conditionals (`#ifdef`, `#ifndef`, `#if`, `#elif`) wrap
/// their content as children. We descend into them to find declarations.
fn walk_children(
    symbols: &mut Vec<Symbol>,
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_error() || child.is_missing() {
            continue;
        }
        // Descend into preprocessor conditional blocks
        match child.kind() {
            "preproc_ifdef" | "preproc_if" | "preproc_elif" => {
                walk_children(symbols, child, source, file);
            }
            _ => {
                if let Some(sym) = extract_top_level(child, source, file) {
                    symbols.push(sym);
                }
            }
        }
    }
}

/// Extract the full source body of a symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the type signature of a C symbol.
///
/// The signature varies by symbol kind:
/// - **Function**: declaration line up to the opening `{`, trimmed
/// - **Struct/Enum**: the full body (header + field/variant list)
/// - **Type**: the full typedef line
/// - **Static (variable)**: the full declaration line
/// - **Const (macro)**: the `#define` line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function => extract_fn_signature(body_text),
        SymbolKind::Type | SymbolKind::Static | SymbolKind::Const => {
            extract_single_line_signature(body_text)
        }
        _ => body_text.to_string(),
    }
}

/// Extract function signature: everything before the opening `{`, trimmed.
fn extract_fn_signature(body: &str) -> String {
    if let Some(brace_pos) = body.find('{') {
        body[..brace_pos].trim().to_string()
    } else {
        // Declaration without body (e.g., in a header)
        body.trim().trim_end_matches(';').trim().to_string()
    }
}

/// Extract a single-line signature for typedefs, variables, and macros.
fn extract_single_line_signature(body: &str) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

/// Extract a top-level symbol from a C AST node.
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let kind_str = node.kind();
    match kind_str {
        "function_definition" => {
            let name = extract_function_name(node, source)?;
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "struct_specifier" => {
            // Only extract structs with a body (definition, not forward declaration)
            let name = node_field_text(node, "name", source)?;
            node.child_by_field_name("body")?;
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "enum_specifier" => {
            let name = node_field_text(node, "name", source)?;
            node.child_by_field_name("body")?;
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "type_definition" => {
            let name = extract_typedef_name(node, source)?;
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "declaration" => {
            let name = extract_declaration_name(node, source)?;
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        "preproc_def" | "preproc_function_def" => {
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
                visibility: Visibility::Public,
                children: vec![],
                doc: extract_doc_comment(node, source),
                body: Some(body),
                signature: Some(signature),
            })
        }
        _ => None,
    }
}

/// Extract the function name from a `function_definition` node.
///
/// The `function_definition` node has a `declarator` field. For simple
/// return types this is a `function_declarator`. For pointer return types
/// (e.g. `const char*`) it is a `pointer_declarator` wrapping the
/// `function_declarator`. We unwrap through pointer declarators until
/// we reach the `function_declarator`, then extract the name identifier.
fn extract_function_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut declarator = node.child_by_field_name("declarator")?;
    // Unwrap pointer_declarator layers (e.g. `char* func(...)`)
    while declarator.kind() == "pointer_declarator" {
        declarator = declarator.child_by_field_name("declarator")?;
    }
    // Now declarator should be function_declarator
    let name_node = declarator.child_by_field_name("declarator")?;
    // The name node itself could be a pointer_declarator in rare cases
    extract_innermost_identifier(name_node, source)
}

/// Extract the typedef name from a `type_definition` node.
///
/// The `declarator` field can be `type_identifier`, `primitive_type`,
/// or `function_declarator` (for function pointer typedefs).
fn extract_typedef_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    if declarator.kind() == "function_declarator" {
        // Function pointer typedef: `typedef int (*name)(args)`
        // The declarator field of function_declarator is the parenthesized_declarator
        let inner = declarator.child_by_field_name("declarator")?;
        let text = inner.utf8_text(source.as_bytes()).ok()?;
        // Strip parens and pointer: (*name) -> name
        let name = text
            .trim_start_matches('(')
            .trim_start_matches('*')
            .trim_end_matches(')');
        Some(name.to_string())
    } else {
        declarator
            .utf8_text(source.as_bytes())
            .ok()
            .map(String::from)
    }
}

/// Extract the variable name from a file-level `declaration` node.
///
/// Declarations have an `init_declarator` as the declarator field,
/// which itself has a `declarator` field containing the identifier.
fn extract_declaration_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    match declarator.kind() {
        "init_declarator" => {
            let name_node = declarator.child_by_field_name("declarator")?;
            // Handle pointer declarators: `* name` -> get the inner identifier
            extract_innermost_identifier(name_node, source)
        }
        "identifier" => declarator
            .utf8_text(source.as_bytes())
            .ok()
            .map(String::from),
        _ => extract_innermost_identifier(declarator, source),
    }
}

/// Recursively find the innermost identifier in a declarator chain.
///
/// Handles pointer declarators (`*name`) by following the `declarator` field.
fn extract_innermost_identifier(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if node.kind() == "pointer_declarator" {
        let inner = node.child_by_field_name("declarator")?;
        extract_innermost_identifier(inner, source)
    } else {
        node.utf8_text(source.as_bytes()).ok().map(String::from)
    }
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract doc comments preceding a definition node.
///
/// Looks for `comment` siblings immediately before the node.
/// Both `/* ... */` and `// ...` comments are collected.
fn extract_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "comment" {
            if let Ok(text) = sib.utf8_text(source.as_bytes()) {
                doc_lines.push(text.trim_end().to_string());
                sibling = sib.prev_sibling();
                continue;
            }
            break;
        }
        break;
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
    use crate::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    /// Helper: parse C source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        CExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the C fixture project.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/c_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(filename: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(filename);
        let mut parser = Parser::for_language(Language::C).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = CExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Extract function -> Function/Public
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_function_as_function_public() {
        let source = "int add(int a, int b) {\n    return a + b;\n}\n";
        let symbols = parse_and_extract(source, "add.c");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_c_function_from_fixture() {
        let (_, symbols) = extract_fixture("utils.c");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);

        let multiply = symbols
            .iter()
            .find(|s| s.name == "multiply")
            .expect("multiply not found");
        assert_eq!(multiply.kind, SymbolKind::Function);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Extract struct -> Struct
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_struct_from_fixture() {
        let (_, symbols) = extract_fixture("main.c");
        let config = symbols
            .iter()
            .find(|s| s.name == "Config")
            .expect("Config not found");
        assert_eq!(config.kind, SymbolKind::Struct);
        assert_eq!(config.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_c_struct_inline() {
        let source = "struct Point {\n    int x;\n    int y;\n};\n";
        let symbols = parse_and_extract(source, "test.c");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        assert_eq!(point.kind, SymbolKind::Struct);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Extract enum -> Enum
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_enum_from_fixture() {
        let (_, symbols) = extract_fixture("main.c");
        let log_level = symbols
            .iter()
            .find(|s| s.name == "LogLevel")
            .expect("LogLevel not found");
        assert_eq!(log_level.kind, SymbolKind::Enum);
        assert_eq!(log_level.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Extract typedef -> Type
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_typedef_from_fixture() {
        let (_, symbols) = extract_fixture("main.c");
        let alias = symbols
            .iter()
            .find(|s| s.name == "size_t_alias")
            .expect("size_t_alias not found");
        assert_eq!(alias.kind, SymbolKind::Type);
        assert_eq!(alias.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_c_typedef_function_pointer_inline() {
        let source = "typedef int (*operation_fn)(int, int);\n";
        let symbols = parse_and_extract(source, "test.h");
        let callback = symbols
            .iter()
            .find(|s| s.name == "operation_fn")
            .expect("operation_fn not found");
        assert_eq!(callback.kind, SymbolKind::Type);
    }

    #[test]
    fn test_extract_c_typedef_function_pointer_fixture() {
        let (_, symbols) = extract_fixture("utils.h");
        let callback = symbols
            .iter()
            .find(|s| s.name == "operation_fn")
            .expect("operation_fn not found in utils.h");
        assert_eq!(callback.kind, SymbolKind::Type);
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Extract macro -> Const
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_macro_as_const() {
        let (_, symbols) = extract_fixture("main.c");
        let max_buf = symbols
            .iter()
            .find(|s| s.name == "MAX_BUFFER_SIZE")
            .expect("MAX_BUFFER_SIZE not found");
        assert_eq!(max_buf.kind, SymbolKind::Const);
        assert_eq!(max_buf.visibility, Visibility::Public);

        let square = symbols
            .iter()
            .find(|s| s.name == "SQUARE")
            .expect("SQUARE not found");
        assert_eq!(square.kind, SymbolKind::Const);
    }

    // -----------------------------------------------------------------------
    // Scenario 9a: Body and signature for C
    // -----------------------------------------------------------------------
    #[test]
    fn test_c_function_body_and_signature() {
        let source = "int add(int a, int b) {\n    return a + b;\n}\n";
        let symbols = parse_and_extract(source, "test.c");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");

        let body = add.body.as_deref().expect("body should be Some");
        assert!(body.contains("return a + b;"));
        assert!(body.starts_with("int add(int a, int b)"));

        let sig = add.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "int add(int a, int b)");
    }

    #[test]
    fn test_c_struct_body_includes_fields() {
        let source = "struct Point {\n    int x;\n    int y;\n};\n";
        let symbols = parse_and_extract(source, "test.c");
        let point = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point not found");
        let body = point.body.as_deref().expect("body should be Some");
        assert!(body.contains("int x;"));
        assert!(body.contains("int y;"));
    }

    #[test]
    fn test_c_typedef_signature() {
        let source = "typedef unsigned long size_t;\n";
        let symbols = parse_and_extract(source, "test.c");
        let st = symbols
            .iter()
            .find(|s| s.name == "size_t")
            .expect("size_t not found");
        let sig = st.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "typedef unsigned long size_t");
    }

    #[test]
    fn test_c_macro_signature() {
        let source = "#define MAX_SIZE 1024\n";
        let symbols = parse_and_extract(source, "test.c");
        let max_size = symbols
            .iter()
            .find(|s| s.name == "MAX_SIZE")
            .expect("MAX_SIZE not found");
        let sig = max_size.signature.as_deref().expect("sig should be Some");
        assert_eq!(sig, "#define MAX_SIZE 1024");
    }

    // -----------------------------------------------------------------------
    // File-level declaration -> Static
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_file_level_variable_as_static() {
        let (_, symbols) = extract_fixture("main.c");
        let counter = symbols
            .iter()
            .find(|s| s.name == "global_counter")
            .expect("global_counter not found");
        assert_eq!(counter.kind, SymbolKind::Static);
        assert_eq!(counter.visibility, Visibility::Public);
    }

    // -----------------------------------------------------------------------
    // Doc comments
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_doc_comment() {
        let source = "/* Add two numbers. */\nint add(int a, int b) {\n    return a + b;\n}\n";
        let symbols = parse_and_extract(source, "test.c");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");
        assert_eq!(add.doc.as_deref(), Some("/* Add two numbers. */"));
    }

    // -----------------------------------------------------------------------
    // Empty/broken source
    // -----------------------------------------------------------------------
    #[test]
    fn test_extract_c_empty_source_returns_empty() {
        let symbols = parse_and_extract("", "empty.c");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_c_broken_source_partial_results() {
        let source = "int good(int a) { return a; }\nint broken( { }\nstruct S { int x; };\n";
        let symbols = parse_and_extract(source, "broken.c");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    // -----------------------------------------------------------------------
    // All fixture symbols have body and signature
    // -----------------------------------------------------------------------
    #[test]
    fn test_all_c_fixture_symbols_have_body_and_signature() {
        for fixture in &["main.c", "utils.c", "utils.h"] {
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
            }
        }
    }
}
