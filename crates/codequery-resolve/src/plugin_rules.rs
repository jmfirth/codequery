//! Runtime loading of stack graph rules from installed language packages.
//!
//! Loads TSG rules from the plugin directory (`~/.local/share/cq/languages/<name>/`)
//! and combines them with a grammar from any source (compiled-in or WASM) to create
//! `StackGraphLanguage` instances for stack graph resolution.
//!
//! Results are cached at process level to avoid expensive re-compilation of TSG rules.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error::ResolveError;

/// Process-level cache of loaded `StackGraphLanguage` instances from plugins.
static PLUGIN_CACHE: LazyLock<Mutex<HashMap<String, Arc<StackGraphLanguage>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Check whether a language has stack graph rules installed in the plugin directory.
///
/// Looks for `~/.local/share/cq/languages/<name>/stack-graphs.tsg`.
#[must_use]
pub fn has_plugin_rules(name: &str) -> bool {
    let Some(dir) = codequery_core::dirs::languages_dir() else {
        return false;
    };
    dir.join(name).join("stack-graphs.tsg").exists()
}

/// Load stack graph rules from the plugin directory for a language.
///
/// Loads the TSG source from `~/.local/share/cq/languages/<name>/stack-graphs.tsg`
/// and the grammar via `codequery_parse::grammar_for_name()`. Results are cached
/// at process level.
///
/// Returns `None` if no TSG file is installed for this language.
/// Returns `Some(Err(...))` if the TSG file exists but fails to load.
pub fn load_plugin_rules(name: &str) -> Option<crate::error::Result<Arc<StackGraphLanguage>>> {
    // Check cache first
    {
        let cache = PLUGIN_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = cache.get(name) {
            return Some(Ok(Arc::clone(cached)));
        }
    }

    // Load TSG source from plugin directory
    let tsg_source = load_tsg_source(name)?;

    // Load grammar (compiled-in → runtime → WASM)
    let grammar = match codequery_parse::grammar_for_name(name) {
        Ok(g) => g,
        Err(e) => {
            return Some(Err(ResolveError::RuleLoadError(format!(
                "{name}: failed to load grammar: {e}"
            ))));
        }
    };

    // Create StackGraphLanguage
    let sgl = match StackGraphLanguage::from_str(grammar, &tsg_source) {
        Ok(sgl) => sgl,
        Err(e) => {
            return Some(Err(ResolveError::RuleLoadError(format!(
                "{name}: failed to load TSG rules: {e}"
            ))));
        }
    };

    let sgl = Arc::new(sgl);

    // Cache the result
    {
        let mut cache = PLUGIN_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(name.to_string(), Arc::clone(&sgl));
    }

    Some(Ok(sgl))
}

/// Load TSG source from the plugin directory.
///
/// For TypeScript, applies dialect preprocessing to keep only TypeScript sections
/// (discarding TSX-specific rules).
fn load_tsg_source(name: &str) -> Option<String> {
    let dir = codequery_core::dirs::languages_dir()?;
    let tsg_path = dir.join(name).join("stack-graphs.tsg");

    let source = std::fs::read_to_string(&tsg_path).ok()?;

    // TypeScript needs dialect preprocessing
    if name == "typescript" {
        Some(preprocess_dialect(&source, "typescript"))
    } else {
        Some(source)
    }
}

/// Preprocess a TSG source file, keeping only lines for the given dialect.
///
/// The format uses `; #dialect <name>` to start a conditional block and
/// `; #end` to close it. Lines outside any block are always kept. Lines
/// inside a block whose dialect does not match are replaced with blank lines
/// to preserve line numbers for error reporting.
#[must_use]
pub fn preprocess_dialect(source: &str, dialect: &str) -> String {
    let mut output = String::with_capacity(source.len());
    // None = outside any block (emit everything)
    // Some(true) = inside matching block (emit)
    // Some(false) = inside non-matching block (suppress)
    let mut filter: Option<bool> = None;

    for line in source.lines() {
        let trimmed = line.trim_start_matches(|c: char| c.is_whitespace() || c == ';');
        let trimmed = trimmed.trim();

        if let Some(rest) = trimmed.strip_prefix("#dialect") {
            let d = rest.trim();
            filter = Some(d == dialect);
        } else if trimmed.starts_with("#end") {
            filter = None;
        } else if filter.unwrap_or(true) {
            output.push_str(line);
        }
        output.push('\n');
    }

    output
}

/// Invalidate the plugin cache for a specific language.
///
/// Called after `cq grammar install` or `cq grammar remove` to ensure
/// the next resolution uses fresh rules.
pub fn invalidate_cache(name: &str) {
    let mut cache = PLUGIN_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.remove(name);
}

/// Invalidate the entire plugin cache.
pub fn invalidate_all() {
    let mut cache = PLUGIN_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_dialect_keeps_matching_sections() {
        let input = "\
line1
; #dialect typescript
ts_only
; #end
; #dialect tsx
tsx_only
; #end
line2
";
        let result = preprocess_dialect(input, "typescript");
        assert!(result.contains("line1"));
        assert!(result.contains("ts_only"));
        assert!(!result.contains("tsx_only"));
        assert!(result.contains("line2"));
    }

    #[test]
    fn test_preprocess_dialect_no_markers_passes_through() {
        let input = "line1\nline2\nline3\n";
        let result = preprocess_dialect(input, "typescript");
        assert_eq!(result, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_has_plugin_rules_returns_false_for_nonexistent() {
        assert!(!has_plugin_rules("nonexistent_language_xyz"));
    }
}
