//! Per-language stack graph rule configuration.
//!
//! Each supported language has a vendored TSG (tree-sitter graph) rule file that
//! describes how AST nodes map to stack graph nodes. This module provides a
//! factory for creating `StackGraphLanguage` instances and a predicate for
//! checking which languages have rules available.

pub mod c;
pub mod cpp;
pub mod go;
pub mod java;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod typescript;

use codequery_core::Language;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error;

/// Check if a language has stack graph rules available.
///
/// Returns `true` for Python, TypeScript, JavaScript, Java, Go, C, C++, and Rust.
/// Returns `false` for all Tier 2 languages (not yet supported).
#[must_use]
pub fn has_rules(lang: Language) -> bool {
    matches!(
        lang,
        Language::Python
            | Language::TypeScript
            | Language::JavaScript
            | Language::Java
            | Language::Go
            | Language::C
            | Language::Cpp
            | Language::Rust
    )
}

/// Create a `StackGraphLanguage` for the given language.
///
/// Returns `None` for languages without stack graph rules (all Tier 2 languages).
/// Returns `Some(Ok(_))` on successful rule loading, or `Some(Err(_))` if the
/// TSG rules fail to parse.
#[must_use]
pub fn language_config(lang: Language) -> Option<error::Result<StackGraphLanguage>> {
    match lang {
        Language::Python => Some(python::create_language()),
        Language::TypeScript => Some(typescript::create_language()),
        Language::JavaScript => Some(javascript::create_language()),
        Language::Java => Some(java::create_language()),
        Language::Go => Some(go::create_language()),
        Language::C => Some(c::create_language()),
        Language::Cpp => Some(cpp::create_language()),
        Language::Rust => Some(rust::create_language()),
        Language::Ruby
        | Language::Php
        | Language::CSharp
        | Language::Swift
        | Language::Kotlin
        | Language::Scala
        | Language::Zig
        | Language::Lua
        | Language::Bash => None,
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
    fn test_has_rules_rust_returns_true() {
        assert!(has_rules(Language::Rust));
    }

    #[test]
    fn test_has_rules_go_returns_true() {
        assert!(has_rules(Language::Go));
    }

    #[test]
    fn test_has_rules_c_returns_true() {
        assert!(has_rules(Language::C));
    }

    #[test]
    fn test_has_rules_cpp_returns_true() {
        assert!(has_rules(Language::Cpp));
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
    fn test_language_config_go_loads_successfully() {
        let result = language_config(Language::Go);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "Go language config should load");
    }

    #[test]
    fn test_language_config_c_loads_successfully() {
        let result = language_config(Language::C);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "C language config should load");
    }

    #[test]
    fn test_language_config_rust_loads_successfully() {
        let result = language_config(Language::Rust);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "Rust language config should load");
    }

    #[test]
    fn test_language_config_cpp_loads_successfully() {
        let result = language_config(Language::Cpp);
        assert!(result.is_some());
        assert!(
            result.unwrap().is_ok(),
            "C++ language config should load"
        );
    }

    // -----------------------------------------------------------------------
    // Tier 2: has_rules returns false, language_config returns None
    // -----------------------------------------------------------------------

    #[test]
    fn test_has_rules_tier2_returns_false() {
        let tier2 = [
            Language::Ruby,
            Language::Php,
            Language::CSharp,
            Language::Swift,
            Language::Kotlin,
            Language::Scala,
            Language::Zig,
            Language::Lua,
            Language::Bash,
        ];
        for lang in tier2 {
            assert!(!has_rules(lang), "expected has_rules({lang:?}) = false");
        }
    }

    #[test]
    fn test_language_config_tier2_returns_none() {
        let tier2 = [
            Language::Ruby,
            Language::Php,
            Language::CSharp,
            Language::Swift,
            Language::Kotlin,
            Language::Scala,
            Language::Zig,
            Language::Lua,
            Language::Bash,
        ];
        for lang in tier2 {
            assert!(
                language_config(lang).is_none(),
                "expected language_config({lang:?}) = None"
            );
        }
    }
}
