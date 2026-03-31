//! Type hierarchy types for supertypes and subtypes.

use std::path::PathBuf;

use serde::Serialize;

use crate::symbol::SymbolKind;

/// A node in a type hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TypeHierarchyNode {
    /// The type name.
    pub name: String,
    /// The symbol kind (struct, class, trait, interface, etc.).
    pub kind: SymbolKind,
    /// The file containing this type.
    pub file: PathBuf,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (0-based).
    pub column: usize,
}

/// Result of a type hierarchy query.
#[derive(Debug, Clone, Serialize)]
pub struct TypeHierarchyResult {
    /// The target type.
    pub target: TypeHierarchyNode,
    /// Types that this type extends or implements.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supertypes: Vec<TypeHierarchyNode>,
    /// Types that extend or implement this type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<TypeHierarchyNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_hierarchy_result_serializes_to_json() {
        let result = TypeHierarchyResult {
            target: TypeHierarchyNode {
                name: "Iterator".to_string(),
                kind: SymbolKind::Trait,
                file: PathBuf::from("src/lib.rs"),
                line: 10,
                column: 0,
            },
            supertypes: vec![],
            subtypes: vec![TypeHierarchyNode {
                name: "MyIter".to_string(),
                kind: SymbolKind::Struct,
                file: PathBuf::from("src/iter.rs"),
                line: 5,
                column: 0,
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"Iterator\""));
        assert!(json.contains("\"MyIter\""));
        assert!(!json.contains("\"supertypes\""));
    }
}
