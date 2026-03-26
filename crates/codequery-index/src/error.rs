//! Error types for the codequery-index crate.

/// Errors that can occur during indexing operations.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// An error from the core crate (file discovery, project detection).
    #[error(transparent)]
    Core(#[from] codequery_core::CoreError),

    /// An error from the parse crate (grammar loading, tree-sitter parsing).
    #[error(transparent)]
    Parse(#[from] codequery_parse::ParseError),

    /// An I/O error (file reads, memory mapping).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience result type for index operations.
pub type Result<T> = std::result::Result<T, IndexError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn test_index_error_from_core_error() {
        let core_err = codequery_core::CoreError::ProjectNotFound(PathBuf::from("/tmp"));
        let err = IndexError::from(core_err);
        assert!(matches!(err, IndexError::Core(_)));
        assert!(err.to_string().contains("/tmp"));
    }

    #[test]
    fn test_index_error_from_parse_error() {
        let parse_err = codequery_parse::ParseError::ParseFailed("test".to_string());
        let err = IndexError::from(parse_err);
        assert!(matches!(err, IndexError::Parse(_)));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_index_error_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "gone");
        let err = IndexError::from(io_err);
        assert!(matches!(err, IndexError::Io(_)));
        assert_eq!(err.to_string(), "gone");
    }
}
