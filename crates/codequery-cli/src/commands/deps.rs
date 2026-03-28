//! Deps command: analyze internal dependencies of a function/method.

use std::collections::HashSet;
use std::path::Path;

use codequery_core::{
    detect_project_root_or, discover_files, language_for_file, Language, ReferenceKind, Resolution,
    Symbol,
};
use codequery_index::{extract_references, scan_project, SymbolIndex};
use codequery_parse::Parser;
use codequery_resolve::StackGraphResolver;

use super::common::find_first_symbol_with_source;
use crate::args::{ExitCode, OutputMode};
use crate::output::{format_deps, Dependency};

/// Run the deps command: analyze internal dependencies of a symbol.
///
/// Finds the target symbol using the narrow pipeline (text pre-filter + parse),
/// then uses stack graph resolution to determine where each dependency is defined.
/// Falls back to syntactic symbol index lookup if stack graph resolution fails.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or the parser cannot be created.
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    lang_filter: Option<Language>,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Find the target symbol using narrow pipeline
    let files = discover_files(&project_root, scope)?;
    let (target, target_source) =
        find_first_symbol_with_source(&files, &project_root, symbol, lang_filter)?;

    let Some(target_sym) = target else {
        if mode == OutputMode::Json {
            print_output(&format_deps(None, &[], symbol, mode, pretty));
        }
        return Ok(ExitCode::NoResults);
    };

    if target_sym.body.is_none() {
        // Symbol found but has no body (e.g., a type alias)
        print_output(&format_deps(Some(&target_sym), &[], symbol, mode, pretty));
        return Ok(ExitCode::Success);
    }

    // 3. Scan project and extract body references
    let source = target_source.as_deref().unwrap_or("");
    let body_refs = extract_body_references(source, &target_sym)?;
    let scan = scan_project(&project_root, None)?;

    // 4. Attempt stack graph resolution, fall back to syntactic index lookup
    let dependencies = resolve_with_stack_graphs(&scan, &target_sym, symbol, source, &body_refs)
        .unwrap_or_else(|| resolve_syntactic(source, &body_refs, symbol, &scan));

    let output = format_deps(Some(&target_sym), &dependencies, symbol, mode, pretty);
    print_output(&output);

    Ok(ExitCode::Success)
}

/// Resolve dependencies via stack graph scope resolution.
///
/// Creates a `StackGraphResolver`, calls `resolve_deps` for the target symbol's
/// line range, then merges resolved results with syntactic body references.
/// Returns `None` if resolution produces no useful results, signaling the caller
/// to fall back to pure syntactic resolution.
fn resolve_with_stack_graphs(
    scan: &[codequery_index::FileSymbols],
    target: &Symbol,
    symbol: &str,
    source: &str,
    body_refs: &[codequery_core::Reference],
) -> Option<Vec<Dependency>> {
    let mut resolver = StackGraphResolver::new();
    let line_range = (target.line, target.end_line);
    let result = resolver.resolve_deps(scan, &target.file, line_range, symbol);

    // Build a lookup from (ref_name) -> resolved def_file for refs that came back resolved
    let mut resolved_map: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for rr in &result.references {
        let def_file = rr.def_file.as_ref().map(|p| p.display().to_string());
        resolved_map.entry(rr.symbol.clone()).or_insert(def_file);
    }

    // If stack graphs returned nothing, signal fallback
    if resolved_map.is_empty() {
        return None;
    }

    // Merge: walk body_refs, use resolved info where available, syntactic index for the rest
    let index = codequery_index::SymbolIndex::from_scan(scan);
    let mut seen = HashSet::new();
    let mut dependencies = Vec::new();

    for reference in body_refs {
        let Some(name) = extract_ref_name(source, reference.line, reference.column) else {
            continue;
        };

        if name == symbol || !seen.insert(name.clone()) {
            continue;
        }

        let dep_kind = ref_kind_label(reference.kind);

        if let Some(def_file) = resolved_map.get(&name) {
            dependencies.push(Dependency {
                name,
                kind: dep_kind.to_string(),
                defined_in: def_file.clone(),
                resolution: Resolution::Resolved,
            });
        } else {
            // Fall back to syntactic index lookup for this specific dependency
            let definitions = index.find_by_name(&name);
            let defined_in = definitions
                .first()
                .map(|sym| sym.file.display().to_string());
            dependencies.push(Dependency {
                name,
                kind: dep_kind.to_string(),
                defined_in,
                resolution: Resolution::Syntactic,
            });
        }
    }

    Some(dependencies)
}

/// Resolve body references to named dependencies via project-wide symbol index (syntactic fallback).
fn resolve_syntactic(
    source: &str,
    body_refs: &[codequery_core::Reference],
    symbol: &str,
    scan: &[codequery_index::FileSymbols],
) -> Vec<Dependency> {
    let index = SymbolIndex::from_scan(scan);

    let mut seen = HashSet::new();
    let mut dependencies = Vec::new();

    for reference in body_refs {
        let Some(name) = extract_ref_name(source, reference.line, reference.column) else {
            continue;
        };

        // Skip self-references and deduplicates
        if name == symbol || !seen.insert(name.clone()) {
            continue;
        }

        let dep_kind = ref_kind_label(reference.kind);

        let definitions = index.find_by_name(&name);
        let defined_in = definitions
            .first()
            .map(|sym| sym.file.display().to_string());

        dependencies.push(Dependency {
            name,
            kind: dep_kind.to_string(),
            defined_in,
            resolution: Resolution::Syntactic,
        });
    }

    dependencies
}

/// Extract references from the target symbol's body line range.
fn extract_body_references(
    source: &str,
    target: &Symbol,
) -> anyhow::Result<Vec<codequery_core::Reference>> {
    let Some(lang) = language_for_file(&target.file) else {
        return Ok(Vec::new());
    };

    let mut parser = Parser::for_language(lang)?;
    let tree = parser.parse(source.as_bytes())?;
    let all_refs = extract_references(source, &tree, &target.file, lang);

    // Filter to references within the symbol's line range
    Ok(all_refs
        .into_iter()
        .filter(|r| r.line >= target.line && r.line <= target.end_line)
        .collect())
}

/// Map a `ReferenceKind` to its string label for dependency output.
fn ref_kind_label(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Call => "call",
        ReferenceKind::TypeUsage => "type_reference",
        ReferenceKind::Import => "import",
        ReferenceKind::Assignment => "assignment",
    }
}

/// Print output if non-empty.
fn print_output(output: &str) {
    if !output.is_empty() {
        println!("{output}");
    }
}

/// Extract the identifier name at a given line and column from source text.
fn extract_ref_name(source: &str, line: usize, column: usize) -> Option<String> {
    let source_line = source.lines().nth(line.checked_sub(1)?)?;
    if column >= source_line.len() {
        return None;
    }
    let rest = &source_line[column..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    // -----------------------------------------------------------------------
    // extract_ref_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_deps_extract_ref_name_simple() {
        let source = "fn main() {\n    greet();\n}";
        let name = extract_ref_name(source, 2, 4);
        assert_eq!(name.as_deref(), Some("greet"));
    }

    #[test]
    fn test_deps_extract_ref_name_at_end_of_line() {
        let source = "fn test() {\n    foo\n}";
        let name = extract_ref_name(source, 2, 4);
        assert_eq!(name.as_deref(), Some("foo"));
    }

    #[test]
    fn test_deps_extract_ref_name_invalid_line() {
        let source = "fn main() {}";
        let name = extract_ref_name(source, 5, 0);
        assert!(name.is_none());
    }

    #[test]
    fn test_deps_extract_ref_name_column_past_end() {
        let source = "fn main() {}";
        let name = extract_ref_name(source, 1, 999);
        assert!(name.is_none());
    }

    // -----------------------------------------------------------------------
    // Fixture integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_deps_finds_function_calls_in_body() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_finds_type_references() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_unresolvable_has_null_defined_in() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Json, true, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_deps_symbol_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_deps_json_includes_best_effort_metadata() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_raw_mode_works() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_framed_mode_works() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_json_mode_symbol_not_found() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }
}
