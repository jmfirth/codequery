//! Core symbol types for representing extracted source code entities.

use std::fmt;
use std::path::PathBuf;

/// A source code symbol extracted from a parsed file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Symbol {
    /// The symbol's name as it appears in source code.
    pub name: String,
    /// What kind of symbol this is (function, struct, etc.).
    pub kind: SymbolKind,
    /// The file path containing this symbol.
    pub file: PathBuf,
    /// The 1-based starting line number.
    pub line: usize,
    /// The 0-based starting column number.
    pub column: usize,
    /// The 1-based ending line number.
    pub end_line: usize,
    /// The visibility of this symbol.
    pub visibility: Visibility,
    /// Child symbols (e.g., methods inside an impl block).
    pub children: Vec<Symbol>,
    /// Documentation comment attached to this symbol, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Full source text of the symbol body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Signature/header only (no body).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// The kind of a source code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// A free function.
    Function,
    /// A method on a type.
    Method,
    /// A struct definition.
    Struct,
    /// A class definition (for languages with classes).
    Class,
    /// A trait definition (Rust).
    Trait,
    /// An interface definition (TypeScript, Go, Java).
    Interface,
    /// An enum definition.
    Enum,
    /// A type alias.
    Type,
    /// A constant binding.
    Const,
    /// A static binding.
    Static,
    /// A module declaration.
    Module,
    /// An impl block.
    Impl,
    /// A test function.
    Test,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Type => "type",
            Self::Const => "const",
            Self::Static => "static",
            Self::Module => "module",
            Self::Impl => "impl",
            Self::Test => "test",
        };
        write!(f, "{s}")
    }
}

/// Source code location.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Location {
    /// The file path.
    pub file: PathBuf,
    /// The 1-based line number.
    pub line: usize,
    /// The 0-based column number.
    pub column: usize,
}

/// Symbol visibility level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Visibility {
    /// Visible to all.
    #[serde(rename = "pub")]
    Public,
    /// Visible only within the containing scope.
    #[serde(rename = "priv")]
    Private,
    /// Visible within the current crate.
    #[serde(rename = "pub(crate)")]
    Crate,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Public => "pub",
            Self::Private => "priv",
            Self::Crate => "pub(crate)",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_display_outputs_correct_string_for_every_variant() {
        let cases = [
            (SymbolKind::Function, "function"),
            (SymbolKind::Method, "method"),
            (SymbolKind::Struct, "struct"),
            (SymbolKind::Class, "class"),
            (SymbolKind::Trait, "trait"),
            (SymbolKind::Interface, "interface"),
            (SymbolKind::Enum, "enum"),
            (SymbolKind::Type, "type"),
            (SymbolKind::Const, "const"),
            (SymbolKind::Static, "static"),
            (SymbolKind::Module, "module"),
            (SymbolKind::Impl, "impl"),
            (SymbolKind::Test, "test"),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
        }
    }

    #[test]
    fn test_visibility_display_outputs_correct_string_for_every_variant() {
        assert_eq!(Visibility::Public.to_string(), "pub");
        assert_eq!(Visibility::Private.to_string(), "priv");
        assert_eq!(Visibility::Crate.to_string(), "pub(crate)");
    }

    #[test]
    fn test_symbol_constructed_with_all_fields_and_accessed() {
        let sym = Symbol {
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/main.rs"),
            line: 10,
            column: 0,
            end_line: 20,
            visibility: Visibility::Public,
            children: vec![],
            doc: Some("A function".to_string()),
            body: None,
            signature: None,
        };
        assert_eq!(sym.name, "my_func");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.file, PathBuf::from("src/main.rs"));
        assert_eq!(sym.line, 10);
        assert_eq!(sym.column, 0);
        assert_eq!(sym.end_line, 20);
        assert_eq!(sym.visibility, Visibility::Public);
        assert!(sym.children.is_empty());
        assert_eq!(sym.doc.as_deref(), Some("A function"));
    }

    #[test]
    fn test_symbol_with_empty_children_vec() {
        let sym = Symbol {
            name: "Empty".to_string(),
            kind: SymbolKind::Struct,
            file: PathBuf::from("lib.rs"),
            line: 1,
            column: 0,
            end_line: 1,
            visibility: Visibility::Private,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        };
        assert!(sym.children.is_empty());
    }

    #[test]
    fn test_symbol_with_nested_children() {
        let child = Symbol {
            name: "inner_method".to_string(),
            kind: SymbolKind::Method,
            file: PathBuf::from("src/lib.rs"),
            line: 5,
            column: 4,
            end_line: 10,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        };
        let parent = Symbol {
            name: "MyStruct".to_string(),
            kind: SymbolKind::Impl,
            file: PathBuf::from("src/lib.rs"),
            line: 3,
            column: 0,
            end_line: 12,
            visibility: Visibility::Public,
            children: vec![child],
            doc: Some("Impl block".to_string()),
            body: None,
            signature: None,
        };
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "inner_method");
        assert_eq!(parent.children[0].kind, SymbolKind::Method);
    }

    #[test]
    fn test_symbol_serializes_to_json_with_expected_structure() {
        let sym = Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/main.rs"),
            line: 1,
            column: 0,
            end_line: 5,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert_eq!(json["name"], "foo");
        assert_eq!(json["kind"], "function");
        assert_eq!(json["file"], "src/main.rs");
        assert_eq!(json["line"], 1);
        assert_eq!(json["column"], 0);
        assert_eq!(json["end_line"], 5);
        assert_eq!(json["visibility"], "pub");
        assert_eq!(json["children"], serde_json::json!([]));
        // doc is None and skip_serializing_if means it should be absent
        assert!(json.get("doc").is_none());
        // body and signature are None and skip_serializing_if means they should be absent
        assert!(json.get("body").is_none());
        assert!(json.get("signature").is_none());
    }

    #[test]
    fn test_symbol_kind_serializes_as_snake_case_in_json() {
        // Test all variants serialize to snake_case
        let cases = [
            (SymbolKind::Function, "function"),
            (SymbolKind::Method, "method"),
            (SymbolKind::Struct, "struct"),
            (SymbolKind::Class, "class"),
            (SymbolKind::Trait, "trait"),
            (SymbolKind::Interface, "interface"),
            (SymbolKind::Enum, "enum"),
            (SymbolKind::Type, "type"),
            (SymbolKind::Const, "const"),
            (SymbolKind::Static, "static"),
            (SymbolKind::Module, "module"),
            (SymbolKind::Impl, "impl"),
            (SymbolKind::Test, "test"),
        ];
        for (kind, expected) in cases {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_visibility_serializes_as_spec_values_in_json() {
        assert_eq!(serde_json::to_value(Visibility::Public).unwrap(), "pub");
        assert_eq!(serde_json::to_value(Visibility::Private).unwrap(), "priv");
        assert_eq!(
            serde_json::to_value(Visibility::Crate).unwrap(),
            "pub(crate)"
        );
    }

    #[test]
    fn test_symbol_body_and_signature_fields_none_omitted_from_json() {
        let sym = Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("test.rs"),
            line: 1,
            column: 0,
            end_line: 3,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert!(json.get("body").is_none());
        assert!(json.get("signature").is_none());
    }

    #[test]
    fn test_symbol_body_and_signature_fields_some_present_in_json() {
        let sym = Symbol {
            name: "bar".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("test.rs"),
            line: 1,
            column: 0,
            end_line: 5,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: Some("fn bar() {\n    42\n}".to_string()),
            signature: Some("fn bar()".to_string()),
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert_eq!(json["body"], "fn bar() {\n    42\n}");
        assert_eq!(json["signature"], "fn bar()");
    }

    #[test]
    fn test_location_constructed_and_serialized() {
        let loc = Location {
            file: PathBuf::from("src/lib.rs"),
            line: 42,
            column: 8,
        };
        assert_eq!(loc.file, PathBuf::from("src/lib.rs"));
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 8);

        let json = serde_json::to_value(&loc).unwrap();
        assert_eq!(json["file"], "src/lib.rs");
        assert_eq!(json["line"], 42);
        assert_eq!(json["column"], 8);
    }
}
