//! Per-language stack graph rule configuration.
//!
//! Each supported language has a vendored TSG (tree-sitter graph) rule file that
//! describes how AST nodes map to stack graph nodes. This module provides a
//! factory for creating `StackGraphLanguage` instances and a predicate for
//! checking which languages have rules available.

pub mod c;
pub mod cpp;
pub mod csharp;
pub mod go;
pub mod java;
pub mod javascript;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod typescript;

use std::sync::Arc;

use codequery_core::Language;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error;
use crate::plugin_rules;

/// Check if a language has stack graph rules available.
///
/// Checks compiled-in rules first (fast path), then falls back to checking
/// the plugin directory for installed `stack-graphs.tsg` files.
#[must_use]
pub fn has_rules(lang: Language) -> bool {
    has_compiled_rules(lang) || plugin_rules::has_plugin_rules(lang.name())
}

/// Check if a language has stack graph rules available by name.
///
/// Like [`has_rules`] but accepts a language name string, enabling
/// support for runtime languages without a `Language` enum variant.
#[must_use]
pub fn has_rules_by_name(name: &str) -> bool {
    if let Some(lang) = Language::from_name(name) {
        has_compiled_rules(lang) || plugin_rules::has_plugin_rules(name)
    } else {
        plugin_rules::has_plugin_rules(name)
    }
}

/// Check if a language has compiled-in stack graph rules.
fn has_compiled_rules(lang: Language) -> bool {
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
            | Language::Ruby
            | Language::CSharp
    )
}

/// Create a `StackGraphLanguage` for the given language.
///
/// Tries compiled-in rules first, then falls back to loading from the plugin
/// directory. Returns `None` if no rules are available from either source.
#[must_use]
pub fn language_config(lang: Language) -> Option<error::Result<StackGraphLanguage>> {
    compiled_language_config(lang)
}

/// Get a `StackGraphLanguage` for a language, checking both compiled-in and plugin sources.
///
/// Returns an `Arc` because plugin-loaded rules are cached at process level.
/// Compiled-in rules are wrapped in a fresh `Arc`.
pub fn get_stack_graph_language(name: &str) -> Option<error::Result<Arc<StackGraphLanguage>>> {
    // Try compiled-in via Language enum
    if let Some(lang) = Language::from_name(name) {
        if let Some(result) = compiled_language_config(lang) {
            return Some(result.map(Arc::new));
        }
    }

    // Plugin fallback
    plugin_rules::load_plugin_rules(name)
}

/// Dispatch to compiled-in language rules.
fn compiled_language_config(lang: Language) -> Option<error::Result<StackGraphLanguage>> {
    match lang {
        Language::Python => Some(python::create_language()),
        Language::TypeScript => Some(typescript::create_language()),
        Language::JavaScript => Some(javascript::create_language()),
        Language::Java => Some(java::create_language()),
        Language::Go => Some(go::create_language()),
        Language::C => Some(c::create_language()),
        Language::Cpp => Some(cpp::create_language()),
        Language::Rust => Some(rust::create_language()),
        Language::Ruby => Some(ruby::create_language()),
        Language::CSharp => Some(csharp::create_language()),
        Language::Php
        | Language::Swift
        | Language::Kotlin
        | Language::Scala
        | Language::Zig
        | Language::Lua
        | Language::Bash
        | Language::Html
        | Language::Css
        | Language::Json
        | Language::Yaml
        | Language::Toml => None,
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
        assert!(result.unwrap().is_ok(), "C++ language config should load");
    }

    #[test]
    fn test_has_rules_ruby_returns_true() {
        assert!(has_rules(Language::Ruby));
    }

    #[test]
    fn test_language_config_ruby_loads_successfully() {
        let result = language_config(Language::Ruby);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "Ruby language config should load");
    }

    // -----------------------------------------------------------------------
    // Tier 2: has_rules returns false, language_config returns None
    // -----------------------------------------------------------------------

    #[test]
    fn test_has_rules_csharp_returns_true() {
        assert!(has_rules(Language::CSharp));
    }

    #[test]
    fn test_language_config_csharp_loads_successfully() {
        let result = language_config(Language::CSharp);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok(), "C# language config should load");
    }

    #[test]
    fn test_has_rules_tier2_returns_false() {
        let tier2 = [
            Language::Php,
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
            Language::Php,
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
