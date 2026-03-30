//! Tree-sitter parsing infrastructure for codequery.
//!
//! This crate provides language-specific parsers that produce tree-sitter
//! parse trees from source code, and symbol extraction that dispatches to
//! per-language extractors. It sits in the "tree-sitter parse" stage
//! of the query pipeline.

#![warn(clippy::pedantic)]

pub mod error;
pub mod extract;
pub mod extract_engine;
pub mod imports;
pub mod languages;
pub mod parser;
pub mod runtime_grammar;
pub mod search;
pub mod wasm_loader;

pub use error::{ParseError, Result};
pub use extract::extract_symbols;
pub use extract_engine::{
    extract_with_config, extract_with_config_uncached, validate_config, CompiledExtractor,
};
pub use imports::{extract_imports, ImportInfo};
#[cfg(feature = "lang-rust")]
pub use languages::rust::{extract_body, extract_signature};
pub use parser::{compiled_grammar, grammar_for_language, Parser, RustParser};
pub use runtime_grammar::{list_runtime_grammars, load_runtime_grammar};
pub use search::{search_file, search_file_raw, SearchMatch};
pub use wasm_loader::{discover_wasm_grammars, find_wasm_grammar, WasmGrammarInfo};
