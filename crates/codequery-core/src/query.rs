//! Query result wrapper with resolution and completeness metadata.
//!
//! Every command result is wrapped in `QueryResult<T>` to communicate how the
//! result was obtained (syntactic vs. resolved) and whether it covers all possible
//! matches (exhaustive vs. best-effort).

use std::fmt;

use serde::Serialize;

/// How the results were obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// Tree-sitter name matching (Phase 1 default).
    Syntactic,
    /// Stack graph scope resolution (Phase 2).
    Resolved,
    /// LSP type resolution (future).
    Semantic,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntactic => write!(f, "syntactic"),
            Self::Resolved => write!(f, "resolved"),
            Self::Semantic => write!(f, "semantic"),
        }
    }
}

/// Whether the result set is complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    /// All possible matches were found.
    Exhaustive,
    /// Some matches may be missing (e.g., due to parse errors or scope limits).
    BestEffort,
}

impl fmt::Display for Completeness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhaustive => write!(f, "exhaustive"),
            Self::BestEffort => write!(f, "best_effort"),
        }
    }
}

/// Wrapper for command results with precision metadata.
///
/// The `data` field is flattened into the top-level JSON object, so the data's
/// fields appear alongside `resolution`, `completeness`, and `note`.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResult<T: Serialize> {
    /// How the results were obtained.
    pub resolution: Resolution,
    /// Whether the result set is complete.
    pub completeness: Completeness,
    /// Optional note explaining limitations or caveats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The actual result data (flattened into the top-level JSON).
    #[serde(flatten)]
    pub data: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize)]
    struct SymbolList {
        symbols: Vec<String>,
    }

    #[test]
    fn test_query_result_wraps_data_with_metadata() {
        let result = QueryResult {
            resolution: Resolution::Syntactic,
            completeness: Completeness::Exhaustive,
            note: None,
            data: SymbolList {
                symbols: vec!["foo".to_string()],
            },
        };
        assert_eq!(result.resolution, Resolution::Syntactic);
        assert_eq!(result.completeness, Completeness::Exhaustive);
        assert!(result.note.is_none());
        assert_eq!(result.data.symbols.len(), 1);
    }

    #[test]
    fn test_query_result_serializes_with_flattened_data() {
        let result = QueryResult {
            resolution: Resolution::Syntactic,
            completeness: Completeness::BestEffort,
            note: Some("some files had parse errors".to_string()),
            data: SymbolList {
                symbols: vec!["foo".to_string(), "bar".to_string()],
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        // Metadata fields at top level
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "best_effort");
        assert_eq!(json["note"], "some files had parse errors");
        // Data fields flattened to top level (not nested under "data")
        assert_eq!(json["symbols"], serde_json::json!(["foo", "bar"]));
        assert!(json.get("data").is_none());
    }

    #[test]
    fn test_query_result_note_omitted_when_none() {
        let result = QueryResult {
            resolution: Resolution::Syntactic,
            completeness: Completeness::Exhaustive,
            note: None,
            data: SymbolList { symbols: vec![] },
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("note").is_none());
    }

    #[test]
    fn test_resolution_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_value(Resolution::Syntactic).unwrap(),
            "syntactic"
        );
        assert_eq!(
            serde_json::to_value(Resolution::Resolved).unwrap(),
            "resolved"
        );
        assert_eq!(
            serde_json::to_value(Resolution::Semantic).unwrap(),
            "semantic"
        );
    }

    #[test]
    fn test_completeness_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_value(Completeness::Exhaustive).unwrap(),
            "exhaustive"
        );
        assert_eq!(
            serde_json::to_value(Completeness::BestEffort).unwrap(),
            "best_effort"
        );
    }
}
