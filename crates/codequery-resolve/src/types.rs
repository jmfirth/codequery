//! Core types for name resolution results.
//!
//! These types represent the output of stack graph resolution — mapping
//! references to their definitions across files.

use std::path::PathBuf;

pub use codequery_core::Resolution;

/// A reference that has been resolved to its definition via stack graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReference {
    /// File containing the reference.
    pub ref_file: PathBuf,
    /// 1-based line of the reference.
    pub ref_line: usize,
    /// 0-based column of the reference.
    pub ref_column: usize,
    /// Symbol name of the reference.
    pub symbol: String,
    /// File containing the definition (if resolved).
    pub def_file: Option<PathBuf>,
    /// 1-based line of the definition (if resolved).
    pub def_line: Option<usize>,
    /// 0-based column of the definition (if resolved).
    pub def_column: Option<usize>,
    /// How this reference was resolved.
    pub resolution: Resolution,
}

/// Result of a resolution operation across potentially multiple files.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// All resolved references found.
    pub references: Vec<ResolvedReference>,
    /// Warnings from files that could not be fully resolved.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_reference_creation() {
        let rr = ResolvedReference {
            ref_file: PathBuf::from("app.py"),
            ref_line: 5,
            ref_column: 0,
            symbol: "greet".to_string(),
            def_file: Some(PathBuf::from("main.py")),
            def_line: Some(1),
            def_column: Some(4),
            resolution: Resolution::Resolved,
        };
        assert_eq!(rr.symbol, "greet");
        assert_eq!(rr.resolution, Resolution::Resolved);
    }

    #[test]
    fn test_resolved_reference_unresolved() {
        let rr = ResolvedReference {
            ref_file: PathBuf::from("app.py"),
            ref_line: 3,
            ref_column: 0,
            symbol: "unknown".to_string(),
            def_file: None,
            def_line: None,
            def_column: None,
            resolution: Resolution::Syntactic,
        };
        assert!(rr.def_file.is_none());
        assert_eq!(rr.resolution, Resolution::Syntactic);
    }

    #[test]
    fn test_resolution_result_empty() {
        let result = ResolutionResult {
            references: vec![],
            warnings: vec![],
        };
        assert!(result.references.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_resolution_result_with_references() {
        let result = ResolutionResult {
            references: vec![ResolvedReference {
                ref_file: PathBuf::from("a.py"),
                ref_line: 1,
                ref_column: 0,
                symbol: "foo".to_string(),
                def_file: Some(PathBuf::from("b.py")),
                def_line: Some(10),
                def_column: Some(4),
                resolution: Resolution::Resolved,
            }],
            warnings: vec!["partial resolution for c.py".to_string()],
        };
        assert_eq!(result.references.len(), 1);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_resolution_enum_variants() {
        assert_ne!(Resolution::Resolved, Resolution::Syntactic);
    }
}
