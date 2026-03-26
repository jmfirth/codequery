//! Python-specific symbol extraction from tree-sitter ASTs.
//!
//! Walks the AST and extracts all symbol definitions — functions, classes,
//! methods, constants (`ALL_CAPS` module-level assignments), and test functions
//! (names starting with `test_`). Also handles decorated definitions.

use std::path::Path;

use codequery_core::{Symbol, SymbolKind, Visibility};

use super::LanguageExtractor;

/// Python language extractor.
pub struct PythonExtractor;

impl LanguageExtractor for PythonExtractor {
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

/// Extract the full source body of a Python symbol's AST node.
#[must_use]
pub fn extract_body(source: &str, node: &tree_sitter::Node<'_>) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

/// Extract the signature of a Python symbol.
///
/// - **Function/Method/Test**: first line of the definition (`def name(params) -> ret:`)
/// - **Class**: header line (`class Name(bases):`)
/// - **Const**: the full assignment line
#[must_use]
pub fn extract_signature(source: &str, node: &tree_sitter::Node<'_>, kind: SymbolKind) -> String {
    let body_text = &source[node.start_byte()..node.end_byte()];
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Test => {
            extract_def_signature(body_text)
        }
        SymbolKind::Class => extract_class_signature(body_text),
        SymbolKind::Const => extract_const_signature(body_text),
        _ => body_text.lines().next().unwrap_or("").to_string(),
    }
}

/// Extract function/method signature: the `def` line up to and including the colon.
fn extract_def_signature(body: &str) -> String {
    // Find the first line that starts with "def " — handles decorated definitions
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("def ") {
            return trimmed.trim_end().to_string();
        }
    }
    // Fallback: first line
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Extract class signature: the `class` line up to and including the colon.
fn extract_class_signature(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("class ") {
            return trimmed.trim_end().to_string();
        }
    }
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Extract constant signature: the full assignment line.
fn extract_const_signature(body: &str) -> String {
    body.lines().next().unwrap_or("").trim_end().to_string()
}

/// Determine Python visibility by name convention.
///
/// - Names starting with `_` (including `__` dunders) are private
/// - All others are public
fn python_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// Determine if a function name indicates a test function.
fn is_test_function(name: &str) -> bool {
    name.starts_with("test_")
}

/// Extract a top-level symbol from a node and push it into the symbols vec.
///
/// This handles `function_definition`, `class_definition`, `decorated_definition`,
/// and `expression_statement` (for module-level constant assignments).
#[allow(clippy::too_many_lines)]
// All node-type match arms for top-level extraction; splitting would obscure the logic
fn extract_top_level(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let kind_str = node.kind();
    match kind_str {
        "function_definition" => {
            if let Some(sym) = extract_function(node, source, file, false) {
                symbols.push(sym);
            }
        }
        "class_definition" => {
            if let Some(sym) = extract_class(node, source, file) {
                symbols.push(sym);
            }
        }
        "decorated_definition" => {
            extract_decorated(node, source, file, symbols);
        }
        "expression_statement" => {
            if let Some(sym) = extract_module_constant(node, source, file) {
                symbols.push(sym);
            }
        }
        _ => {}
    }
}

/// Extract a function definition as a symbol.
///
/// When `is_method` is true, the symbol kind is `Method` (unless it's a test).
fn extract_function(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    is_method: bool,
) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let kind = if is_test_function(&name) {
        SymbolKind::Test
    } else if is_method {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, kind);
    Some(Symbol {
        name: name.clone(),
        kind,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: python_visibility(&name),
        children: vec![],
        doc: extract_docstring(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a class definition as a symbol, including its method children.
fn extract_class(node: tree_sitter::Node<'_>, source: &str, file: &Path) -> Option<Symbol> {
    let name = node_field_text(node, "name", source)?;
    let children = extract_class_methods(node, source, file);
    let body = extract_body(source, &node);
    let signature = extract_signature(source, &node, SymbolKind::Class);
    Some(Symbol {
        name: name.clone(),
        kind: SymbolKind::Class,
        file: file.to_path_buf(),
        line: node.start_position().row + 1,
        column: node.start_position().column,
        end_line: node.end_position().row + 1,
        visibility: python_visibility(&name),
        children,
        doc: extract_docstring(node, source),
        body: Some(body),
        signature: Some(signature),
    })
}

/// Extract a decorated definition — unwrap the inner function or class.
///
/// The decorator node wraps the actual definition. We extract the inner
/// definition but use the outer (decorated) node for body/line range
/// so decorators are included.
fn extract_decorated(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    // Find the inner definition (last child that is function_definition or class_definition)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = node_field_text(child, "name", source) {
                    let kind = if is_test_function(&name) {
                        SymbolKind::Test
                    } else {
                        SymbolKind::Function
                    };
                    // Use the outer decorated node for body/range to include decorators
                    let body = extract_body(source, &node);
                    let signature = extract_signature(source, &child, kind);
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind,
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                        column: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        visibility: python_visibility(&name),
                        children: vec![],
                        doc: extract_docstring(child, source),
                        body: Some(body),
                        signature: Some(signature),
                    });
                }
            }
            "class_definition" => {
                if let Some(name) = node_field_text(child, "name", source) {
                    let children = extract_class_methods(child, source, file);
                    let body = extract_body(source, &node);
                    let signature = extract_signature(source, &child, SymbolKind::Class);
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Class,
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                        column: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        visibility: python_visibility(&name),
                        children,
                        doc: extract_docstring(child, source),
                        body: Some(body),
                        signature: Some(signature),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Extract methods from a class definition body.
///
/// Handles both plain `function_definition` and `decorated_definition` children
/// within the class body block.
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
        match child.kind() {
            "function_definition" => {
                if let Some(sym) = extract_function(child, source, file, true) {
                    methods.push(sym);
                }
            }
            "decorated_definition" => {
                extract_decorated_method(child, source, file, &mut methods);
            }
            _ => {}
        }
    }

    methods
}

/// Extract a decorated method inside a class body.
///
/// Similar to `extract_decorated` but marks the inner function as a Method.
fn extract_decorated_method(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    methods: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_definition" {
            if let Some(name) = node_field_text(child, "name", source) {
                let kind = SymbolKind::Method;
                // Use outer decorated node for body/range to include decorators
                let body = extract_body(source, &node);
                let signature = extract_signature(source, &child, kind);
                methods.push(Symbol {
                    name: name.clone(),
                    kind,
                    file: file.to_path_buf(),
                    line: node.start_position().row + 1,
                    column: node.start_position().column,
                    end_line: node.end_position().row + 1,
                    visibility: python_visibility(&name),
                    children: vec![],
                    doc: extract_docstring(child, source),
                    body: Some(body),
                    signature: Some(signature),
                });
            }
        }
    }
}

/// Extract a module-level constant assignment (`ALL_CAPS = value`).
///
/// Only extracts assignments where the left side is a single identifier
/// in `SCREAMING_SNAKE_CASE` (all uppercase letters and underscores).
fn extract_module_constant(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
) -> Option<Symbol> {
    // expression_statement -> assignment -> (left: identifier, right: expression)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "assignment" {
            let left = child.child_by_field_name("left")?;
            if left.kind() != "identifier" {
                return None;
            }
            let name = left.utf8_text(source.as_bytes()).ok()?.to_string();
            if !is_screaming_snake_case(&name) {
                return None;
            }
            let body = extract_body(source, &node);
            let signature = extract_signature(source, &node, SymbolKind::Const);
            return Some(Symbol {
                name: name.clone(),
                kind: SymbolKind::Const,
                file: file.to_path_buf(),
                line: node.start_position().row + 1,
                column: node.start_position().column,
                end_line: node.end_position().row + 1,
                visibility: python_visibility(&name),
                children: vec![],
                doc: None,
                body: Some(body),
                signature: Some(signature),
            });
        }
    }
    None
}

/// Check if a name is in `SCREAMING_SNAKE_CASE` (all uppercase + underscores, at least one letter).
fn is_screaming_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.chars().any(|c| c.is_ascii_uppercase())
}

/// Get the text of a named field on a node.
fn node_field_text(node: tree_sitter::Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    child.utf8_text(source.as_bytes()).ok().map(String::from)
}

/// Extract a docstring from a function or class definition.
///
/// In Python, a docstring is the first statement in the body if it is
/// an `expression_statement` containing a single `string` node.
fn extract_docstring(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let body = node.child_by_field_name("body")?;

    // The body is a `block` node. Its first non-comment child may be the docstring.
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if child.kind() == "expression_statement" {
            // Check if the expression statement contains a single string
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "string" || inner.kind() == "concatenated_string" {
                    return inner.utf8_text(source.as_bytes()).ok().map(String::from);
                }
            }
        }
        // First non-comment statement is not a string expression — no docstring
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use codequery_core::Language;
    use std::path::PathBuf;

    /// Helper: parse Python source and extract symbols.
    fn parse_and_extract(source: &str, file: &str) -> Vec<Symbol> {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        PythonExtractor::extract_symbols(source, &tree, Path::new(file))
    }

    /// Helper: path to the fixture python project directory.
    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_project")
    }

    /// Helper: parse a fixture file and extract symbols.
    fn extract_fixture(relative_path: &str) -> (String, Vec<Symbol>) {
        let path = fixture_dir().join(relative_path);
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let (source, tree) = parser.parse_file(&path).unwrap();
        let symbols = PythonExtractor::extract_symbols(&source, &tree, &path);
        (source, symbols)
    }

    // =======================================================================
    // Test Scenario 1: Extract function -> Function/Public
    // =======================================================================
    #[test]
    fn test_extract_function_public() {
        let (_, symbols) = extract_fixture("src/main.py");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert_eq!(greet.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_function_add_public() {
        let (_, symbols) = extract_fixture("src/main.py");
        let add = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add not found");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);
    }

    // =======================================================================
    // Test Scenario 2: Extract class with methods -> Class with Method children
    // =======================================================================
    #[test]
    fn test_extract_class_with_methods() {
        let (_, symbols) = extract_fixture("src/models.py");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        assert_eq!(user.kind, SymbolKind::Class);
        assert_eq!(user.visibility, Visibility::Public);
        assert!(
            !user.children.is_empty(),
            "User should have method children"
        );

        let method_names: Vec<&str> = user.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"__init__"));
        assert!(method_names.contains(&"is_adult"));
        assert!(method_names.contains(&"_internal_check"));

        // All children should be Method kind
        for child in &user.children {
            assert_eq!(
                child.kind,
                SymbolKind::Method,
                "child {} should be Method",
                child.name
            );
        }
    }

    #[test]
    fn test_extract_class_with_inheritance() {
        let (_, symbols) = extract_fixture("src/models.py");
        let admin = symbols
            .iter()
            .find(|s| s.name == "Admin")
            .expect("Admin not found");
        assert_eq!(admin.kind, SymbolKind::Class);
        assert_eq!(admin.visibility, Visibility::Public);
        let method_names: Vec<&str> = admin.children.iter().map(|c| c.name.as_str()).collect();
        assert!(method_names.contains(&"__init__"));
        assert!(method_names.contains(&"promote"));
    }

    // =======================================================================
    // Test Scenario 3: Extract decorated function (e.g., @staticmethod)
    // =======================================================================
    #[test]
    fn test_extract_decorated_method_staticmethod() {
        let (_, symbols) = extract_fixture("src/models.py");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let create = user
            .children
            .iter()
            .find(|c| c.name == "create")
            .expect("create not found in User methods");
        assert_eq!(create.kind, SymbolKind::Method);
        // Body should include the @staticmethod decorator
        let body = create.body.as_deref().expect("body should be Some");
        assert!(
            body.contains("@staticmethod"),
            "body should include decorator"
        );
    }

    #[test]
    fn test_extract_decorated_method_classmethod() {
        let (_, symbols) = extract_fixture("src/models.py");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let from_dict = user
            .children
            .iter()
            .find(|c| c.name == "from_dict")
            .expect("from_dict not found in User methods");
        assert_eq!(from_dict.kind, SymbolKind::Method);
        let body = from_dict.body.as_deref().expect("body should be Some");
        assert!(
            body.contains("@classmethod"),
            "body should include decorator"
        );
    }

    // =======================================================================
    // Test Scenario 4: Extract _private function -> Private visibility
    // =======================================================================
    #[test]
    fn test_extract_private_function_underscore_prefix() {
        let (_, symbols) = extract_fixture("src/main.py");
        let priv_fn = symbols
            .iter()
            .find(|s| s.name == "_private_helper")
            .expect("_private_helper not found");
        assert_eq!(priv_fn.kind, SymbolKind::Function);
        assert_eq!(priv_fn.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_private_function_double_underscore() {
        let (_, symbols) = extract_fixture("src/utils.py");
        let dunder = symbols
            .iter()
            .find(|s| s.name == "__double_private")
            .expect("__double_private not found");
        assert_eq!(dunder.kind, SymbolKind::Function);
        assert_eq!(dunder.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_private_method_in_class() {
        let (_, symbols) = extract_fixture("src/models.py");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let internal = user
            .children
            .iter()
            .find(|c| c.name == "_internal_check")
            .expect("_internal_check not found");
        assert_eq!(internal.kind, SymbolKind::Method);
        assert_eq!(internal.visibility, Visibility::Private);
    }

    // =======================================================================
    // Test Scenario 5: Extract test_ function -> Test kind
    // =======================================================================
    #[test]
    fn test_extract_test_functions_as_test_kind() {
        let (_, symbols) = extract_fixture("tests/test_main.py");
        let test_greet = symbols
            .iter()
            .find(|s| s.name == "test_greet")
            .expect("test_greet not found");
        assert_eq!(test_greet.kind, SymbolKind::Test);

        let test_add = symbols
            .iter()
            .find(|s| s.name == "test_add")
            .expect("test_add not found");
        assert_eq!(test_add.kind, SymbolKind::Test);

        let test_empty = symbols
            .iter()
            .find(|s| s.name == "test_greet_empty")
            .expect("test_greet_empty not found");
        assert_eq!(test_empty.kind, SymbolKind::Test);
    }

    #[test]
    fn test_non_test_function_in_test_file_is_function() {
        let (_, symbols) = extract_fixture("tests/test_main.py");
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper_not_a_test")
            .expect("helper_not_a_test not found");
        assert_eq!(helper.kind, SymbolKind::Function);
    }

    // =======================================================================
    // Test Scenario 6: Extract module-level constant (ALL_CAPS) -> Const
    // =======================================================================
    #[test]
    fn test_extract_module_constant_all_caps() {
        let (_, symbols) = extract_fixture("src/main.py");
        let max_retries = symbols
            .iter()
            .find(|s| s.name == "MAX_RETRIES")
            .expect("MAX_RETRIES not found");
        assert_eq!(max_retries.kind, SymbolKind::Const);
        assert_eq!(max_retries.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_module_constant_default_timeout() {
        let (_, symbols) = extract_fixture("src/main.py");
        let timeout = symbols
            .iter()
            .find(|s| s.name == "DEFAULT_TIMEOUT")
            .expect("DEFAULT_TIMEOUT not found");
        assert_eq!(timeout.kind, SymbolKind::Const);
        assert_eq!(timeout.visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_private_constant_underscore_prefix() {
        let (_, symbols) = extract_fixture("src/main.py");
        let internal = symbols.iter().find(|s| s.name == "_INTERNAL_STATE");
        // _INTERNAL_STATE starts with underscore, so it's private.
        // But it also needs to be ALL_CAPS to be detected as a constant.
        // The underscore prefix doesn't disqualify it from being ALL_CAPS.
        // However, our is_screaming_snake_case requires uppercase letters,
        // and _ is allowed, so _INTERNAL_STATE should match.
        assert!(internal.is_some(), "_INTERNAL_STATE should be extracted");
        let internal = internal.unwrap();
        assert_eq!(internal.kind, SymbolKind::Const);
        assert_eq!(internal.visibility, Visibility::Private);
    }

    // =======================================================================
    // Test Scenario 7: Body and signature extraction
    // =======================================================================
    #[test]
    fn test_body_function_returns_complete_source() {
        let source = "def greet(name: str) -> str:\n    return f\"Hello, {name}!\"\n";
        let symbols = parse_and_extract(source, "test.py");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let body = greet.body.as_deref().expect("body should be Some");
        assert!(body.starts_with("def greet(name: str) -> str:"));
        assert!(body.contains("return f\"Hello, {name}!\""));
    }

    #[test]
    fn test_signature_function_is_def_line() {
        let source = "def greet(name: str) -> str:\n    return f\"Hello, {name}!\"\n";
        let symbols = parse_and_extract(source, "test.py");
        let greet = symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("greet not found");
        let sig = greet
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "def greet(name: str) -> str:");
    }

    #[test]
    fn test_signature_class_is_class_line() {
        let source = "class User:\n    def __init__(self):\n        pass\n";
        let symbols = parse_and_extract(source, "test.py");
        let user = symbols
            .iter()
            .find(|s| s.name == "User")
            .expect("User not found");
        let sig = user.signature.as_deref().expect("signature should be Some");
        assert_eq!(sig, "class User:");
    }

    #[test]
    fn test_signature_class_with_bases() {
        let source = "class Admin(User):\n    pass\n";
        let symbols = parse_and_extract(source, "test.py");
        let admin = symbols
            .iter()
            .find(|s| s.name == "Admin")
            .expect("Admin not found");
        let sig = admin
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "class Admin(User):");
    }

    #[test]
    fn test_signature_const_is_assignment_line() {
        let source = "MAX_SIZE = 100\n";
        let symbols = parse_and_extract(source, "test.py");
        let max_size = symbols
            .iter()
            .find(|s| s.name == "MAX_SIZE")
            .expect("MAX_SIZE not found");
        let sig = max_size
            .signature
            .as_deref()
            .expect("signature should be Some");
        assert_eq!(sig, "MAX_SIZE = 100");
    }

    #[test]
    fn test_body_class_includes_methods() {
        let source = "class Foo:\n    def bar(self):\n        return 42\n";
        let symbols = parse_and_extract(source, "test.py");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        let body = foo.body.as_deref().expect("body should be Some");
        assert!(body.contains("def bar(self):"));
        assert!(body.contains("return 42"));
    }

    // =======================================================================
    // Test Scenario 8: extract_symbols dispatches correctly for Python
    // =======================================================================
    #[test]
    fn test_extract_symbols_dispatch_python() {
        let source = "def foo():\n    pass\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let symbols = crate::extract_symbols(source, &tree, Path::new("test.py"), Language::Python);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    // =======================================================================
    // Additional edge cases
    // =======================================================================
    #[test]
    fn test_extract_empty_source_returns_empty_vec() {
        let symbols = parse_and_extract("", "empty.py");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_broken_source_returns_partial_results() {
        let source = "def good():\n    pass\n\ndef broken(\n\nclass Valid:\n    pass\n";
        let symbols = parse_and_extract(source, "broken.py");
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_methods_not_top_level() {
        let source =
            "class Foo:\n    def bar(self):\n        pass\n    def baz(self):\n        pass\n";
        let symbols = parse_and_extract(source, "test.py");
        // Methods should NOT appear at top level
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
    fn test_extract_line_numbers_are_1_based() {
        let source = "def first():\n    pass\n\ndef second():\n    pass\n";
        let symbols = parse_and_extract(source, "test.py");
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
        assert_eq!(second.line, 4);
    }

    #[test]
    fn test_lowercase_assignment_not_extracted() {
        let source = "my_var = 42\n";
        let symbols = parse_and_extract(source, "test.py");
        assert!(
            symbols.is_empty(),
            "lowercase assignment should not be extracted as symbol"
        );
    }

    #[test]
    fn test_docstring_extraction() {
        let source = "def foo():\n    \"\"\"This is a docstring.\"\"\"\n    pass\n";
        let symbols = parse_and_extract(source, "test.py");
        let foo = symbols
            .iter()
            .find(|s| s.name == "foo")
            .expect("foo not found");
        assert!(foo.doc.is_some(), "docstring should be extracted");
        let doc = foo.doc.as_deref().unwrap();
        assert!(doc.contains("This is a docstring."));
    }

    #[test]
    fn test_class_docstring_extraction() {
        let source =
            "class Foo:\n    \"\"\"A class docstring.\"\"\"\n    def bar(self):\n        pass\n";
        let symbols = parse_and_extract(source, "test.py");
        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Foo not found");
        assert!(foo.doc.is_some(), "class docstring should be extracted");
        let doc = foo.doc.as_deref().unwrap();
        assert!(doc.contains("A class docstring."));
    }

    #[test]
    fn test_all_fixture_symbols_have_body_and_signature() {
        for fixture in &[
            "src/main.py",
            "src/models.py",
            "src/utils.py",
            "tests/test_main.py",
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
}
