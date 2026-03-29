//! Reference types for representing symbol usage sites.
//!
//! A `Reference` captures where a symbol is used — call sites, imports, type usages,
//! and assignments. These are produced by cross-reference commands (`refs`, `callers`).

use std::fmt;
use std::path::PathBuf;

use crate::symbol::SymbolKind;

/// A reference to a symbol (call site, import, type usage, etc.).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Reference {
    /// The file path containing this reference.
    pub file: PathBuf,
    /// The 1-based line number of the reference.
    pub line: usize,
    /// The 0-based column number of the reference.
    pub column: usize,
    /// What kind of reference this is.
    pub kind: ReferenceKind,
    /// The source line containing the reference.
    pub context: String,
    /// Name of the enclosing function (for the callers command).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// Kind of the enclosing symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_kind: Option<SymbolKind>,
}

/// The kind of a reference to a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// A function or method call.
    Call,
    /// A type usage (in a signature, variable type, etc.).
    TypeUsage,
    /// An import statement.
    Import,
    /// An assignment to the symbol.
    Assignment,
    /// The definition location itself (def-as-ref, matches LSP convention).
    Definition,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Call => "call",
            Self::TypeUsage => "type_usage",
            Self::Import => "import",
            Self::Assignment => "assignment",
            Self::Definition => "definition",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_constructed_and_serialized() {
        let r = Reference {
            file: PathBuf::from("src/main.rs"),
            line: 10,
            column: 4,
            kind: ReferenceKind::Call,
            context: "    greet(name);".to_string(),
            caller: Some("main".to_string()),
            caller_kind: Some(SymbolKind::Function),
        };
        assert_eq!(r.file, PathBuf::from("src/main.rs"));
        assert_eq!(r.line, 10);
        assert_eq!(r.column, 4);
        assert_eq!(r.kind, ReferenceKind::Call);
        assert_eq!(r.context, "    greet(name);");
        assert_eq!(r.caller.as_deref(), Some("main"));
        assert_eq!(r.caller_kind, Some(SymbolKind::Function));

        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["file"], "src/main.rs");
        assert_eq!(json["line"], 10);
        assert_eq!(json["column"], 4);
        assert_eq!(json["kind"], "call");
        assert_eq!(json["context"], "    greet(name);");
        assert_eq!(json["caller"], "main");
        assert_eq!(json["caller_kind"], "function");
    }

    #[test]
    fn test_reference_without_caller_omits_fields_in_json() {
        let r = Reference {
            file: PathBuf::from("src/lib.rs"),
            line: 5,
            column: 0,
            kind: ReferenceKind::Import,
            context: "use crate::models::User;".to_string(),
            caller: None,
            caller_kind: None,
        };
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("caller").is_none());
        assert!(json.get("caller_kind").is_none());
        assert_eq!(json["kind"], "import");
    }

    #[test]
    fn test_reference_kind_display_and_serialization() {
        let cases = [
            (ReferenceKind::Call, "call"),
            (ReferenceKind::TypeUsage, "type_usage"),
            (ReferenceKind::Import, "import"),
            (ReferenceKind::Assignment, "assignment"),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
            assert_eq!(serde_json::to_value(kind).unwrap(), expected);
        }
    }
}
