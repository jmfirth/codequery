//! Tree-sitter parsing infrastructure for codequery.
//!
//! This crate provides language-specific parsers that produce tree-sitter
//! parse trees from source code, and symbol extraction that dispatches to
//! per-language extractors. It sits in the "tree-sitter parse" stage
//! of the query pipeline.

#![warn(clippy::pedantic)]

pub mod error;
pub mod extract;
pub mod languages;
pub mod parser;

pub use error::{ParseError, Result};
pub use extract::extract_symbols;
pub use parser::{Parser, RustParser};
