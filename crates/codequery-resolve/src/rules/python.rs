//! Stack graph rules for Python.

use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error::ResolveError;

/// Vendored TSG source for Python stack graph construction.
pub const TSG_SOURCE: &str = include_str!("../../tsg/python/stack-graphs.tsg");

/// Create a `StackGraphLanguage` for Python.
///
/// Loads the vendored TSG rules and the Python tree-sitter grammar.
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the TSG rules fail to parse.
pub fn create_language() -> crate::error::Result<StackGraphLanguage> {
    let grammar: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    StackGraphLanguage::from_str(grammar, TSG_SOURCE)
        .map_err(|e| ResolveError::RuleLoadError(format!("python: {e}")))
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
            "failed to create Python language: {}",
            result.err().map_or(String::new(), |e| e.to_string())
        );
    }
}
