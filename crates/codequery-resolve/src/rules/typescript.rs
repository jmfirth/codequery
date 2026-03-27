//! Stack graph rules for TypeScript.
//!
//! The vendored TSG file contains both TypeScript and TSX dialect sections,
//! marked with `; #dialect typescript` / `; #dialect tsx` and `; #end`
//! directives. We preprocess the file at load time, keeping only the
//! TypeScript sections and discarding TSX-specific rules.

use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::error::ResolveError;

/// Vendored TSG source for TypeScript stack graph construction (unprocessed).
const TSG_SOURCE_RAW: &str = include_str!("../../tsg/typescript/stack-graphs.tsg");

/// Vendored builtins source for TypeScript (standard library type definitions).
pub const BUILTINS_SOURCE: &str = include_str!("../../tsg/typescript/builtins.ts");

/// Preprocess a TSG source file, keeping only lines for the given dialect.
///
/// The format uses `;  #dialect <name>` to start a conditional block and
/// `; #end` to close it. Lines outside any block are always kept. Lines
/// inside a block whose dialect does not match are replaced with blank lines
/// to preserve line numbers for error reporting.
fn preprocess_dialect(source: &str, dialect: &str) -> String {
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

/// Create a `StackGraphLanguage` for TypeScript.
///
/// Loads the vendored TSG rules (preprocessed for the TypeScript dialect)
/// and the TypeScript tree-sitter grammar.
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the TSG rules fail to parse.
pub fn create_language() -> crate::error::Result<StackGraphLanguage> {
    let tsg_source = preprocess_dialect(TSG_SOURCE_RAW, "typescript");
    let grammar: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    StackGraphLanguage::from_str(grammar, &tsg_source)
        .map_err(|e| ResolveError::RuleLoadError(format!("typescript: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsg_source_raw_is_non_empty() {
        assert!(!TSG_SOURCE_RAW.is_empty());
    }

    #[test]
    fn test_builtins_source_is_non_empty() {
        assert!(!BUILTINS_SOURCE.is_empty());
    }

    #[test]
    fn test_preprocess_keeps_typescript_sections() {
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
    fn test_preprocess_removes_tsx_sections_for_typescript() {
        let result = preprocess_dialect(TSG_SOURCE_RAW, "typescript");
        assert!(!result.contains("jsx_element"));
        assert!(result.contains("type_assertion"));
    }

    #[test]
    fn test_create_language_succeeds() {
        let result = create_language();
        assert!(
            result.is_ok(),
            "failed to create TypeScript language: {}",
            result.err().map_or(String::new(), |e| e.to_string())
        );
    }
}
