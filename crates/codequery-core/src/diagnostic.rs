//! Diagnostic types for syntax errors and language server messages.

use std::path::PathBuf;

use serde::Serialize;

/// A diagnostic message from either tree-sitter or a language server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// The file containing the diagnostic.
    pub file: PathBuf,
    /// Start line (1-based).
    pub line: usize,
    /// Start column (0-based).
    pub column: usize,
    /// End line (1-based).
    pub end_line: usize,
    /// End column (0-based).
    pub end_column: usize,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// Where this diagnostic came from.
    pub source: DiagnosticSource,
    /// Optional error code from the language server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// A fatal error.
    Error,
    /// A warning that does not prevent compilation.
    Warning,
    /// Informational message.
    Information,
    /// A hint or suggestion.
    Hint,
}

/// The origin of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSource {
    /// Tree-sitter parse error (syntax only).
    Syntax,
    /// Language server diagnostic (semantic).
    Lsp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_serializes_to_json() {
        let diag = Diagnostic {
            file: PathBuf::from("src/main.rs"),
            line: 42,
            column: 8,
            end_line: 42,
            end_column: 15,
            severity: DiagnosticSeverity::Error,
            message: "unexpected token".to_string(),
            source: DiagnosticSource::Syntax,
            code: None,
        };
        let json = serde_json::to_string(&diag).unwrap();
        assert!(json.contains("\"severity\":\"error\""));
        assert!(json.contains("\"source\":\"syntax\""));
    }

    #[test]
    fn severity_ordering() {
        assert!(DiagnosticSeverity::Error < DiagnosticSeverity::Warning);
        assert!(DiagnosticSeverity::Warning < DiagnosticSeverity::Information);
        assert!(DiagnosticSeverity::Information < DiagnosticSeverity::Hint);
    }
}
