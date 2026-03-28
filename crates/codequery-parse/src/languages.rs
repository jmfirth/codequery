//! Per-language symbol extraction modules.
//!
//! Each supported language has a module that implements [`LanguageExtractor`]
//! for language-specific symbol extraction from tree-sitter ASTs.

use std::path::Path;

use codequery_core::Symbol;

pub mod bash;
pub mod c;
pub mod cpp;
pub mod csharp;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod lua;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod swift;
pub mod typescript;
pub mod zig;

/// Trait for language-specific symbol extraction from tree-sitter ASTs.
///
/// Each language module implements this trait on a zero-sized struct,
/// providing the extraction logic for that language's AST node types.
pub trait LanguageExtractor {
    /// Extract all symbol definitions from a parsed source file.
    ///
    /// # Arguments
    /// * `source` — the source text (needed to extract node text via byte ranges)
    /// * `tree` — the parsed tree-sitter tree
    /// * `file` — the file path (stored in each `Symbol` for output)
    fn extract_symbols(source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol>;
}
