//! Structural supertype relation extraction from tree-sitter ASTs.
//!
//! Walks a parse tree to find inheritance and trait implementation
//! declarations, producing `SupertypeRelation` values. This provides
//! an AST-level type hierarchy fallback when no language server is
//! available.

use std::path::{Path, PathBuf};

use codequery_core::Language;

/// A single supertype relationship extracted from the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupertypeRelation {
    /// The subtype (the implementing/extending type).
    pub subtype: String,
    /// The supertype (the trait/interface/base class).
    pub supertype: String,
    /// The file containing this relation.
    pub file: PathBuf,
    /// The 1-based line number of the declaration.
    pub line: usize,
}

/// Extract supertype relations from a parsed source file.
///
/// Walks the tree and finds inheritance declarations: trait impls in Rust,
/// `extends`/`implements` in TypeScript/Java, base classes in Python,
/// and embedded types in Go. Returns an empty `Vec` for unsupported
/// languages or when no inheritance is found.
#[must_use]
pub fn extract_supertype_relations(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,
) -> Vec<SupertypeRelation> {
    let root = tree.root_node();
    let mut relations = Vec::new();

    match language {
        Language::Rust => collect_rust_relations(&root, source, file, &mut relations),
        Language::TypeScript | Language::JavaScript => {
            collect_ts_relations(&root, source, file, &mut relations);
        }
        Language::Python => collect_python_relations(&root, source, file, &mut relations),
        Language::Java => collect_java_relations(&root, source, file, &mut relations),
        Language::Go => collect_go_relations(&root, source, file, &mut relations),
        _ => {} // Graceful degradation for unsupported languages
    }

    relations
}

/// Extract UTF-8 text from a tree-sitter node.
fn node_text(node: &tree_sitter::Node, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes())
        .ok()
        .map(std::string::ToString::to_string)
}

/// Find a named child of a specific kind.
fn find_child_of_kind<'a>(
    node: &'a tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

// ---------------------------------------------------------------------------
// Rust: `impl Trait for Type`
// ---------------------------------------------------------------------------

fn collect_rust_relations(
    node: &tree_sitter::Node,
    source: &str,
    file: &Path,
    relations: &mut Vec<SupertypeRelation>,
) {
    if node.kind() == "impl_item" {
        // Look for `impl Trait for Type` pattern.
        // The trait is the `trait` field, the type is the `type` field.
        if let (Some(trait_node), Some(type_node)) = (
            node.child_by_field_name("trait"),
            node.child_by_field_name("type"),
        ) {
            if let (Some(supertype), Some(subtype)) = (
                node_text(&trait_node, source),
                node_text(&type_node, source),
            ) {
                relations.push(SupertypeRelation {
                    subtype,
                    supertype,
                    file: file.to_path_buf(),
                    line: node.start_position().row + 1,
                });
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_rust_relations(&child, source, file, relations);
    }
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript: `class Foo extends Bar implements Baz`
// ---------------------------------------------------------------------------

fn collect_ts_relations(
    node: &tree_sitter::Node,
    source: &str,
    file: &Path,
    relations: &mut Vec<SupertypeRelation>,
) {
    if node.kind() == "class_declaration" {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, source));

        if let Some(subtype) = class_name {
            // TypeScript AST: class_declaration → class_heritage → extends_clause / implements_clause
            // Each clause contains type identifiers as named children.
            if let Some(heritage) = find_child_of_kind(node, "class_heritage") {
                let mut heritage_cursor = heritage.walk();
                for clause in heritage.named_children(&mut heritage_cursor) {
                    if clause.kind() == "extends_clause" || clause.kind() == "implements_clause" {
                        collect_ts_clause_types(&clause, source, file, &subtype, node, relations);
                    }
                }
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_relations(&child, source, file, relations);
    }
}

/// Extract type names from an extends_clause or implements_clause.
fn collect_ts_clause_types(
    clause: &tree_sitter::Node,
    source: &str,
    file: &Path,
    subtype: &str,
    class_node: &tree_sitter::Node,
    relations: &mut Vec<SupertypeRelation>,
) {
    let mut cursor = clause.walk();
    for child in clause.named_children(&mut cursor) {
        if let Some(name) = extract_type_name(&child, source) {
            relations.push(SupertypeRelation {
                subtype: subtype.to_string(),
                supertype: name,
                file: file.to_path_buf(),
                line: class_node.start_position().row + 1,
            });
        }
    }
}

/// Extract a type name from a node, handling identifiers and generic types.
fn extract_type_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    if node.kind() == "generic_type" {
        // Extract just the base name from `Foo<T>`
        node.child_by_field_name("name")
            .and_then(|n| node_text(&n, source))
            .or_else(|| {
                // Fallback: first named child
                let mut cursor = node.walk();
                let first = node
                    .named_children(&mut cursor)
                    .next()
                    .and_then(|n| node_text(&n, source));
                first
            })
    } else {
        node_text(node, source)
    }
}

// ---------------------------------------------------------------------------
// Python: `class Foo(Bar, Baz):`
// ---------------------------------------------------------------------------

fn collect_python_relations(
    node: &tree_sitter::Node,
    source: &str,
    file: &Path,
    relations: &mut Vec<SupertypeRelation>,
) {
    if node.kind() == "class_definition" {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, source));

        if let Some(subtype) = class_name {
            // Base classes are in the `superclasses` field (argument_list)
            if let Some(args) = node.child_by_field_name("superclasses") {
                let mut cursor = args.walk();
                for arg in args.named_children(&mut cursor) {
                    // Skip keyword arguments like `metaclass=ABCMeta`
                    if arg.kind() == "keyword_argument" {
                        continue;
                    }
                    if let Some(name) = node_text(&arg, source) {
                        relations.push(SupertypeRelation {
                            subtype: subtype.clone(),
                            supertype: name,
                            file: file.to_path_buf(),
                            line: node.start_position().row + 1,
                        });
                    }
                }
            }
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_python_relations(&child, source, file, relations);
    }
}

// ---------------------------------------------------------------------------
// Java: `class Foo extends Bar implements Baz, Qux`
// ---------------------------------------------------------------------------

fn collect_java_relations(
    node: &tree_sitter::Node,
    source: &str,
    file: &Path,
    relations: &mut Vec<SupertypeRelation>,
) {
    if node.kind() == "class_declaration" {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, source));

        if let Some(subtype) = class_name {
            // `superclass` field for extends
            if let Some(superclass) = node.child_by_field_name("superclass") {
                // The superclass node is a `superclass` wrapper containing a type
                let mut cursor = superclass.walk();
                for child in superclass.named_children(&mut cursor) {
                    if let Some(name) = extract_type_name(&child, source) {
                        relations.push(SupertypeRelation {
                            subtype: subtype.clone(),
                            supertype: name,
                            file: file.to_path_buf(),
                            line: node.start_position().row + 1,
                        });
                    }
                }
            }

            // `interfaces` field for implements
            if let Some(interfaces) = node.child_by_field_name("interfaces") {
                // super_interfaces node contains a type_list
                let mut cursor = interfaces.walk();
                for child in interfaces.named_children(&mut cursor) {
                    if let Some(name) = extract_type_name(&child, source) {
                        relations.push(SupertypeRelation {
                            subtype: subtype.clone(),
                            supertype: name,
                            file: file.to_path_buf(),
                            line: node.start_position().row + 1,
                        });
                    }
                }
            }
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_java_relations(&child, source, file, relations);
    }
}

// ---------------------------------------------------------------------------
// Go: embedded fields in struct types
// ---------------------------------------------------------------------------

fn collect_go_relations(
    node: &tree_sitter::Node,
    source: &str,
    file: &Path,
    relations: &mut Vec<SupertypeRelation>,
) {
    if node.kind() == "type_spec" {
        let type_name = node
            .child_by_field_name("name")
            .and_then(|n| node_text(&n, source));

        if let Some(subtype) = type_name {
            // Look for struct_type child
            if let Some(struct_type) = node.child_by_field_name("type") {
                if struct_type.kind() == "struct_type" {
                    if let Some(field_list) =
                        find_child_of_kind(&struct_type, "field_declaration_list")
                    {
                        let mut cursor = field_list.walk();
                        for field in field_list.named_children(&mut cursor) {
                            if field.kind() == "field_declaration" {
                                // Embedded field: has a type but no name
                                let has_name = field.child_by_field_name("name").is_some();
                                if !has_name {
                                    if let Some(type_node) = field.child_by_field_name("type") {
                                        if let Some(name) = node_text(&type_node, source) {
                                            relations.push(SupertypeRelation {
                                                subtype: subtype.clone(),
                                                supertype: name,
                                                file: file.to_path_buf(),
                                                line: field.start_position().row + 1,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_go_relations(&child, source, file, relations);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use std::path::PathBuf;

    #[test]
    fn test_rust_impl_trait_for_type() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "use std::fmt::Display;\nstruct Foo;\nimpl Display for Foo {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Rust);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].subtype, "Foo");
        assert_eq!(relations[0].supertype, "Display");
    }

    #[test]
    fn test_rust_inherent_impl_no_relation() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "struct Foo;\nimpl Foo {\n    fn bar(&self) {}\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Rust);
        assert!(
            relations.is_empty(),
            "inherent impl should not produce a supertype relation"
        );
    }

    #[test]
    fn test_typescript_extends() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let source = "class Dog extends Animal {\n  bark() {}\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.ts");

        let relations = extract_supertype_relations(source, &tree, &file, Language::TypeScript);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].subtype, "Dog");
        assert_eq!(relations[0].supertype, "Animal");
    }

    #[test]
    fn test_typescript_implements() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let source =
            "interface Runnable { run(): void; }\nclass Dog extends Animal implements Runnable {\n  run() {}\n  bark() {}\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.ts");

        let relations = extract_supertype_relations(source, &tree, &file, Language::TypeScript);
        // Filter to just Dog's relations (interface declarations don't produce relations)
        let dog_relations: Vec<_> = relations.iter().filter(|r| r.subtype == "Dog").collect();
        assert!(
            dog_relations.len() >= 2,
            "expected at least 2 relations (extends + implements), got {}",
            dog_relations.len()
        );
        let supertypes: Vec<&str> = dog_relations.iter().map(|r| r.supertype.as_str()).collect();
        assert!(supertypes.contains(&"Animal"), "missing Animal supertype");
        assert!(
            supertypes.contains(&"Runnable"),
            "missing Runnable supertype"
        );
    }

    #[test]
    fn test_python_single_base_class() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let source = "class Dog(Animal):\n    pass\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.py");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Python);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].subtype, "Dog");
        assert_eq!(relations[0].supertype, "Animal");
    }

    #[test]
    fn test_python_multiple_base_classes() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let source = "class Dog(Animal, Pet):\n    pass\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.py");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Python);
        assert_eq!(relations.len(), 2);
        let supertypes: Vec<&str> = relations.iter().map(|r| r.supertype.as_str()).collect();
        assert!(supertypes.contains(&"Animal"));
        assert!(supertypes.contains(&"Pet"));
    }

    #[test]
    fn test_java_extends_and_implements() {
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let source = "class Dog extends Animal implements Serializable {\n    void bark() {}\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("Test.java");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Java);
        assert!(
            relations.len() >= 2,
            "expected at least 2 relations, got {}",
            relations.len()
        );
        let supertypes: Vec<&str> = relations.iter().map(|r| r.supertype.as_str()).collect();
        assert!(supertypes.contains(&"Animal"), "missing Animal supertype");
        assert!(
            supertypes.contains(&"Serializable"),
            "missing Serializable supertype"
        );
    }

    #[test]
    fn test_go_embedded_struct() {
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let source = "package main\n\ntype Animal struct {\n    Name string\n}\n\ntype Dog struct {\n    Animal\n    Breed string\n}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("main.go");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Go);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].subtype, "Dog");
        assert_eq!(relations[0].supertype, "Animal");
    }

    #[test]
    fn test_no_inheritance_returns_empty() {
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let source = "struct Foo { x: i32 }\nfn bar() {}\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.rs");

        let relations = extract_supertype_relations(source, &tree, &file, Language::Rust);
        assert!(relations.is_empty());
    }

    #[test]
    fn test_unsupported_language_returns_empty() {
        // C has no class/trait inheritance, so hierarchy extraction returns empty
        let mut parser = Parser::for_language(Language::C).unwrap();
        let source = "int main() { return 0; }\n";
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from("test.c");

        let relations = extract_supertype_relations(source, &tree, &file, Language::C);
        assert!(relations.is_empty());
    }
}
