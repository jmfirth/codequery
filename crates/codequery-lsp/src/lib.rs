//! LSP integration types for codequery.
//!
//! This crate provides a minimal subset of LSP (Language Server Protocol) types
//! and JSON-RPC protocol primitives needed by codequery to communicate with
//! language servers. It is not a full LSP library — only the types cq actually
//! uses are defined here.

#![warn(clippy::pedantic)]

pub mod config;
pub mod error;
pub mod protocol;
pub mod server;
pub mod transport;
pub mod types;

pub use config::{LanguageServerRegistry, ServerConfig};
pub use error::{LspError, Result};
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
pub use server::LspServer;
pub use transport::StdioTransport;
pub use types::{
    ClientCapabilities, DefinitionParams, DidOpenTextDocumentParams, HoverParams, InitializeParams,
    LspLocation, MarkupContent, Position, Range, ReferenceParams, ServerCapabilities,
    TextDocumentIdentifier, TextDocumentItem,
};
