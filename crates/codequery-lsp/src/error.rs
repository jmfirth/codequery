//! Error types for the codequery-lsp crate.

use std::time::Duration;

/// Errors that can occur during LSP server communication.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    /// The requested language server binary was not found on the system.
    #[error("language server not found: {0}")]
    ServerNotFound(String),

    /// The language server process terminated unexpectedly.
    #[error("language server crashed: {0}")]
    ServerCrashed(String),

    /// The LSP initialize handshake failed.
    #[error("initialize failed: {0}")]
    InitializeFailed(String),

    /// An LSP request returned an error response.
    #[error("request failed: {method}: {message}")]
    RequestFailed {
        /// The LSP method that failed.
        method: String,
        /// The error message from the server.
        message: String,
    },

    /// An LSP request timed out waiting for a response.
    #[error("request timed out after {0:?}")]
    Timeout(Duration),

    /// The cq LSP daemon process is not running.
    #[error("daemon not running")]
    DaemonNotRunning,

    /// Failed to establish a connection to the language server.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// An I/O error occurred during server communication.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A JSON serialization or deserialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// A specialized `Result` type for LSP operations.
pub type Result<T> = std::result::Result<T, LspError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_lsp_error_server_not_found_message() {
        let err = LspError::ServerNotFound("rust-analyzer".to_string());
        assert_eq!(err.to_string(), "language server not found: rust-analyzer");
    }

    #[test]
    fn test_lsp_error_server_crashed_message() {
        let err = LspError::ServerCrashed("segmentation fault".to_string());
        assert_eq!(
            err.to_string(),
            "language server crashed: segmentation fault"
        );
    }

    #[test]
    fn test_lsp_error_initialize_failed_message() {
        let err = LspError::InitializeFailed("unsupported protocol version".to_string());
        assert_eq!(
            err.to_string(),
            "initialize failed: unsupported protocol version"
        );
    }

    #[test]
    fn test_lsp_error_request_failed_message() {
        let err = LspError::RequestFailed {
            method: "textDocument/definition".to_string(),
            message: "no definition found".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "request failed: textDocument/definition: no definition found"
        );
    }

    #[test]
    fn test_lsp_error_timeout_message() {
        let err = LspError::Timeout(Duration::from_secs(30));
        assert_eq!(err.to_string(), "request timed out after 30s");
    }

    #[test]
    fn test_lsp_error_daemon_not_running_message() {
        let err = LspError::DaemonNotRunning;
        assert_eq!(err.to_string(), "daemon not running");
    }

    #[test]
    fn test_lsp_error_connection_failed_message() {
        let err = LspError::ConnectionFailed("connection refused".to_string());
        assert_eq!(err.to_string(), "connection failed: connection refused");
    }

    #[test]
    fn test_lsp_error_io_wraps_std_io_error() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
        let err = LspError::from(io_err);
        assert_eq!(err.to_string(), "pipe broken");
        assert!(matches!(err, LspError::Io(_)));
    }

    #[test]
    fn test_lsp_error_json_wraps_serde_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = LspError::from(json_err);
        assert!(matches!(err, LspError::Json(_)));
    }

    #[test]
    fn test_result_alias_works_with_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(42));
    }

    #[test]
    fn test_result_alias_works_with_err() {
        let result: Result<i32> = Err(LspError::DaemonNotRunning);
        assert!(result.is_err());
    }
}
