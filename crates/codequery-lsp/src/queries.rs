//! LSP query operations for codequery.
//!
//! Provides high-level methods on [`LspServer`] for the four LSP operations
//! cq uses: opening documents, finding definitions, finding references, and
//! hover information. Also provides URI/path conversion helpers and
//! position coordinate translation.

use std::path::{Path, PathBuf};

use crate::error::{LspError, Result};
use crate::server::LspServer;
use crate::types::{
    DefinitionParams, DidOpenTextDocumentParams, HoverParams, LocationLink, LspLocation, Position,
    ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentItem,
};

/// Converts a filesystem path to an LSP `file://` URI.
///
/// Attempts to canonicalize the path first (resolving symlinks and relative
/// components). Falls back to the original path if canonicalization fails.
#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", absolute.display())
}

/// Converts an LSP `file://` URI back to a filesystem path.
///
/// Strips the `file://` prefix. Returns the path as-is if the URI does not
/// have the expected prefix.
#[must_use]
pub fn uri_to_path(uri: &str) -> PathBuf {
    let path_str = uri.strip_prefix("file://").unwrap_or(uri);
    PathBuf::from(path_str)
}

/// Converts a cq line number (1-based) to an LSP line number (0-based).
///
/// Returns 0 if the input is 0 (defensive, though cq lines should always be >= 1).
fn cq_line_to_lsp(line: usize) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    // Line numbers in source files will never exceed u32::MAX.
    {
        line.saturating_sub(1) as u32
    }
}

/// Converts a cq byte column (0-based) to an approximate LSP character offset (UTF-16).
///
/// For Phase 4, we approximate UTF-16 code units as byte offsets. This is
/// correct for ASCII and close enough for most code. A proper implementation
/// would require reading the source line and counting UTF-16 code units.
fn cq_column_to_lsp(column: usize) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    // Column offsets in source files will never exceed u32::MAX.
    {
        column as u32
    }
}

impl LspServer {
    /// Notifies the language server that a document has been opened.
    ///
    /// Sends a `textDocument/didOpen` notification with the document's URI,
    /// language identifier, and full source text. Tracks opened documents
    /// internally to avoid sending duplicate open notifications.
    ///
    /// If the document has already been opened, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification cannot be sent to the server.
    pub fn open_document(&mut self, path: &Path, source: &str, language_id: &str) -> Result<()> {
        let uri = path_to_uri(path);

        // Skip if already opened.
        if self.opened_docs.contains(&uri) {
            return Ok(());
        }

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
                text: source.to_string(),
            },
        };

        let params_value = serde_json::to_value(&params)?;
        self.transport_mut()
            .send_notification("textDocument/didOpen", params_value)?;

        self.opened_docs.insert(uri);
        Ok(())
    }

    /// Finds the definition(s) of a symbol at a position in a document.
    ///
    /// Sends a `textDocument/definition` request. The LSP spec allows three
    /// response shapes: a single `Location`, an array of `Location`, or an
    /// array of `LocationLink`. All three are handled and normalized to
    /// `Vec<LspLocation>`.
    ///
    /// Lines are 1-based and columns are 0-based byte offsets (cq convention).
    /// These are converted to LSP's 0-based line and UTF-16 character offsets.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub fn find_definition(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
    ) -> Result<Vec<LspLocation>> {
        let uri = path_to_uri(path);
        let params = DefinitionParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position::new(cq_line_to_lsp(line), cq_column_to_lsp(column)),
        };

        let params_value = serde_json::to_value(&params)?;
        let result = self
            .transport_mut()
            .send_request("textDocument/definition", params_value)?;

        parse_definition_response(&result)
    }

    /// Finds all references to a symbol at a position in a document.
    ///
    /// Sends a `textDocument/references` request. The response is an array of
    /// `Location` objects.
    ///
    /// Lines are 1-based and columns are 0-based byte offsets (cq convention).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub fn find_references(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
        include_declaration: bool,
    ) -> Result<Vec<LspLocation>> {
        let uri = path_to_uri(path);
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position::new(cq_line_to_lsp(line), cq_column_to_lsp(column)),
            context: ReferenceContext {
                include_declaration,
            },
        };

        let params_value = serde_json::to_value(&params)?;
        let result = self
            .transport_mut()
            .send_request("textDocument/references", params_value)?;

        parse_locations_response(&result)
    }

    /// Gets hover information at a position in a document.
    ///
    /// Sends a `textDocument/hover` request. The response may contain
    /// `MarkupContent`, a plain string, or a `MarkedString` array. The text
    /// content is extracted and returned as a `String`.
    ///
    /// Returns `None` if the server has no hover information for the position.
    ///
    /// Lines are 1-based and columns are 0-based byte offsets (cq convention).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub fn hover(&mut self, path: &Path, line: usize, column: usize) -> Result<Option<String>> {
        let uri = path_to_uri(path);
        let params = HoverParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position::new(cq_line_to_lsp(line), cq_column_to_lsp(column)),
        };

        let params_value = serde_json::to_value(&params)?;
        let result = self
            .transport_mut()
            .send_request("textDocument/hover", params_value)?;

        // Null response means no hover info.
        if result.is_null() {
            return Ok(None);
        }

        Ok(Some(parse_hover_contents(&result)))
    }
}

/// Parses the response from `textDocument/definition`.
///
/// Handles three possible response shapes:
/// 1. A single `Location` object.
/// 2. An array of `Location` objects.
/// 3. An array of `LocationLink` objects.
/// 4. Null (no results).
fn parse_definition_response(result: &serde_json::Value) -> Result<Vec<LspLocation>> {
    // Null means no results.
    if result.is_null() {
        return Ok(Vec::new());
    }

    // Try as array first (most common for multi-result).
    if let Some(arr) = result.as_array() {
        if arr.is_empty() {
            return Ok(Vec::new());
        }

        // Check if it's LocationLink[] (has targetUri) or Location[] (has uri).
        if arr[0].get("targetUri").is_some() {
            // LocationLink array.
            let links: Vec<LocationLink> = serde_json::from_value(result.clone()).map_err(|e| {
                LspError::ConnectionFailed(format!("failed to parse LocationLink[]: {e}"))
            })?;
            return Ok(links
                .into_iter()
                .map(|link| LspLocation {
                    uri: link.target_uri,
                    range: link.target_selection_range,
                })
                .collect());
        }

        // Location array.
        let locations: Vec<LspLocation> = serde_json::from_value(result.clone())
            .map_err(|e| LspError::ConnectionFailed(format!("failed to parse Location[]: {e}")))?;
        return Ok(locations);
    }

    // Single Location object.
    let location: LspLocation = serde_json::from_value(result.clone())
        .map_err(|e| LspError::ConnectionFailed(format!("failed to parse Location: {e}")))?;
    Ok(vec![location])
}

/// Parses a `Location[]` response (used by references).
fn parse_locations_response(result: &serde_json::Value) -> Result<Vec<LspLocation>> {
    if result.is_null() {
        return Ok(Vec::new());
    }

    let locations: Vec<LspLocation> = serde_json::from_value(result.clone())
        .map_err(|e| LspError::ConnectionFailed(format!("failed to parse Location[]: {e}")))?;
    Ok(locations)
}

/// Extracts text from the hover response's `contents` field.
///
/// Handles multiple formats:
/// - `MarkupContent` (object with `kind` and `value` fields)
/// - Plain string
/// - `MarkedString` (object with `language` and `value` fields)
/// - Array of `MarkedString` or strings
fn parse_hover_contents(result: &serde_json::Value) -> String {
    let contents = result.get("contents").unwrap_or(result);

    // MarkupContent or MarkedString: { kind/language: string, value: string }
    if let Some(value) = contents.get("value") {
        if let Some(text) = value.as_str() {
            return text.to_string();
        }
    }

    // Plain string.
    if let Some(text) = contents.as_str() {
        return text.to_string();
    }

    // Array of MarkedString or strings.
    if let Some(arr) = contents.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(text) = item.as_str() {
                parts.push(text.to_string());
            } else if let Some(value) = item.get("value") {
                if let Some(text) = value.as_str() {
                    parts.push(text.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return parts.join("\n\n");
        }
    }

    // Fallback: serialize the contents as a string.
    contents.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── path_to_uri tests ──────────────────────────────────────────

    #[test]
    fn test_path_to_uri_absolute_path() {
        let uri = path_to_uri(Path::new("/tmp/test-project"));
        assert!(uri.starts_with("file:///"));
        assert!(uri.contains("test-project"));
    }

    #[test]
    fn test_path_to_uri_resolves_relative() {
        let uri = path_to_uri(Path::new("."));
        assert!(uri.starts_with("file:///"));
    }

    // ─── uri_to_path tests ──────────────────────────────────────────

    #[test]
    fn test_uri_to_path_strips_file_prefix() {
        let path = uri_to_path("file:///usr/local/bin/test");
        assert_eq!(path, PathBuf::from("/usr/local/bin/test"));
    }

    #[test]
    fn test_uri_to_path_handles_missing_prefix() {
        let path = uri_to_path("/some/path");
        assert_eq!(path, PathBuf::from("/some/path"));
    }

    #[test]
    fn test_uri_to_path_preserves_spaces_and_special_chars() {
        let path = uri_to_path("file:///home/user/my project/file.rs");
        assert_eq!(path, PathBuf::from("/home/user/my project/file.rs"));
    }

    #[test]
    fn test_path_to_uri_roundtrip() {
        let original = Path::new("/tmp/roundtrip-test");
        let uri = path_to_uri(original);
        let recovered = uri_to_path(&uri);
        // The path may be canonicalized differently, but should contain the basename.
        assert!(
            recovered
                .to_str()
                .map_or(false, |s| s.contains("roundtrip-test")),
            "recovered path {:?} should contain 'roundtrip-test'",
            recovered
        );
    }

    // ─── coordinate conversion tests ────────────────────────────────

    #[test]
    fn test_cq_line_to_lsp_converts_1_based_to_0_based() {
        assert_eq!(cq_line_to_lsp(1), 0);
        assert_eq!(cq_line_to_lsp(10), 9);
        assert_eq!(cq_line_to_lsp(100), 99);
    }

    #[test]
    fn test_cq_line_to_lsp_zero_saturates() {
        assert_eq!(cq_line_to_lsp(0), 0);
    }

    #[test]
    fn test_cq_column_to_lsp_passes_through() {
        // Both are 0-based in Phase 4 (byte approx = UTF-16 for ASCII).
        assert_eq!(cq_column_to_lsp(0), 0);
        assert_eq!(cq_column_to_lsp(5), 5);
        assert_eq!(cq_column_to_lsp(42), 42);
    }

    // ─── parse_definition_response tests ────────────────────────────

    #[test]
    fn test_parse_definition_response_null() {
        let result = serde_json::Value::Null;
        let locs = parse_definition_response(&result).unwrap();
        assert!(locs.is_empty());
    }

    #[test]
    fn test_parse_definition_response_single_location() {
        let result = serde_json::json!({
            "uri": "file:///src/main.rs",
            "range": {
                "start": {"line": 10, "character": 4},
                "end": {"line": 10, "character": 12}
            }
        });
        let locs = parse_definition_response(&result).unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].uri, "file:///src/main.rs");
        assert_eq!(locs[0].range.start.line, 10);
        assert_eq!(locs[0].range.start.character, 4);
    }

    #[test]
    fn test_parse_definition_response_location_array() {
        let result = serde_json::json!([
            {
                "uri": "file:///src/a.rs",
                "range": {
                    "start": {"line": 5, "character": 0},
                    "end": {"line": 5, "character": 10}
                }
            },
            {
                "uri": "file:///src/b.rs",
                "range": {
                    "start": {"line": 20, "character": 2},
                    "end": {"line": 20, "character": 8}
                }
            }
        ]);
        let locs = parse_definition_response(&result).unwrap();
        assert_eq!(locs.len(), 2);
        assert_eq!(locs[0].uri, "file:///src/a.rs");
        assert_eq!(locs[1].uri, "file:///src/b.rs");
    }

    #[test]
    fn test_parse_definition_response_location_link_array() {
        let result = serde_json::json!([
            {
                "targetUri": "file:///src/lib.rs",
                "targetRange": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 5, "character": 1}
                },
                "targetSelectionRange": {
                    "start": {"line": 1, "character": 7},
                    "end": {"line": 1, "character": 15}
                }
            }
        ]);
        let locs = parse_definition_response(&result).unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].uri, "file:///src/lib.rs");
        // Should use targetSelectionRange for the location range.
        assert_eq!(locs[0].range.start.line, 1);
        assert_eq!(locs[0].range.start.character, 7);
    }

    #[test]
    fn test_parse_definition_response_empty_array() {
        let result = serde_json::json!([]);
        let locs = parse_definition_response(&result).unwrap();
        assert!(locs.is_empty());
    }

    // ─── parse_locations_response tests ─────────────────────────────

    #[test]
    fn test_parse_locations_response_null() {
        let result = serde_json::Value::Null;
        let locs = parse_locations_response(&result).unwrap();
        assert!(locs.is_empty());
    }

    #[test]
    fn test_parse_locations_response_array() {
        let result = serde_json::json!([
            {
                "uri": "file:///src/main.rs",
                "range": {
                    "start": {"line": 3, "character": 0},
                    "end": {"line": 3, "character": 10}
                }
            },
            {
                "uri": "file:///src/lib.rs",
                "range": {
                    "start": {"line": 15, "character": 4},
                    "end": {"line": 15, "character": 14}
                }
            }
        ]);
        let locs = parse_locations_response(&result).unwrap();
        assert_eq!(locs.len(), 2);
    }

    #[test]
    fn test_parse_locations_response_empty_array() {
        let result = serde_json::json!([]);
        let locs = parse_locations_response(&result).unwrap();
        assert!(locs.is_empty());
    }

    // ─── parse_hover_contents tests ─────────────────────────────────

    #[test]
    fn test_parse_hover_contents_markup_content() {
        let result = serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": "```rust\nfn main() {}\n```"
            }
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "```rust\nfn main() {}\n```");
    }

    #[test]
    fn test_parse_hover_contents_plain_string() {
        let result = serde_json::json!({
            "contents": "fn main()"
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "fn main()");
    }

    #[test]
    fn test_parse_hover_contents_marked_string() {
        let result = serde_json::json!({
            "contents": {
                "language": "rust",
                "value": "fn foo() -> i32"
            }
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "fn foo() -> i32");
    }

    #[test]
    fn test_parse_hover_contents_array_of_strings() {
        let result = serde_json::json!({
            "contents": ["first part", "second part"]
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "first part\n\nsecond part");
    }

    #[test]
    fn test_parse_hover_contents_array_of_marked_strings() {
        let result = serde_json::json!({
            "contents": [
                {"language": "rust", "value": "fn foo()"},
                "Documentation for foo"
            ]
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "fn foo()\n\nDocumentation for foo");
    }

    #[test]
    fn test_parse_hover_contents_fallback_serializes() {
        let result = serde_json::json!({
            "contents": 42
        });
        let text = parse_hover_contents(&result);
        assert_eq!(text, "42");
    }

    // ─── LocationLink type tests ────────────────────────────────────

    #[test]
    fn test_location_link_deserializes() {
        let json = serde_json::json!({
            "targetUri": "file:///test.rs",
            "targetRange": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 10, "character": 1}
            },
            "targetSelectionRange": {
                "start": {"line": 2, "character": 4},
                "end": {"line": 2, "character": 10}
            }
        });
        let link: LocationLink = serde_json::from_value(json).unwrap();
        assert_eq!(link.target_uri, "file:///test.rs");
        assert!(link.origin_selection_range.is_none());
    }

    #[test]
    fn test_location_link_with_origin_range() {
        let json = serde_json::json!({
            "originSelectionRange": {
                "start": {"line": 5, "character": 0},
                "end": {"line": 5, "character": 8}
            },
            "targetUri": "file:///target.rs",
            "targetRange": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 20, "character": 1}
            },
            "targetSelectionRange": {
                "start": {"line": 3, "character": 4},
                "end": {"line": 3, "character": 15}
            }
        });
        let link: LocationLink = serde_json::from_value(json).unwrap();
        assert!(link.origin_selection_range.is_some());
        assert_eq!(link.origin_selection_range.unwrap().start.line, 5);
    }
}
