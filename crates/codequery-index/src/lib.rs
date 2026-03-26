//! Parallel file scanning and grep pre-filter for codequery.
//!
//! This crate provides the indexing infrastructure for wide commands (refs,
//! callers, symbols, tree). It discovers source files, applies optional text
//! pre-filters, and parses files in parallel with rayon to extract symbols.

#![warn(clippy::pedantic)]

pub mod error;
pub mod grep;
pub mod scanner;

pub use error::{IndexError, Result};
pub use grep::{file_contains_word, filter_files};
pub use scanner::{scan_project, scan_with_filter, FileSymbols};
