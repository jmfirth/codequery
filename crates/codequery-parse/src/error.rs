//! Error types for the codequery-parse crate.

/// Errors that can occur during tree-sitter parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The language grammar failed to load.
    #[error("failed to load language grammar: {0}")]
    LanguageError(String),

    /// An I/O error occurred reading a source file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Tree-sitter returned no tree (language not set or internal failure).
    #[error("tree-sitter parse returned no tree for: {0}")]
    ParseFailed(String),

    /// A tree-sitter query failed to compile.
    #[error("invalid tree-sitter query: {0}")]
    QueryError(String),

    /// A search pattern could not be parsed as valid source code.
    #[error("pattern failed to parse: {0}")]
    PatternError(String),
}

/// Convenience result type for parse operations.
pub type Result<T> = std::result::Result<T, ParseError>;
