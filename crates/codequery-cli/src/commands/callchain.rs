//! Callchain command: multi-level call hierarchy tracing.
//!
//! Recursively finds callers of a symbol up to a configurable depth,
//! building a nested tree of call relationships.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, language_name_for_file, CallChainNode,
    ReferenceKind, SymbolKind,
};
use codequery_index::{extract_references, scan_project_cached, FileSymbols, SymbolIndex};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_callchain;

/// Run the callchain command: trace multi-level call hierarchy.
///
/// Finds callers of the target symbol, then callers of those callers,
/// recursively up to `depth` levels. Uses the project scan and reference
/// extraction infrastructure.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or scanning fails.
#[allow(clippy::too_many_arguments)]
// CLI command runners naturally take one parameter per flag
pub fn run(
    symbol: &str,
    depth: usize,
    project: Option<&Path>,
    scope: Option<&Path>,
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

    // 4. Build a project-wide caller map: symbol_name -> [(caller_name, caller_kind, file, line, col)]
    let caller_map = build_caller_map(&scan);

    // 5. Build the call chain tree
    let definitions = index.find_by_name(symbol);
    let (file, line, column, kind) = if let Some(def) = definitions.first() {
        (def.file.clone(), def.line, def.column, def.kind)
    } else {
        (
            std::path::PathBuf::from("<unknown>"),
            0,
            0,
            SymbolKind::Function,
        )
    };

    let mut visited = HashSet::new();
    let root = build_chain_node(
        symbol,
        kind,
        &file,
        line,
        column,
        &caller_map,
        &index,
        depth,
        &mut visited,
    );

    // 6. Format and output
    let has_callers = !root.callers.is_empty();

    if !has_callers && mode != OutputMode::Json {
        Ok(ExitCode::Success)
    } else {
        let output = format_callchain(&root, depth, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        Ok(ExitCode::Success)
    }
}

/// A caller entry: (`caller_name`, `caller_kind`, file, line, column)
type CallerEntry = (String, SymbolKind, std::path::PathBuf, usize, usize);

/// Build a map from symbol name to its callers across the project.
fn build_caller_map(scan: &[FileSymbols]) -> HashMap<String, Vec<CallerEntry>> {
    let mut map: HashMap<String, Vec<CallerEntry>> = HashMap::new();

    for file_entry in scan {
        let language = if let Some(lang) = language_for_file(&file_entry.file) {
            lang
        } else if let Some(name) = language_name_for_file(&file_entry.file) {
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
            if r.kind != ReferenceKind::Call {
                continue;
            }
            let Some(caller_name) = &r.caller else {
                continue;
            };
            let caller_kind = r.caller_kind.unwrap_or(SymbolKind::Function);

            // Extract the called symbol name from the context
            let line_text = r.context.as_str();
            if let Some(called_name) = extract_name_at_column(line_text, r.column) {
                map.entry(called_name).or_default().push((
                    caller_name.clone(),
                    caller_kind,
                    r.file.clone(),
                    r.line,
                    r.column,
                ));
            }
        }
    }

    map
}

/// Recursively build a call chain node.
#[allow(clippy::too_many_arguments)]
fn build_chain_node(
    name: &str,
    kind: SymbolKind,
    file: &Path,
    line: usize,
    column: usize,
    caller_map: &HashMap<String, Vec<CallerEntry>>,
    index: &SymbolIndex,
    remaining_depth: usize,
    visited: &mut HashSet<String>,
) -> CallChainNode {
    let mut node = CallChainNode {
        name: name.to_string(),
        kind,
        file: file.to_path_buf(),
        line,
        column,
        callers: vec![],
    };

    if remaining_depth == 0 || !visited.insert(name.to_string()) {
        return node;
    }

    if let Some(callers) = caller_map.get(name) {
        // Deduplicate callers by name (multiple call sites from same function = one entry)
        let mut seen_callers: HashSet<String> = HashSet::new();

        for (caller_name, caller_kind, caller_file, caller_line, caller_col) in callers {
            if !seen_callers.insert(caller_name.clone()) {
                continue;
            }

            // Look up the caller's definition for accurate position
            let (def_file, def_line, def_col, def_kind) =
                if let Some(def) = index.find_by_name(caller_name).first() {
                    (def.file.clone(), def.line, def.column, def.kind)
                } else {
                    (caller_file.clone(), *caller_line, *caller_col, *caller_kind)
                };

            let child = build_chain_node(
                caller_name,
                def_kind,
                &def_file,
                def_line,
                def_col,
                caller_map,
                index,
                remaining_depth - 1,
                visited,
            );
            node.callers.push(child);
        }
    }

    visited.remove(name);
    node
}

/// Extract the identifier name at a given column position in a source line.
fn extract_name_at_column(line: &str, column: usize) -> Option<String> {
    let bytes = line.as_bytes();
    if column >= bytes.len() {
        return None;
    }
    let mut end = column;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if end > column {
        Some(line[column..end].to_string())
    } else {
        None
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
    fn extract_name_at_column_out_of_bounds() {
        assert_eq!(extract_name_at_column("abc", 10), None);
    }

    #[test]
    fn test_build_caller_map_empty_scan() {
        let scan: Vec<FileSymbols> = vec![];
        let map = build_caller_map(&scan);
        assert!(map.is_empty());
    }
}
