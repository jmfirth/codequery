//! Error types for the codequery-core crate.

use std::path::PathBuf;

/// Errors that can occur in core codequery operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// No project root (e.g., `Cargo.toml`, `package.json`) found from the given path.
    #[error("no project root found from {0}")]
    ProjectNotFound(PathBuf),

    /// No source files found in the given directory.
    #[error("no source files found in {0}")]
    NoSourceFiles(PathBuf),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A path-related error.
    #[error("{0}")]
    Path(String),
}

/// A specialized `Result` type for core codequery operations.
pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_core_error_project_not_found_produces_expected_message() {
        let err = CoreError::ProjectNotFound(PathBuf::from("/tmp/missing"));
        assert_eq!(err.to_string(), "no project root found from /tmp/missing");
    }

    #[test]
    fn test_core_error_io_wraps_std_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err = CoreError::from(io_err);
        assert_eq!(err.to_string(), "file not found");
        // Verify it's the Io variant
        assert!(matches!(err, CoreError::Io(_)));
    }

    #[test]
    fn test_core_error_no_source_files_message() {
        let err = CoreError::NoSourceFiles(PathBuf::from("/tmp/empty"));
        assert_eq!(err.to_string(), "no source files found in /tmp/empty");
    }

    #[test]
    fn test_core_error_path_message() {
        let err = CoreError::Path("invalid path encoding".to_string());
        assert_eq!(err.to_string(), "invalid path encoding");
    }

    #[test]
    fn test_result_alias_works_with_ok() {
        let result: Result<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_alias_works_with_err() {
        let result: Result<i32> = Err(CoreError::Path("bad".to_string()));
        assert!(result.is_err());
    }
}
