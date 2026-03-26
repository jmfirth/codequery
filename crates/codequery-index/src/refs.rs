//! Reference extraction from tree-sitter ASTs.
//!
//! Walks ASTs looking for identifier nodes that are NOT definitions,
//! classifying each as a call, type usage, import, or assignment.
//! Captures file location, source context, and the enclosing caller name.

use std::path::Path;

use codequery_core::{Language, Reference, ReferenceKind, SymbolKind};

/// Extract references (non-definition identifier usages) from a parsed file.
///
/// Walks the AST looking for identifier nodes that are NOT definitions.
/// Each reference is classified by kind (call, type usage, import, assignment)
/// and annotated with the source line and enclosing function name.
///
/// For Rust, uses language-specific node type matching. For other languages,
/// falls back to a generic identifier-based approach that classifies all
/// non-definition identifiers as calls.
#[must_use]
pub fn extract_references(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,
) -> Vec<Reference> {
    let lines: Vec<&str> = source.lines().collect();
    match language {
        Language::Rust => extract_rust_references(source, tree, file, &lines),
        _ => extract_generic_references(source, tree, file, &lines),
    }
}

/// Extract references from a Rust source file.
fn extract_rust_references(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    lines: &[&str],
) -> Vec<Reference> {
    let root = tree.root_node();
    let mut refs = Vec::new();

    walk_rust_node(root, source, file, lines, &mut refs, None);

    refs
}

/// Recursively walk a Rust AST node, extracting references.
///
/// `enclosing_fn` tracks the name of the nearest enclosing function or method
/// so that references can report their caller.
#[allow(clippy::too_many_lines)]
// All node-type match arms for reference extraction; splitting would obscure the logic
fn walk_rust_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
    enclosing_fn: Option<(&str, SymbolKind)>,
) {
    let kind = node.kind();

    // Track enclosing function/method for caller info
    let new_enclosing: Option<(&str, SymbolKind)> = if kind == "function_item" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(|name| (name, SymbolKind::Function))
    } else {
        None
    };
    let current_enclosing = new_enclosing.or(enclosing_fn);

    match kind {
        // Use declarations -> Import references
        "use_declaration" => {
            extract_use_refs(node, source, file, lines, refs, current_enclosing);
            return; // Don't recurse into use subtree — we handled it
        }

        // Call expressions -> Call references
        "call_expression" => {
            if let Some(func_node) = node.child_by_field_name("function") {
                extract_call_ref(func_node, source, file, lines, refs, current_enclosing);
            }
        }

        // Type references in annotations, parameters, return types, etc.
        "type_identifier" => {
            if !is_definition_name(node, source) {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    if !is_rust_primitive(text) {
                        let row = node.start_position().row;
                        let context = lines.get(row).unwrap_or(&"").to_string();
                        refs.push(Reference {
                            file: file.to_path_buf(),
                            line: row + 1,
                            column: node.start_position().column,
                            kind: ReferenceKind::TypeUsage,
                            context,
                            caller: current_enclosing.map(|(n, _)| n.to_string()),
                            caller_kind: current_enclosing.map(|(_, k)| k),
                        });
                    }
                }
            }
            return; // Leaf node
        }

        // Assignment and compound assignment (=, +=, -=, etc.)
        "assignment_expression" | "compound_assignment_expr" => {
            if let Some(left) = node.child_by_field_name("left") {
                extract_assignment_ref(left, source, file, lines, refs, current_enclosing);
            }
        }

        _ => {}
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            walk_rust_node(child, source, file, lines, refs, current_enclosing);
        }
    }
}

/// Extract import references from a `use_declaration` node.
fn extract_use_refs(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
    enclosing: Option<(&str, SymbolKind)>,
) {
    walk_use_tree(node, source, file, lines, refs, enclosing);
}

/// Recursively walk a use tree, extracting identifier references as imports.
fn walk_use_tree(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
    enclosing: Option<(&str, SymbolKind)>,
) {
    let kind = node.kind();

    if kind == "identifier" || kind == "type_identifier" {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            // Skip keywords like "crate", "self", "super"
            if !is_use_keyword(text) {
                let row = node.start_position().row;
                let context = lines.get(row).unwrap_or(&"").to_string();
                refs.push(Reference {
                    file: file.to_path_buf(),
                    line: row + 1,
                    column: node.start_position().column,
                    kind: ReferenceKind::Import,
                    context,
                    caller: enclosing.map(|(n, _)| n.to_string()),
                    caller_kind: enclosing.map(|(_, k)| k),
                });
            }
        }
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_use_tree(child, source, file, lines, refs, enclosing);
        }
    }
}

/// Extract a call reference from a function node in a call expression.
fn extract_call_ref(
    func_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
    enclosing: Option<(&str, SymbolKind)>,
) {
    // For simple calls like `greet()`, the function node is an identifier
    // For path calls like `module::func()`, we want the last segment
    // For method calls like `obj.method()`, tree-sitter-rust uses field_expression
    //   with a "field" child (field_identifier node)
    let target = match func_node.kind() {
        "identifier" => Some(func_node),
        "scoped_identifier" => func_node.child_by_field_name("name"),
        "field_expression" => func_node.child_by_field_name("field"),
        _ => None,
    };

    if let Some(name_node) = target {
        if name_node.utf8_text(source.as_bytes()).is_ok() {
            let row = name_node.start_position().row;
            let context = lines.get(row).unwrap_or(&"").to_string();
            refs.push(Reference {
                file: file.to_path_buf(),
                line: row + 1,
                column: name_node.start_position().column,
                kind: ReferenceKind::Call,
                context,
                caller: enclosing.map(|(n, _)| n.to_string()),
                caller_kind: enclosing.map(|(_, k)| k),
            });
        }
    }
}

/// Extract an assignment reference from the left side of an assignment.
fn extract_assignment_ref(
    left_node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
    enclosing: Option<(&str, SymbolKind)>,
) {
    let target = match left_node.kind() {
        "identifier" => Some(left_node),
        "field_expression" => left_node.child_by_field_name("field"),
        _ => None,
    };

    if let Some(name_node) = target {
        if name_node.utf8_text(source.as_bytes()).is_ok() {
            let row = name_node.start_position().row;
            let context = lines.get(row).unwrap_or(&"").to_string();
            refs.push(Reference {
                file: file.to_path_buf(),
                line: row + 1,
                column: name_node.start_position().column,
                kind: ReferenceKind::Assignment,
                context,
                caller: enclosing.map(|(n, _)| n.to_string()),
                caller_kind: enclosing.map(|(_, k)| k),
            });
        }
    }
}

/// Check if a node is the name field of a definition node.
fn is_definition_name(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let parent_kind = parent.kind();
    // If this node is the "name" field of a definition, it's not a reference
    let is_def_parent = matches!(
        parent_kind,
        "struct_item" | "enum_item" | "trait_item" | "type_item" | "union_item"
    );
    if !is_def_parent {
        return false;
    }
    let Some(name_child) = parent.child_by_field_name("name") else {
        return false;
    };
    // Compare by text position since Node doesn't implement Eq
    let _ = source;
    name_child.start_byte() == node.start_byte() && name_child.end_byte() == node.end_byte()
}

/// Check if a string is a Rust primitive type name or Self.
fn is_rust_primitive(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "char"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "str"
            | "Self"
    )
}

/// Check if a string is a use-path keyword (not a real identifier).
fn is_use_keyword(name: &str) -> bool {
    matches!(name, "crate" | "self" | "super")
}

/// Extract references from any language using a generic identifier-based approach.
///
/// All non-definition identifiers are classified as `Call` since we lack
/// language-specific knowledge to distinguish call vs type vs import.
fn extract_generic_references(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    lines: &[&str],
) -> Vec<Reference> {
    let root = tree.root_node();
    let mut refs = Vec::new();

    walk_generic_node(root, source, file, lines, &mut refs);

    refs
}

/// Recursively walk any AST, extracting identifier references generically.
fn walk_generic_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &Path,
    lines: &[&str],
    refs: &mut Vec<Reference>,
) {
    let kind = node.kind();

    // Treat standalone identifiers as references when they're not definitions
    if kind == "identifier" && !is_generic_definition_name(node, source) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            // Skip common keywords that tree-sitter might parse as identifiers
            if !is_common_keyword(text) {
                let row = node.start_position().row;
                let context = lines.get(row).unwrap_or(&"").to_string();
                refs.push(Reference {
                    file: file.to_path_buf(),
                    line: row + 1,
                    column: node.start_position().column,
                    kind: ReferenceKind::Call,
                    context,
                    caller: None,
                    caller_kind: None,
                });
            }
        }
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            walk_generic_node(child, source, file, lines, refs);
        }
    }
}

/// Check if a node is a definition name in a generic (language-agnostic) sense.
fn is_generic_definition_name(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let parent_kind = parent.kind();

    // Common definition patterns across languages
    let is_definition_parent = parent_kind.contains("definition")
        || parent_kind.contains("declaration")
        || parent_kind.contains("_item");

    if !is_definition_parent {
        return false;
    }

    let Some(name_child) = parent.child_by_field_name("name") else {
        return false;
    };
    let _ = source;
    name_child.start_byte() == node.start_byte() && name_child.end_byte() == node.end_byte()
}

/// Check if a string is a common language keyword.
fn is_common_keyword(name: &str) -> bool {
    matches!(
        name,
        "true"
            | "false"
            | "null"
            | "None"
            | "self"
            | "this"
            | "super"
            | "return"
            | "break"
            | "continue"
            | "if"
            | "else"
            | "for"
            | "while"
            | "match"
            | "let"
            | "mut"
            | "ref"
            | "pub"
            | "fn"
            | "struct"
            | "enum"
            | "trait"
            | "impl"
            | "use"
            | "mod"
            | "const"
            | "static"
            | "type"
            | "where"
            | "as"
            | "in"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_parse::Parser;
    use std::path::PathBuf;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        parser.parse(source.as_bytes()).unwrap()
    }

    fn parse_python(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        parser.parse(source.as_bytes()).unwrap()
    }

    /// Extract the identifier text at a reference's location.
    fn ref_name_at<'a>(r: &Reference, source: &'a str) -> &'a str {
        let line = source.lines().nth(r.line - 1).unwrap_or("");
        let rest = &line[r.column..];
        let end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        &rest[..end]
    }

    fn ref_names<'a>(refs: &[Reference], source: &'a str) -> Vec<&'a str> {
        refs.iter().map(|r| ref_name_at(r, source)).collect()
    }

    // -----------------------------------------------------------------------
    // Rust: function calls
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_finds_function_calls() {
        let source = "fn main() {\n    greet();\n    hello();\n}\nfn greet() {}\nfn hello() {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let call_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .collect();
        assert!(
            call_refs.len() >= 2,
            "expected at least 2 call refs, got {}: {:?}",
            call_refs.len(),
            call_refs
        );

        let names = ref_names(&refs, source);
        assert!(
            names.contains(&"greet"),
            "expected 'greet' in refs, got: {names:?}"
        );
        assert!(
            names.contains(&"hello"),
            "expected 'hello' in refs, got: {names:?}"
        );
    }

    #[test]
    fn test_refs_rust_method_call_extracted() {
        let source = "fn main() {\n    user.validate();\n}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let call_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .collect();
        assert!(
            !call_refs.is_empty(),
            "expected method call reference, got none"
        );

        // The method name should be "validate"
        let names: Vec<&str> = call_refs.iter().map(|r| ref_name_at(r, source)).collect();
        assert!(
            names.contains(&"validate"),
            "expected 'validate' in method call refs, got: {names:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rust: type usages
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_finds_type_usages() {
        let source = "struct User {}\nfn create() -> User {\n    todo!()\n}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let type_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::TypeUsage)
            .collect();
        assert!(
            !type_refs.is_empty(),
            "expected type usage refs, got none. All refs: {:?}",
            refs
        );

        let names: Vec<&str> = type_refs.iter().map(|r| ref_name_at(r, source)).collect();
        assert!(
            names.contains(&"User"),
            "expected 'User' type usage, got: {names:?}"
        );
    }

    #[test]
    fn test_refs_rust_type_usage_in_parameter() {
        let source = "struct User {}\nfn process(user: User) {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let type_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::TypeUsage)
            .collect();
        assert!(
            !type_refs.is_empty(),
            "expected type usage for User in parameter, got none. All refs: {:?}",
            refs
        );
    }

    // -----------------------------------------------------------------------
    // Rust: imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_finds_imports() {
        let source = "use std::collections::HashMap;\nfn main() {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let import_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Import)
            .collect();
        assert!(
            !import_refs.is_empty(),
            "expected import refs, got none. All refs: {:?}",
            refs
        );

        let names: Vec<&str> = import_refs.iter().map(|r| ref_name_at(r, source)).collect();
        assert!(
            names.contains(&"HashMap"),
            "expected 'HashMap' in import refs, got: {names:?}"
        );
    }

    #[test]
    fn test_refs_rust_use_group_imports() {
        let source = "use crate::models::{User, Role};\nfn main() {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let import_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Import)
            .collect();
        let names: Vec<&str> = import_refs.iter().map(|r| ref_name_at(r, source)).collect();
        assert!(
            names.contains(&"User"),
            "expected 'User' in grouped import, got: {names:?}"
        );
        assert!(
            names.contains(&"Role"),
            "expected 'Role' in grouped import, got: {names:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rust: enclosing caller
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_captures_enclosing_caller() {
        let source = "fn main() {\n    greet();\n}\nfn greet() {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let call_in_main: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call && r.caller.as_deref() == Some("main"))
            .collect();
        assert!(
            !call_in_main.is_empty(),
            "expected call with caller='main', got: {:?}",
            refs
        );
    }

    #[test]
    fn test_refs_rust_top_level_import_has_no_caller() {
        let source = "use std::fmt;\nfn main() {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let import_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Import)
            .collect();
        assert!(!import_refs.is_empty());
        for r in &import_refs {
            assert!(
                r.caller.is_none(),
                "top-level import should have no caller, got: {:?}",
                r.caller
            );
        }
    }

    // -----------------------------------------------------------------------
    // Rust: assignments
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_finds_assignments() {
        let source = "fn main() {\n    let mut x = 0;\n    x = 42;\n}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let assign_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Assignment)
            .collect();
        assert!(
            !assign_refs.is_empty(),
            "expected assignment refs, got none. All refs: {:?}",
            refs
        );
    }

    // -----------------------------------------------------------------------
    // Rust: context and location
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_rust_context_contains_source_line() {
        let source = "fn main() {\n    greet();\n}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let call_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .collect();
        assert!(!call_refs.is_empty());

        for r in &call_refs {
            assert!(
                !r.context.is_empty(),
                "reference context should not be empty"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Generic fallback (non-Rust)
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_generic_returns_results_for_python() {
        let source = "def main():\n    greet()\n\ndef greet():\n    pass\n";
        let tree = parse_python(source);
        let file = PathBuf::from("test.py");
        let refs = extract_references(source, &tree, &file, Language::Python);

        // Generic fallback should find at least the "greet" call inside main
        assert!(
            !refs.is_empty(),
            "expected at least one reference from Python generic extraction"
        );
        // All should be classified as Call in generic mode
        assert!(
            refs.iter().all(|r| r.kind == ReferenceKind::Call),
            "generic extraction should classify all as Call, got: {:?}",
            refs.iter().map(|r| r.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_refs_generic_has_no_caller_info() {
        let source = "def main():\n    greet()\n";
        let tree = parse_python(source);
        let file = PathBuf::from("test.py");
        let refs = extract_references(source, &tree, &file, Language::Python);

        // Generic extraction doesn't track callers
        for r in &refs {
            assert!(
                r.caller.is_none(),
                "generic extraction should not set caller, got: {:?}",
                r.caller
            );
        }
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_empty_source_returns_empty() {
        let source = "";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_refs_definitions_only_no_call_references() {
        let source = "fn greet() {}\nstruct User {}\nenum Role {}";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let call_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .collect();
        assert!(
            call_refs.is_empty(),
            "definitions-only source should have no call refs, got: {:?}",
            call_refs
        );
    }

    #[test]
    fn test_refs_rust_skips_primitive_types() {
        let source = "fn foo(x: u32, y: bool) -> i64 { 0 }";
        let tree = parse_rust(source);
        let file = PathBuf::from("test.rs");
        let refs = extract_references(source, &tree, &file, Language::Rust);

        let type_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::TypeUsage)
            .collect();
        assert!(
            type_refs.is_empty(),
            "primitive types should not appear as type usages, got: {:?}",
            type_refs
        );
    }
}
