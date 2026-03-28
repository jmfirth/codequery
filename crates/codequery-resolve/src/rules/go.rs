//! Stack graph rules for Go.

use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error::ResolveError;

/// Custom TSG source for Go stack graph construction.
pub const TSG_SOURCE: &str = include_str!("../../tsg/go/stack-graphs.tsg");

/// Create a `StackGraphLanguage` for Go.
///
/// Loads the custom TSG rules and the Go tree-sitter grammar.
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the TSG rules fail to parse.
pub fn create_language() -> crate::error::Result<StackGraphLanguage> {
    let grammar: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    StackGraphLanguage::from_str(grammar, TSG_SOURCE)
        .map_err(|e| ResolveError::RuleLoadError(format!("go: {e}")))
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
            "failed to create Go language: {}",
            result.err().map_or(String::new(), |e| e.to_string())
        );
    }
}
