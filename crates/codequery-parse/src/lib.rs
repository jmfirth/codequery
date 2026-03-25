//! Tree-sitter parsing infrastructure for codequery.
//!
//! This crate provides language-specific parsers that produce tree-sitter
//! parse trees from source code. It sits in the "tree-sitter parse" stage
//! of the query pipeline.

#![warn(clippy::pedantic)]

pub mod error;
pub mod parser;

pub use error::{ParseError, Result};
pub use parser::RustParser;
