//! Parallel file scanning, grep pre-filter, symbol indexing, and reference
//! extraction for codequery.
//!
//! This crate provides the indexing infrastructure for wide commands (refs,
//! callers, symbols, tree). It discovers source files, applies optional text
//! pre-filters, parses files in parallel with rayon to extract symbols, and
//! builds an in-memory symbol index with reference extraction.

#![warn(clippy::pedantic)]

pub mod error;
pub mod grep;
pub mod index;
pub mod refs;
pub mod scanner;

pub use error::{IndexError, Result};
pub use grep::{file_contains_word, filter_files};
pub use index::SymbolIndex;
pub use refs::extract_references;
pub use scanner::{scan_project, scan_with_filter, FileSymbols};
