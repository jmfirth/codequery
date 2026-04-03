//! Per-language stack graph rule configuration.
//!
//! All grammars are loaded at runtime via WASM plugins from the plugin directory
//! (`~/.local/share/cq/languages/<name>/`). This module provides a thin facade
//! over `plugin_rules` for checking availability and loading `StackGraphLanguage`
//! instances.

use std::sync::Arc;

use codequery_core::Language;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error;
use crate::plugin_rules;

/// Check if a language has stack graph rules available.
#[must_use]
pub fn has_rules(lang: Language) -> bool {
    plugin_rules::has_plugin_rules(lang.name())
}

/// Check if a language has stack graph rules available by name.
///
/// Like [`has_rules`] but accepts a language name string, enabling
/// support for runtime languages without a `Language` enum variant.
#[must_use]
pub fn has_rules_by_name(name: &str) -> bool {
    plugin_rules::has_plugin_rules(name)
}

/// Get a `StackGraphLanguage` for a language by name.
///
/// Returns an `Arc` because plugin-loaded rules are cached at process level.
#[must_use]
pub fn get_stack_graph_language(name: &str) -> Option<error::Result<Arc<StackGraphLanguage>>> {
    plugin_rules::load_plugin_rules(name)
}
