//! Stack graph rules for Rust (partial).
//!
//! Custom TSG rules covering what stack graphs can handle well for Rust:
//! `use` declarations, `mod` declarations, function/struct/enum/trait/const/static
//! definitions, `impl` blocks, `pub` visibility, and function body scopes.
//!
//! Explicitly not handled (beyond stack graph scope):
//! - Trait method resolution (which impl is used?) -- requires type inference
//! - Generic instantiation -- requires type checking
//! - Macro expansion -- requires the compiler
//! - Lifetime resolution
//! - Associated types

use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error::ResolveError;

/// Vendored TSG source for Rust stack graph construction.
pub const TSG_SOURCE: &str = include_str!("../../tsg/rust/stack-graphs.tsg");

/// Create a `StackGraphLanguage` for Rust.
///
/// Loads the custom TSG rules and the Rust tree-sitter grammar.
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the TSG rules fail to parse.
pub fn create_language() -> crate::error::Result<StackGraphLanguage> {
    let grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    StackGraphLanguage::from_str(grammar, TSG_SOURCE)
        .map_err(|e| ResolveError::RuleLoadError(format!("rust: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsg_source_is_non_empty() {
        assert!(!TSG_SOURCE.is_empty());
    }

    #[test]
    fn test_create_language_succeeds() {
        let result = create_language();
        assert!(
            result.is_ok(),
            "failed to create Rust language: {}",
            result.err().map_or(String::new(), |e| e.to_string())
        );
    }
}
