//! Per-language stack graph rule configuration.
//!
//! Each supported language has a vendored TSG (tree-sitter graph) rule file that
//! describes how AST nodes map to stack graph nodes. This module provides a
//! factory for creating `StackGraphLanguage` instances and a predicate for
//! checking which languages have rules available.

pub mod java;
pub mod javascript;
pub mod python;
pub mod typescript;

use codequery_core::Language;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error;

/// Check if a language has stack graph rules available.
///
/// Returns `true` for Python, TypeScript, JavaScript, and Java.
/// Returns `false` for Rust, Go, C, and C++ (not yet supported).
#[must_use]
pub fn has_rules(lang: Language) -> bool {
    matches!(
        lang,
        Language::Python | Language::TypeScript | Language::JavaScript | Language::Java
    )
}

/// Create a `StackGraphLanguage` for the given language.
///
/// Returns `None` for languages without stack graph rules (Rust, Go, C, C++).
/// Returns `Some(Ok(_))` on successful rule loading, or `Some(Err(_))` if the
/// TSG rules fail to parse.
#[must_use]
pub fn language_config(lang: Language) -> Option<error::Result<StackGraphLanguage>> {
    match lang {
        Language::Python => Some(python::create_language()),
        Language::TypeScript => Some(typescript::create_language()),
        Language::JavaScript => Some(javascript::create_language()),
        Language::Java => Some(java::create_language()),
        Language::Rust | Language::Go | Language::C | Language::Cpp => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_rules_python_returns_true() {
        assert!(has_rules(Language::Python));
    }

    #[test]
    fn test_has_rules_typescript_returns_true() {
        assert!(has_rules(Language::TypeScript));
    }

    #[test]
    fn test_has_rules_javascript_returns_true() {
        assert!(has_rules(Language::JavaScript));
    }

    #[test]
    fn test_has_rules_java_returns_true() {
        assert!(has_rules(Language::Java));
    }

    #[test]
    fn test_has_rules_rust_returns_false() {
        assert!(!has_rules(Language::Rust));
    }

    #[test]
    fn test_has_rules_go_returns_false() {
        assert!(!has_rules(Language::Go));
    }

    #[test]
    fn test_has_rules_c_returns_false() {
        assert!(!has_rules(Language::C));
    }

    #[test]
    fn test_has_rules_cpp_returns_false() {
        assert!(!has_rules(Language::Cpp));
    }

    #[test]
    fn test_language_config_python_loads_successfully() {
        let result = language_config(Language::Python);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_ok(),
            "Python language config should load"
        );
    }

    #[test]
    fn test_language_config_typescript_loads_successfully() {
        let result = language_config(Language::TypeScript);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_ok(),
            "TypeScript language config should load"
        );
    }

    #[test]
    fn test_language_config_javascript_loads_successfully() {
        let result = language_config(Language::JavaScript);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_ok(),
            "JavaScript language config should load"
        );
    }

    #[test]
    fn test_language_config_java_loads_successfully() {
        let result = language_config(Language::Java);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "Java language config should load");
    }

    #[test]
    fn test_language_config_rust_returns_none() {
        assert!(language_config(Language::Rust).is_none());
    }

    #[test]
    fn test_language_config_go_returns_none() {
        assert!(language_config(Language::Go).is_none());
    }

    #[test]
    fn test_language_config_c_returns_none() {
        assert!(language_config(Language::C).is_none());
    }

    #[test]
    fn test_language_config_cpp_returns_none() {
        assert!(language_config(Language::Cpp).is_none());
    }
}
