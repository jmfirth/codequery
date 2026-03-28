//! LSP integration types for codequery.
//!
//! This crate provides a minimal subset of LSP (Language Server Protocol) types
//! and JSON-RPC protocol primitives needed by codequery to communicate with
//! language servers. It is not a full LSP library — only the types cq actually
//! uses are defined here.

#![warn(clippy::pedantic)]

pub mod config;
pub mod daemon;
pub mod error;
pub mod oneshot;
pub mod pid;
pub mod protocol;
pub mod queries;
pub mod server;
pub mod socket;
pub mod transport;
pub mod types;

pub use config::{LanguageServerRegistry, ServerConfig};
pub use daemon::Daemon;
pub use error::{LspError, Result};
pub use oneshot::{
    semantic_definition, semantic_definition_with_wait, semantic_refs, semantic_refs_with_wait,
};
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
pub use queries::{path_to_uri, uri_to_path};
pub use server::LspServer;
pub use socket::{DaemonRequest, DaemonResponse, ServerInfo};
pub use transport::StdioTransport;
pub use types::{
    ClientCapabilities, DefinitionParams, DidOpenTextDocumentParams, HoverParams, InitializeParams,
    LocationLink, LspLocation, MarkupContent, Position, Range, ReferenceParams, ServerCapabilities,
    TextDocumentIdentifier, TextDocumentItem,
};
