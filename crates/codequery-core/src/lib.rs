//! Core types and project utilities for codequery.
//!
//! This crate defines the shared data types — symbols, locations, errors — that
//! all other codequery crates depend on. It sits at the bottom of the dependency
//! graph and owns no parsing or output logic.

#![warn(clippy::pedantic)]

pub mod discovery;
pub mod error;
pub mod path_utils;
pub mod project;
pub mod query;
pub mod reference;
pub mod symbol;

pub use discovery::{discover_files, language_for_file, Language};
pub use error::{CoreError, Result};
pub use path_utils::resolve_display_path;
pub use project::{detect_project_root, detect_project_root_or};
pub use query::{Completeness, QueryResult, Resolution};
pub use reference::{Reference, ReferenceKind};
pub use symbol::{Location, Symbol, SymbolKind, Visibility};
