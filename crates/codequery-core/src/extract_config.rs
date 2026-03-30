//! Declarative extraction configuration for plugin languages.
//!
//! Defines the schema for `extract.toml` files that map tree-sitter
//! queries to cq's `Symbol` types. Plugin languages use this format
//! to describe how symbols are extracted from their AST without
//! writing Rust code.

use serde::Deserialize;

use crate::SymbolKind;

/// Top-level configuration from an `extract.toml` file.
///
/// Contains language metadata and a list of symbol extraction rules,
/// each mapping a tree-sitter query to a `SymbolKind`.
#[derive(Debug, Deserialize)]
pub struct ExtractConfig {
    /// Language identification metadata.
    pub language: LanguageInfo,
    /// Symbol extraction rules, each defining how to find one kind of symbol.
    pub symbols: Vec<SymbolRule>,
}

/// Language identification metadata from the `[language]` section.
#[derive(Debug, Deserialize)]
pub struct LanguageInfo {
    /// The canonical language name (e.g., `"python"`, `"elixir"`).
    pub name: String,
    /// File extensions associated with this language (e.g., `[".py"]`).
    pub extensions: Vec<String>,
}

/// A single symbol extraction rule from a `[[symbols]]` entry.
///
/// Each rule defines a tree-sitter query that matches one kind of symbol,
/// along with capture names that specify which parts of the match map to
/// symbol fields (name, body, etc.).
#[derive(Debug, Deserialize)]
pub struct SymbolRule {
    /// The symbol kind string (e.g., `"function"`, `"class"`, `"module"`).
    /// Mapped to `SymbolKind` at runtime via [`parse_symbol_kind`].
    pub kind: String,
    /// The tree-sitter query pattern that matches this symbol type.
    pub query: String,
    /// The capture name for the symbol's name (e.g., `"@name"`).
    pub name: String,
    /// The capture name for the symbol's body, if available.
    #[serde(default)]
    pub body: Option<String>,
    /// The doc extraction strategy (e.g., `"preceding_comment"`, `"docstring"`).
    #[serde(default)]
    pub doc: Option<String>,
    /// The visibility strategy (e.g., `"pub_keyword"`, `"underscore_prefix"`).
    #[serde(default)]
    pub visibility: Option<String>,
}

/// Parse a symbol kind string into a `SymbolKind` enum variant.
///
/// Returns `None` if the string does not match any known variant.
/// Matching is case-insensitive.
#[must_use]
pub fn parse_symbol_kind(s: &str) -> Option<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "struct" => Some(SymbolKind::Struct),
        "class" => Some(SymbolKind::Class),
        "trait" => Some(SymbolKind::Trait),
        "interface" => Some(SymbolKind::Interface),
        "enum" => Some(SymbolKind::Enum),
        "type" => Some(SymbolKind::Type),
        "const" => Some(SymbolKind::Const),
        "static" => Some(SymbolKind::Static),
        "module" => Some(SymbolKind::Module),
        "impl" => Some(SymbolKind::Impl),
        "test" => Some(SymbolKind::Test),
        _ => None,
    }
}

/// Load an `ExtractConfig` from a TOML string.
///
/// # Errors
///
/// Returns an error message if the TOML cannot be parsed or does not
/// match the expected schema.
pub fn load_extract_config(toml_str: &str) -> std::result::Result<ExtractConfig, String> {
    toml::from_str(toml_str).map_err(|e| format!("failed to parse extract.toml: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_symbol_kind_all_variants() {
        let cases = [
            ("function", Some(SymbolKind::Function)),
            ("method", Some(SymbolKind::Method)),
            ("struct", Some(SymbolKind::Struct)),
            ("class", Some(SymbolKind::Class)),
            ("trait", Some(SymbolKind::Trait)),
            ("interface", Some(SymbolKind::Interface)),
            ("enum", Some(SymbolKind::Enum)),
            ("type", Some(SymbolKind::Type)),
            ("const", Some(SymbolKind::Const)),
            ("static", Some(SymbolKind::Static)),
            ("module", Some(SymbolKind::Module)),
            ("impl", Some(SymbolKind::Impl)),
            ("test", Some(SymbolKind::Test)),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_symbol_kind(input), expected, "failed for {input}");
        }
    }

    #[test]
    fn test_parse_symbol_kind_case_insensitive() {
        assert_eq!(parse_symbol_kind("Function"), Some(SymbolKind::Function));
        assert_eq!(parse_symbol_kind("CLASS"), Some(SymbolKind::Class));
        assert_eq!(parse_symbol_kind("Struct"), Some(SymbolKind::Struct));
    }

    #[test]
    fn test_parse_symbol_kind_unknown_returns_none() {
        assert_eq!(parse_symbol_kind("unknown"), None);
        assert_eq!(parse_symbol_kind(""), None);
        assert_eq!(parse_symbol_kind("lambda"), None);
    }

    #[test]
    fn test_load_extract_config_minimal() {
        let toml = r#"
[language]
name = "test"
extensions = [".test"]

[[symbols]]
kind = "function"
query = '(function_definition name: (identifier) @name)'
name = "@name"
"#;
        let config = load_extract_config(toml).unwrap();
        assert_eq!(config.language.name, "test");
        assert_eq!(config.language.extensions, vec![".test"]);
        assert_eq!(config.symbols.len(), 1);
        assert_eq!(config.symbols[0].kind, "function");
        assert_eq!(config.symbols[0].name, "@name");
        assert!(config.symbols[0].body.is_none());
        assert!(config.symbols[0].doc.is_none());
        assert!(config.symbols[0].visibility.is_none());
    }

    #[test]
    fn test_load_extract_config_full() {
        let toml = r#"
[language]
name = "python"
extensions = [".py"]

[[symbols]]
kind = "function"
query = '(function_definition name: (identifier) @name) @def'
name = "@name"
body = "@def"
doc = "docstring"
visibility = "underscore_prefix"

[[symbols]]
kind = "class"
query = '(class_definition name: (identifier) @name body: (block) @body)'
name = "@name"
body = "@body"
"#;
        let config = load_extract_config(toml).unwrap();
        assert_eq!(config.language.name, "python");
        assert_eq!(config.symbols.len(), 2);
        assert_eq!(config.symbols[0].body, Some("@def".to_string()));
        assert_eq!(config.symbols[0].doc, Some("docstring".to_string()));
        assert_eq!(
            config.symbols[0].visibility,
            Some("underscore_prefix".to_string())
        );
        assert_eq!(config.symbols[1].kind, "class");
        assert!(config.symbols[1].doc.is_none());
    }

    #[test]
    fn test_load_extract_config_invalid_toml_returns_error() {
        let result = load_extract_config("this is not valid toml {{{");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse extract.toml"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_load_extract_config_missing_required_field_returns_error() {
        let toml = r#"
[language]
name = "test"
# missing extensions

[[symbols]]
kind = "function"
query = '(function_definition)'
name = "@name"
"#;
        let result = load_extract_config(toml);
        assert!(result.is_err());
    }
}
