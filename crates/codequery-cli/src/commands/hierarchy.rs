//! Hierarchy command: show type hierarchy (supertypes and subtypes).
//!
//! Finds what a type extends/implements and what extends/implements it,
//! using structural AST matching as the fallback tier.

use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, SymbolKind, TypeHierarchyNode, TypeHierarchyResult,
};
use codequery_index::{scan_project_cached, SymbolIndex};
use codequery_parse::extract_supertype_relations;

use crate::args::{ExitCode, OutputMode};
use crate::output::format_hierarchy;

/// Run the hierarchy command: show type supertypes and subtypes.
///
/// Scans the project for supertype relations (extends, implements, impl Trait for)
/// and reports both directions for the target type.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or scanning fails.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
// CLI command runners naturally take one parameter per flag
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    lang_filter: Option<codequery_core::Language>,
    mode: OutputMode,
    pretty: bool,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Parallel scan all source files
    let scan = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Build symbol index and find the target type
    let index = SymbolIndex::from_scan(&scan);
    let definitions = index.find_by_name(symbol);

    // Find the target — prefer type-like symbols (struct, class, trait, interface, enum)
    let target_def = definitions
        .iter()
        .find(|d| {
            matches!(
                d.kind,
                SymbolKind::Struct
                    | SymbolKind::Class
                    | SymbolKind::Trait
                    | SymbolKind::Interface
                    | SymbolKind::Enum
            )
        })
        .or(definitions.first());

    let target = if let Some(def) = target_def {
        TypeHierarchyNode {
            name: def.name.clone(),
            kind: def.kind,
            file: def.file.clone(),
            line: def.line,
            column: def.column,
        }
    } else {
        // Symbol not found — still attempt structural matching in case
        // the type is defined externally
        TypeHierarchyNode {
            name: symbol.to_string(),
            kind: SymbolKind::Class,
            file: std::path::PathBuf::from("<unknown>"),
            line: 0,
            column: 0,
        }
    };

    // 4. Extract all supertype relations across the project
    let mut supertypes: Vec<TypeHierarchyNode> = Vec::new();
    let mut subtypes: Vec<TypeHierarchyNode> = Vec::new();

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

        let relations = extract_supertype_relations(
            &file_entry.source,
            &file_entry.tree,
            &file_entry.file,
            language,
        );

        for rel in &relations {
            if rel.subtype == symbol {
                // This type extends/implements something
                if let Some(super_def) = index.find_by_name(&rel.supertype).first() {
                    supertypes.push(TypeHierarchyNode {
                        name: super_def.name.clone(),
                        kind: super_def.kind,
                        file: super_def.file.clone(),
                        line: super_def.line,
                        column: super_def.column,
                    });
                } else {
                    supertypes.push(TypeHierarchyNode {
                        name: rel.supertype.clone(),
                        kind: SymbolKind::Trait,
                        file: rel.file.clone(),
                        line: rel.line,
                        column: 0,
                    });
                }
            }
            if rel.supertype == symbol {
                // Something extends/implements this type
                if let Some(sub_def) = index.find_by_name(&rel.subtype).first() {
                    subtypes.push(TypeHierarchyNode {
                        name: sub_def.name.clone(),
                        kind: sub_def.kind,
                        file: sub_def.file.clone(),
                        line: sub_def.line,
                        column: sub_def.column,
                    });
                } else {
                    subtypes.push(TypeHierarchyNode {
                        name: rel.subtype.clone(),
                        kind: SymbolKind::Struct,
                        file: rel.file.clone(),
                        line: rel.line,
                        column: 0,
                    });
                }
            }
        }
    }

    // 5. Deduplicate
    supertypes.sort_by(|a, b| a.name.cmp(&b.name));
    supertypes.dedup_by(|a, b| a.name == b.name);
    subtypes.sort_by(|a, b| a.name.cmp(&b.name));
    subtypes.dedup_by(|a, b| a.name == b.name);

    let result = TypeHierarchyResult {
        target,
        supertypes,
        subtypes,
    };

    // 6. Format and output
    let has_results = !result.supertypes.is_empty() || !result.subtypes.is_empty();

    if !has_results && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_hierarchy(&result, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if has_results {
            Ok(ExitCode::Success)
        } else {
            Ok(ExitCode::NoResults)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_against_empty_project() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let result = run(
            "NonexistentType",
            Some(tmp.path()),
            None,
            None,
            OutputMode::Json,
            false,
            false,
        );
        assert!(result.is_ok());
    }
}
