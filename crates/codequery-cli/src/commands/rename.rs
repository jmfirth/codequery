//! Rename command: rename a symbol across the project.
//!
//! Uses the precision cascade to find all references, computes text edits,
//! and either outputs a diff (dry-run) or applies the changes. Applies by
//! default when resolution is semantic/resolved; dry-run when syntactic.

use std::collections::HashMap;
use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, Language, Resolution, SymbolKind, TextEdit,
};
use codequery_index::{extract_references, scan_project_cached, SymbolIndex};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_rename;

/// Run the rename command: rename a symbol across the project.
///
/// Finds all references to the symbol using the precision cascade, computes
/// text edits, and either outputs a unified diff or applies the changes.
///
/// By default:
/// - Applies immediately when resolution is semantic or resolved
/// - Shows a dry-run diff when resolution is syntactic (higher false positive risk)
/// - `--dry-run` forces preview mode regardless of resolution
/// - `--apply` forces write mode regardless of resolution (reserved for future use)
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, scanning fails,
/// or the symbol is not found.
#[allow(clippy::too_many_arguments)]
// CLI command runners naturally take one parameter per flag
pub fn run(
    old_name: &str,
    new_name: &str,
    dry_run: bool,
    project: Option<&Path>,
    scope: Option<&Path>,
    lang_filter: Option<Language>,
    mode: OutputMode,
    pretty: bool,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Parallel scan all source files
    let scan = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Build symbol index and find the symbol definition
    let index = SymbolIndex::from_scan(&scan);
    let definitions = index.find_by_name(old_name);

    if definitions.is_empty() {
        return Err(anyhow::anyhow!("symbol not found: {old_name}"));
    }

    // 4. Collect all references to the symbol (syntactic tier for now)
    let mut edits: Vec<TextEdit> = Vec::new();
    let resolution = Resolution::Syntactic;

    for file_entry in &scan {
        let absolute = project_root.join(&file_entry.file);
        let Some(language) = language_for_file(&absolute) else {
            continue;
        };

        // Apply language filter
        if let Some(lang) = lang_filter {
            if language != lang {
                continue;
            }
        }

        let refs = extract_references(
            &file_entry.source,
            &file_entry.tree,
            &file_entry.file,
            language,
        );

        // Find references matching the old name
        for r in &refs {
            let line_text = r.context.as_str();
            if let Some(ref_name) = extract_name_at_column(line_text, r.column) {
                if ref_name == old_name {
                    edits.push(TextEdit {
                        file: r.file.clone(),
                        line: r.line,
                        column: r.column,
                        end_line: r.line,
                        end_column: r.column + old_name.len(),
                        new_text: new_name.to_string(),
                    });
                }
            }
        }
    }

    // Also include the definition sites
    for def in &definitions {
        // Skip impl blocks
        if def.kind == SymbolKind::Impl {
            continue;
        }
        edits.push(TextEdit {
            file: def.file.clone(),
            line: def.line,
            column: def.column,
            end_line: def.line,
            end_column: def.column + old_name.len(),
            new_text: new_name.to_string(),
        });
    }

    // Deduplicate edits by location
    edits.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    edits.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.column == b.column);

    // Count affected files
    let mut files: std::collections::HashSet<&Path> = std::collections::HashSet::new();
    for edit in &edits {
        files.insert(&edit.file);
    }
    let files_affected = files.len();

    // 5. Determine whether to apply or dry-run
    // Semantic/resolved → apply by default. Syntactic → dry-run by default.
    let should_apply = !dry_run
        && matches!(resolution, Resolution::Semantic | Resolution::Resolved);

    // 6. Apply edits if not dry-run
    if should_apply {
        apply_edits(&project_root, &edits)?;
    }

    // 7. Format and output
    let result = codequery_core::RenameResult {
        old_name: old_name.to_string(),
        new_name: new_name.to_string(),
        edits,
        files_affected,
        applied: should_apply,
        resolution,
    };

    let output = format_rename(&result, mode, pretty);
    if !output.is_empty() {
        println!("{output}");
    }

    if result.edits.is_empty() {
        Ok(ExitCode::NoResults)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Apply text edits to files on disk.
///
/// Processes edits grouped by file, applying them in reverse line order
/// so that earlier edits don't shift the positions of later ones.
fn apply_edits(project_root: &Path, edits: &[TextEdit]) -> anyhow::Result<()> {
    // Group edits by file
    let mut by_file: HashMap<&Path, Vec<&TextEdit>> = HashMap::new();
    for edit in edits {
        by_file.entry(edit.file.as_path()).or_default().push(edit);
    }

    for (file, mut file_edits) in by_file {
        let abs_path = project_root.join(file);
        let content = std::fs::read_to_string(&abs_path)?;
        let mut lines: Vec<String> = content.lines().map(String::from).collect();

        // Sort edits in reverse order so we can apply without position shifts
        file_edits.sort_by(|a, b| b.line.cmp(&a.line).then(b.column.cmp(&a.column)));

        for edit in &file_edits {
            let line_idx = edit.line.saturating_sub(1);
            if line_idx < lines.len() {
                let line = &mut lines[line_idx];
                if edit.column + (edit.end_column - edit.column) <= line.len() {
                    line.replace_range(edit.column..edit.end_column, &edit.new_text);
                }
            }
        }

        let new_content = lines.join("\n");
        // Preserve trailing newline if original had one
        let new_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
            new_content + "\n"
        } else {
            new_content
        };
        std::fs::write(&abs_path, new_content)?;
    }

    Ok(())
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
    fn test_run_symbol_not_found() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let result = run(
            "NonexistentSymbol",
            "NewName",
            false,
            Some(tmp.path()),
            None,
            None,
            OutputMode::Json,
            false,
            false,
        );
        assert!(result.is_err());
    }
}
