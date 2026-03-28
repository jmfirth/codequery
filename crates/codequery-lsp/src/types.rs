//! Minimal LSP types for codequery.
//!
//! These types represent the subset of the Language Server Protocol that cq
//! actually uses. They are not a complete LSP type library — only the types
//! needed for definition lookup, references, hover, and document synchronization
//! are included.

use serde::{Deserialize, Serialize};

/// Parameters for the `initialize` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The process ID of the parent process that started the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,

    /// The root URI of the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_uri: Option<String>,

    /// The capabilities provided by the client.
    pub capabilities: ClientCapabilities,
}

/// Client capabilities sent during initialization.
///
/// Kept minimal — cq does not need to advertise rich editor capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilities {
    /// Text document specific client capabilities.
    #[serde(rename = "textDocument", skip_serializing_if = "Option::is_none")]
    pub text_document: Option<serde_json::Value>,
}

/// Server capabilities returned from the `initialize` response.
///
/// Stored as opaque JSON since cq only needs to know whether specific
/// providers are available, not parse every capability in detail.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Whether the server provides definition support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<serde_json::Value>,

    /// Whether the server provides references support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<serde_json::Value>,

    /// Whether the server provides hover support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<serde_json::Value>,

    /// Whether the server provides document symbol support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_symbol_provider: Option<serde_json::Value>,
}

/// Identifies a text document by its URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TextDocumentIdentifier {
    /// The text document's URI (e.g., `file:///path/to/file.rs`).
    pub uri: String,
}

/// A text document item with content, used for `didOpen` notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    /// The text document's URI.
    pub uri: String,

    /// The text document's language identifier (e.g., "rust", "python").
    pub language_id: String,

    /// The version number of this document (increases after each change).
    pub version: i32,

    /// The content of the opened text document.
    pub text: String,
}

/// A position in a text document, expressed as zero-based line and character offset.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Position {
    /// Zero-based line number.
    pub line: u32,

    /// Zero-based character offset on the line (UTF-16 code units in LSP spec).
    pub character: u32,
}

impl Position {
    /// Creates a new position at the given line and character.
    #[must_use]
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a text document, expressed as a start and end position.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Range {
    /// The range's start position (inclusive).
    pub start: Position,

    /// The range's end position (exclusive).
    pub end: Position,
}

impl Range {
    /// Creates a new range from start to end.
    #[must_use]
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// A location in a document, as returned by LSP responses.
///
/// Named `LspLocation` to avoid collision with `codequery_core::Location`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct LspLocation {
    /// The document URI.
    pub uri: String,

    /// The range within the document.
    pub range: Range,
}

/// Parameters for the `textDocument/references` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The position inside the text document.
    pub position: Position,

    /// Context for the reference request.
    pub context: ReferenceContext,
}

/// Context for a references request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    /// Include the declaration of the current symbol.
    pub include_declaration: bool,
}

/// Parameters for the `textDocument/definition` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The position inside the text document.
    pub position: Position,
}

/// Parameters for the `textDocument/hover` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoverParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The position inside the text document.
    pub position: Position,
}

/// Parameters for the `textDocument/didOpen` notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    /// The document that was opened.
    pub text_document: TextDocumentItem,
}

/// The content of a hover or markup response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarkupContent {
    /// The type of markup (e.g., "plaintext", "markdown").
    pub kind: String,

    /// The content string.
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_new() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_position_default_is_origin() {
        let pos = Position::default();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn test_range_new() {
        let range = Range::new(Position::new(1, 0), Position::new(1, 10));
        assert_eq!(range.start.line, 1);
        assert_eq!(range.end.character, 10);
    }

    #[test]
    fn test_initialize_params_serializes_with_camel_case() {
        let params = InitializeParams {
            process_id: Some(1234),
            root_uri: Some("file:///project".to_string()),
            capabilities: ClientCapabilities::default(),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["processId"], 1234);
        assert_eq!(json["rootUri"], "file:///project");
        assert!(json.get("capabilities").is_some());
    }

    #[test]
    fn test_initialize_params_omits_none_fields() {
        let params = InitializeParams {
            process_id: None,
            root_uri: None,
            capabilities: ClientCapabilities::default(),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert!(json.get("processId").is_none());
        assert!(json.get("rootUri").is_none());
    }

    #[test]
    fn test_server_capabilities_deserializes_from_partial_json() {
        let json = r#"{"definitionProvider":true,"hoverProvider":{"dynamicRegistration":false}}"#;
        let caps: ServerCapabilities = serde_json::from_str(json).unwrap();
        assert!(caps.definition_provider.is_some());
        assert!(caps.hover_provider.is_some());
        assert!(caps.references_provider.is_none());
        assert!(caps.document_symbol_provider.is_none());
    }

    #[test]
    fn test_text_document_identifier_serializes() {
        let id = TextDocumentIdentifier {
            uri: "file:///test.rs".to_string(),
        };
        let json = serde_json::to_value(&id).unwrap();
        assert_eq!(json["uri"], "file:///test.rs");
    }

    #[test]
    fn test_text_document_item_serializes_with_camel_case() {
        let item = TextDocumentItem {
            uri: "file:///test.rs".to_string(),
            language_id: "rust".to_string(),
            version: 1,
            text: "fn main() {}".to_string(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["languageId"], "rust");
        assert_eq!(json["version"], 1);
    }

    #[test]
    fn test_lsp_location_roundtrip() {
        let loc = LspLocation {
            uri: "file:///src/main.rs".to_string(),
            range: Range::new(Position::new(5, 0), Position::new(5, 10)),
        };
        let json = serde_json::to_string(&loc).unwrap();
        let parsed: LspLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, parsed);
    }

    #[test]
    fn test_definition_params_serializes() {
        let params = DefinitionParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position::new(10, 5),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["textDocument"]["uri"], "file:///test.rs");
        assert_eq!(json["position"]["line"], 10);
        assert_eq!(json["position"]["character"], 5);
    }

    #[test]
    fn test_reference_params_serializes() {
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position::new(3, 7),
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["context"]["includeDeclaration"], true);
    }

    #[test]
    fn test_hover_params_serializes() {
        let params = HoverParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position::new(0, 0),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["textDocument"]["uri"], "file:///test.rs");
    }

    #[test]
    fn test_did_open_params_serializes() {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: "file:///test.py".to_string(),
                language_id: "python".to_string(),
                version: 1,
                text: "print('hello')".to_string(),
            },
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["textDocument"]["languageId"], "python");
        assert_eq!(json["textDocument"]["text"], "print('hello')");
    }

    #[test]
    fn test_markup_content_serializes() {
        let content = MarkupContent {
            kind: "markdown".to_string(),
            value: "```rust\nfn main() {}\n```".to_string(),
        };
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["kind"], "markdown");
    }

    #[test]
    fn test_position_equality() {
        let a = Position::new(5, 3);
        let b = Position::new(5, 3);
        let c = Position::new(5, 4);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_range_equality() {
        let a = Range::new(Position::new(1, 0), Position::new(1, 10));
        let b = Range::new(Position::new(1, 0), Position::new(1, 10));
        let c = Range::new(Position::new(1, 0), Position::new(2, 0));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_client_capabilities_default_is_empty() {
        let caps = ClientCapabilities::default();
        assert!(caps.text_document.is_none());
    }

    #[test]
    fn test_server_capabilities_default_is_empty() {
        let caps = ServerCapabilities::default();
        assert!(caps.definition_provider.is_none());
        assert!(caps.references_provider.is_none());
        assert!(caps.hover_provider.is_none());
        assert!(caps.document_symbol_provider.is_none());
    }
}
