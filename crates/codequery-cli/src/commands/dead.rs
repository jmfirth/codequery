//! Dead code command: find symbols with zero references across the project.

use std::collections::HashSet;
use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, language_name_for_file, Symbol, SymbolKind,
    Visibility,
};
use codequery_index::{extract_references, scan_project_cached, SymbolIndex};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_dead;

/// Run the dead code command: find unreferenced symbols.
///
/// Scans the project for all symbols and all references, then reports
/// symbols whose name appears nowhere in the reference set. Focuses on
/// private symbols to minimize false positives (exported symbols may have
/// external callers).
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or scanning fails.
#[allow(clippy::too_many_arguments)]
// CLI command runners naturally take one parameter per flag
pub fn run(
    project: Option<&Path>,
    scope: Option<&Path>,
    kind: Option<&str>,
    limit: Option<usize>,
    mode: OutputMode,
    pretty: bool,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Parallel scan all source files
    let scan = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Build symbol index
    let index = SymbolIndex::from_scan(&scan);

    // 4. Collect all referenced symbol names across the project
    let mut referenced_names: HashSet<String> = HashSet::new();
    for file_entry in &scan {
        let absolute = project_root.join(&file_entry.file);
        let language = if let Some(lang) = language_for_file(&absolute) {
            lang
        } else if let Some(name) = language_name_for_file(&absolute) {
            match codequery_core::Language::from_name(&name) {
                Some(lang) => lang,
                None => continue,
            }
        } else {
            continue;
        };
        let refs = extract_references(
            &file_entry.source,
            &file_entry.tree,
            &file_entry.file,
            language,
        );
        for r in &refs {
            // Extract the referenced name from the context at the column position
            let line_text = r.context.as_str();
            if let Some(name) = extract_name_at_column(line_text, r.column) {
                referenced_names.insert(name);
            }
        }
    }

    // 5. Find symbols with zero references
    let kind_filter = kind.map(parse_kind).transpose()?;

    let mut dead_symbols: Vec<Symbol> = index
        .all_symbols()
        .iter()
        .filter(|sym| {
            // Apply kind filter
            if let Some(k) = kind_filter {
                if sym.kind != k {
                    return false;
                }
            }

            // Skip impl blocks — they're containers, not callable symbols
            if sym.kind == SymbolKind::Impl {
                return false;
            }

            // Skip test functions — they're called by the test runner
            if sym.kind == SymbolKind::Test {
                return false;
            }

            // Skip main — it's an entry point
            if sym.name == "main" {
                return false;
            }

            // A symbol is "dead" if its name doesn't appear in any reference
            !referenced_names.contains(&sym.name)
        })
        .cloned()
        .collect();

    // 6. Sort by file path, then by line number
    dead_symbols.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 7. Apply --limit if provided
    if let Some(limit) = limit {
        dead_symbols.truncate(limit);
    }

    // 8. Format and output
    let is_pub_warning = dead_symbols
        .iter()
        .any(|s| s.visibility == Visibility::Public);

    if dead_symbols.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_dead(&dead_symbols, is_pub_warning, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if dead_symbols.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

/// Extract the identifier name at a given column position in a source line.
fn extract_name_at_column(line: &str, column: usize) -> Option<String> {
    let bytes = line.as_bytes();
    if column >= bytes.len() {
        return None;
    }
    let start = column;
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if end > start {
        Some(line[start..end].to_string())
    } else {
        None
    }
}

/// Parse a `--kind` filter string into a `SymbolKind`.
fn parse_kind(s: &str) -> anyhow::Result<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" => Ok(SymbolKind::Function),
        "method" => Ok(SymbolKind::Method),
        "struct" => Ok(SymbolKind::Struct),
        "class" => Ok(SymbolKind::Class),
        "trait" => Ok(SymbolKind::Trait),
        "interface" => Ok(SymbolKind::Interface),
        "enum" => Ok(SymbolKind::Enum),
        "type" | "type_alias" => Ok(SymbolKind::Type),
        "const" | "constant" => Ok(SymbolKind::Const),
        "static" => Ok(SymbolKind::Static),
        "module" => Ok(SymbolKind::Module),
        _ => Err(anyhow::anyhow!(
            "unknown symbol kind: {s}. valid kinds: function, method, struct, class, \
             trait, interface, enum, type, const, static, module"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name_at_column_basic() {
        assert_eq!(
            extract_name_at_column("    greet();", 4),
            Some("greet".to_string())
        );
    }

    #[test]
    fn extract_name_at_column_with_underscores() {
        assert_eq!(
            extract_name_at_column("    my_func();", 4),
            Some("my_func".to_string())
        );
    }

    #[test]
    fn extract_name_at_column_out_of_bounds() {
        assert_eq!(extract_name_at_column("abc", 10), None);
    }

    #[test]
    fn extract_name_at_column_non_identifier() {
        assert_eq!(extract_name_at_column("  ();", 2), None);
    }

    #[test]
    fn parse_kind_valid() {
        assert_eq!(parse_kind("function").unwrap(), SymbolKind::Function);
        assert_eq!(parse_kind("struct").unwrap(), SymbolKind::Struct);
        assert_eq!(parse_kind("TRAIT").unwrap(), SymbolKind::Trait);
    }

    #[test]
    fn parse_kind_invalid() {
        assert!(parse_kind("unknown").is_err());
    }
}
