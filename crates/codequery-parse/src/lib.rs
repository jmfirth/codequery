//! Tree-sitter parsing infrastructure for codequery.
//!
//! This crate provides language-specific parsers that produce tree-sitter
//! parse trees from source code, and symbol extraction that dispatches to
//! per-language extractors. It sits in the "tree-sitter parse" stage
//! of the query pipeline.

#![warn(clippy::pedantic)]

pub mod diagnostics;
pub mod error;
pub mod extract;
pub mod extract_engine;
pub mod hierarchy;
pub mod imports;
pub mod languages;
pub mod parser;
pub mod runtime_grammar;
pub mod search;
pub mod types;
pub mod wasm_loader;

pub use diagnostics::extract_syntax_errors;
pub use error::{ParseError, Result};
pub use extract::{extract_symbols, extract_symbols_by_name};
pub use extract_engine::{
    extract_with_config, extract_with_config_uncached, validate_config, CompiledExtractor,
};
pub use hierarchy::{extract_supertype_relations, SupertypeRelation};
pub use imports::{extract_imports, ImportInfo};
pub use languages::rust::{extract_body, extract_signature};
pub use parser::{grammar_for_language, grammar_for_name, Parser, RustParser};
pub use runtime_grammar::{list_runtime_grammars, load_runtime_grammar};
pub use search::{search_file, SearchMatch};
pub use types::extract_type_at_position;
pub use wasm_loader::{discover_wasm_grammars, find_wasm_grammar, WasmGrammarInfo};
