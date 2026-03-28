//! Error types for the codequery-resolve crate.

/// Errors that can occur during stack graph resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// Failed to load TSG rules for a language.
    #[error("failed to load stack graph rules: {0}")]
    RuleLoadError(String),

    /// Resolution timed out.
    #[error("resolution timed out after {0:?}")]
    ResolutionTimeout(std::time::Duration),
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
        let Ok(value) = Ok(42) as Result<i32> else {
            panic!("expected Ok");
        };
        assert_eq!(value, 42);
    }

    #[test]
    fn test_result_alias_works_with_err() {
        let result: Result<i32> = Err(ResolveError::RuleLoadError("bad".to_string()));
        assert!(result.is_err());
    }
}
