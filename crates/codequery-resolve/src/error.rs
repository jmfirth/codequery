//! Error types for the codequery-resolve crate.

/// Errors that can occur during stack graph resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// Failed to load TSG rules for a language.
    #[error("failed to load stack graph rules: {0}")]
    RuleLoadError(String),
}

/// A specialized `Result` type for resolve operations.
pub type Result<T> = std::result::Result<T, ResolveError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_error_rule_load_message() {
        let err = ResolveError::RuleLoadError("python: invalid syntax".to_string());
        assert_eq!(
            err.to_string(),
            "failed to load stack graph rules: python: invalid syntax"
        );
    }

    #[test]
    fn test_result_alias_works_with_ok() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_alias_works_with_err() {
        let result: Result<i32> = Err(ResolveError::RuleLoadError("bad".to_string()));
        assert!(result.is_err());
    }
}
