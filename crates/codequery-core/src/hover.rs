//! Hover information types for type info, docs, and signatures at a location.

use std::path::PathBuf;

use serde::Serialize;

/// Type and documentation information for a source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HoverInfo {
    /// The type of the symbol or expression, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_info: Option<String>,
    /// Documentation comment, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// The signature of the enclosing symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// The file containing the location.
    pub file: PathBuf,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (0-based).
    pub column: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_info_serializes_to_json() {
        let info = HoverInfo {
            type_info: Some("String".to_string()),
            docs: Some("A greeting function.".to_string()),
            signature: Some("fn greet(name: &str) -> String".to_string()),
            file: PathBuf::from("src/main.rs"),
            line: 10,
            column: 4,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"type_info\":\"String\""));
    }

    #[test]
    fn hover_info_skips_none_fields() {
        let info = HoverInfo {
            type_info: None,
            docs: None,
            signature: None,
            file: PathBuf::from("src/main.rs"),
            line: 1,
            column: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("type_info"));
        assert!(!json.contains("docs"));
        assert!(!json.contains("signature"));
    }
}
