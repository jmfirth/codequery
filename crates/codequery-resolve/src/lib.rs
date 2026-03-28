//! Stack graph name resolution for codequery.
//!
//! This crate owns the construction and resolution of stack graphs, mapping
//! tree-sitter parse trees to name bindings via per-language TSG rules.
//! It sits between `codequery-parse` (which produces trees) and
//! `codequery-index` (which consumes resolution results).

#![warn(clippy::pedantic)]

pub mod error;
pub mod graph;
pub mod resolve;
pub mod resolver;
pub mod rules;
pub mod types;

pub use error::{ResolveError, Result};
pub use graph::{build_graph, build_graph_with_timeout, GraphResult, GraphWarning};
pub use resolve::{resolve_all_references, resolve_references, resolve_references_with_timeout};
pub use resolver::StackGraphResolver;
pub use rules::{has_rules, language_config};
pub use types::{Resolution, ResolutionResult, ResolvedReference};
