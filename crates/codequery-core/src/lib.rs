//! Core types and project utilities for codequery.
//!
//! This crate defines the shared data types — symbols, locations, errors — that
//! all other codequery crates depend on. It sits at the bottom of the dependency
//! graph and owns no parsing or output logic.

#![warn(clippy::pedantic)]

pub mod callchain;
pub mod config;
pub mod diagnostic;
pub mod dirs;
pub mod discovery;
pub mod edit;
pub mod error;
pub mod extract_config;
pub mod hierarchy;
pub mod hover;
pub mod path_utils;
pub mod project;
pub mod query;
pub mod reference;
pub mod symbol;

/// Default GitHub release tag for grammar package downloads.
///
/// Grammar packages are decoupled from binary releases — they live under a
/// stable tag (e.g., `grammars-v1`) so that any binary version can download
/// the latest compatible grammars without waiting for a matching release.
pub const DEFAULT_GRAMMAR_RELEASE_TAG: &str = "grammars-v1";

pub use callchain::CallChainNode;
pub use config::{load_config, LspConfig, LspServerOverride, ProjectConfig};
pub use diagnostic::{Diagnostic, DiagnosticSeverity, DiagnosticSource};
pub use discovery::{
    discover_files, discover_files_with_config, language_for_file,
    language_for_file_with_overrides, language_name_for_file, wasm_name_for_language, Language,
};
pub use edit::{RenameResult, TextEdit};
pub use error::{CoreError, Result};
pub use extract_config::{load_extract_config, parse_symbol_kind, ExtractConfig, SymbolRule};
pub use hierarchy::{TypeHierarchyNode, TypeHierarchyResult};
pub use hover::HoverInfo;
pub use path_utils::resolve_display_path;
pub use project::{detect_project_root, detect_project_root_or};
pub use query::{Completeness, QueryResult, Resolution};
pub use reference::{Reference, ReferenceKind};
pub use symbol::{Location, Symbol, SymbolKind, Visibility};
