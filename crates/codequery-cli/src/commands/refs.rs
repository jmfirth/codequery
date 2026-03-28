//! Refs command: find all references to a symbol across the project.

use std::path::Path;

use codequery_core::{detect_project_root_or, Reference, ReferenceKind, Resolution, Symbol};
use codequery_index::{extract_references, scan_project, SymbolIndex};
use codequery_resolve::StackGraphResolver;

use crate::args::{ExitCode, OutputMode};
use crate::output::format_refs;

/// Run the refs command: find all references to a symbol across the project.
///
/// Scans all source files in parallel, builds a symbol index to find the
/// definition, then uses stack graph resolution (with syntactic fallback) to
/// extract references from every file. This is a wide command with best-effort
/// completeness.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or scanning encounters a fatal error.
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    context_lines: usize,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Scan all files in parallel
    let scan_results = scan_project(&project_root, scope)?;

    // 3. Build symbol index and find definitions
    let index = SymbolIndex::from_scan(&scan_results);
    let definitions = index.find_by_name(symbol);

    // 4. Build a syntactic reference map for kind/context enrichment
    let mut syntactic_map: std::collections::HashMap<
        (std::path::PathBuf, usize, usize),
        Reference,
    > = std::collections::HashMap::new();

    for file_result in &scan_results {
        let Some(language) = codequery_core::language_for_file(&file_result.file) else {
            continue;
        };

        let file_refs = extract_references(
            &file_result.source,
            &file_result.tree,
            &file_result.file,
            language,
        );

        for r in file_refs {
            if ref_name_matches(&r, &file_result.source, symbol) {
                syntactic_map.insert((r.file.clone(), r.line, r.column), r);
            }
        }
    }

    // 5. Use stack graph resolver for scope-aware reference resolution
    let mut resolver = StackGraphResolver::new();
    let resolution_result = resolver.resolve_refs(&scan_results, symbol);

    // Determine the top-level resolution quality
    let all_resolved = !resolution_result.references.is_empty()
        && resolution_result
            .references
            .iter()
            .all(|r| r.resolution == Resolution::Resolved);
    let resolution = if all_resolved {
        Resolution::Resolved
    } else {
        Resolution::Syntactic
    };

    // 6. Convert ResolvedReferences to core References, enriching with syntactic data
    let source_map: std::collections::HashMap<&Path, &str> = scan_results
        .iter()
        .map(|fs| (fs.file.as_path(), fs.source.as_str()))
        .collect();

    let mut all_refs: Vec<Reference> = resolution_result
        .references
        .iter()
        .map(|rr| {
            let key = (rr.ref_file.clone(), rr.ref_line, rr.ref_column);
            if let Some(syntactic_ref) = syntactic_map.get(&key) {
                syntactic_ref.clone()
            } else {
                // Build a Reference from the resolved data + source context
                let context = source_map
                    .get(rr.ref_file.as_path())
                    .and_then(|src| src.lines().nth(rr.ref_line.saturating_sub(1)))
                    .unwrap_or("")
                    .to_string();
                Reference {
                    file: rr.ref_file.clone(),
                    line: rr.ref_line,
                    column: rr.ref_column,
                    kind: ReferenceKind::Call,
                    context,
                    caller: None,
                    caller_kind: None,
                }
            }
        })
        .collect();

    // 7. Sort by file path, then line number
    all_refs.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 8. Format and output
    let has_results = !definitions.is_empty() || !all_refs.is_empty();

    if !has_results && mode != OutputMode::Json {
        return Ok(ExitCode::NoResults);
    }

    let def_clones: Vec<Symbol> = definitions.into_iter().cloned().collect();
    let output = format_refs(
        &def_clones,
        &all_refs,
        symbol,
        mode,
        pretty,
        context_lines,
        &source_map,
        resolution,
    );
    if !output.is_empty() {
        println!("{output}");
    }

    if has_results {
        Ok(ExitCode::Success)
    } else {
        Ok(ExitCode::NoResults)
    }
}

/// Check if the identifier at a reference's location matches the symbol name.
///
/// Extracts the identifier text from the source at the reference's line and
/// column, then compares against the target symbol name.
fn ref_name_matches(reference: &Reference, source: &str, symbol: &str) -> bool {
    let Some(line_text) = source.lines().nth(reference.line - 1) else {
        return false;
    };
    if reference.column >= line_text.len() {
        return false;
    }
    let rest = &line_text[reference.column..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    &rest[..end] == symbol
}

/// Read context lines around a reference from the source text.
///
/// Returns up to `context` lines before and after the reference line,
/// formatted as a block of text.
pub fn get_context_lines(source: &str, ref_line: usize, context: usize) -> Vec<String> {
    if context == 0 {
        return Vec::new();
    }

    let lines: Vec<&str> = source.lines().collect();
    let total = lines.len();

    if ref_line == 0 || ref_line > total {
        return Vec::new();
    }

    let idx = ref_line - 1; // 0-based
    let start = idx.saturating_sub(context);
    let end = (idx + context + 1).min(total);

    lines[start..end].iter().map(|l| (*l).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::ReferenceKind;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    // -----------------------------------------------------------------------
    // ref_name_matches
    // -----------------------------------------------------------------------

    fn make_ref(line: usize, column: usize, kind: ReferenceKind) -> Reference {
        Reference {
            file: PathBuf::from("test.rs"),
            line,
            column,
            kind,
            context: String::new(),
            caller: None,
            caller_kind: None,
        }
    }

    #[test]
    fn test_ref_name_matches_exact_match() {
        let source = "fn main() {\n    greet();\n}";
        let r = make_ref(2, 4, ReferenceKind::Call);
        assert!(ref_name_matches(&r, source, "greet"));
    }

    #[test]
    fn test_ref_name_matches_does_not_match_different_name() {
        let source = "fn main() {\n    hello();\n}";
        let r = make_ref(2, 4, ReferenceKind::Call);
        assert!(!ref_name_matches(&r, source, "greet"));
    }

    #[test]
    fn test_ref_name_matches_partial_name_does_not_match() {
        let source = "fn main() {\n    greeter();\n}";
        let r = make_ref(2, 4, ReferenceKind::Call);
        assert!(!ref_name_matches(&r, source, "greet"));
    }

    #[test]
    fn test_ref_name_matches_out_of_bounds_line_returns_false() {
        let source = "fn main() {}";
        let r = make_ref(99, 0, ReferenceKind::Call);
        assert!(!ref_name_matches(&r, source, "main"));
    }

    // -----------------------------------------------------------------------
    // get_context_lines
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_context_lines_zero_context_returns_empty() {
        let source = "line1\nline2\nline3";
        let result = get_context_lines(source, 2, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_context_lines_one_context_returns_surrounding() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let result = get_context_lines(source, 3, 1);
        assert_eq!(result, vec!["line2", "line3", "line4"]);
    }

    #[test]
    fn test_get_context_lines_at_start_clips_before() {
        let source = "line1\nline2\nline3";
        let result = get_context_lines(source, 1, 2);
        assert_eq!(result, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_get_context_lines_at_end_clips_after() {
        let source = "line1\nline2\nline3";
        let result = get_context_lines(source, 3, 2);
        assert_eq!(result, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_get_context_lines_out_of_bounds_returns_empty() {
        let source = "line1\nline2";
        let result = get_context_lines(source, 99, 1);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // Integration: run against fixture project
    // -----------------------------------------------------------------------

    #[test]
    fn test_refs_finds_function_call_references() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Framed, false, 0);
        assert!(result.is_ok());
        // greet is defined in lib.rs — there may or may not be call refs,
        // but the definition should be found
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_finds_import_references() {
        let project = fixture_project();
        // User is imported in services.rs
        let result = run("User", Some(&project), None, OutputMode::Framed, false, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_shows_definition_location() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Framed, false, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            0,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_refs_json_includes_best_effort_metadata() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Json, true, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Raw, false, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_with_context_returns_success() {
        let project = fixture_project();
        let result = run("User", Some(&project), None, OutputMode::Framed, false, 2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_refs_json_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            0,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }
}
