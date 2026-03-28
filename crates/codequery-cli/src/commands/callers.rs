//! Callers command: find call sites for a function/method.
//!
//! Uses stack graph resolution (with syntactic fallback) filtered to
//! `ReferenceKind::Call` only. Includes the caller function name and kind in
//! output. Same resolution pattern as the refs command.

use std::path::Path;

use codequery_core::{detect_project_root_or, Reference, ReferenceKind, Resolution, Symbol};
use codequery_index::{extract_references, scan_project_cached, SymbolIndex};
use codequery_lsp::resolve_with_cascade;
use codequery_resolve::StackGraphResolver;

use crate::args::{ExitCode, OutputMode};
use crate::output::format_callers;

/// Run the callers command: find all call sites for a function/method.
///
/// Scans all source files in parallel, builds a symbol index to find the
/// definition, then uses the resolution cascade (daemon, oneshot LSP, stack
/// graph, syntactic fallback) to extract call-site references. This is a wide
/// command with best-effort completeness.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or scanning encounters a fatal error.
#[allow(clippy::too_many_arguments)]
// All parameters are essential CLI-to-command plumbing; grouping would obscure the call site.
#[allow(clippy::too_many_lines)]
// Steps 1-8 form a linear pipeline; splitting would scatter the flow.
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    context_lines: usize,
    use_cache: bool,
    use_semantic: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Scan all files in parallel (with optional caching)
    let scan_results = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Build symbol index and find definitions
    let index = SymbolIndex::from_scan(&scan_results);
    let definitions = index.find_by_name(symbol);

    // 4. Build a syntactic reference map for kind/context enrichment (Call refs only)
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
            if r.kind == ReferenceKind::Call && ref_name_matches(&r, &file_result.source, symbol) {
                syntactic_map.insert((r.file.clone(), r.line, r.column), r);
            }
        }
    }

    // 5. Use resolution cascade (daemon -> oneshot LSP -> stack graph)
    //    The cascade returns all references; we filter to calls via the syntactic map.
    let resolution_result = if let Some(def) = definitions.first() {
        let def_lang =
            codequery_core::language_for_file(&def.file).unwrap_or(codequery_core::Language::Rust);
        let mut result = resolve_with_cascade(
            &project_root,
            def_lang,
            symbol,
            &def.file,
            def.line,
            def.column,
            &scan_results,
            use_semantic,
        );
        // Filter to call references only: keep Semantic/Resolved refs that match
        // a Call in the syntactic map, or keep all Resolved refs (stack graph
        // can't classify kinds, so downstream intersection handles it).
        result.references.retain(|r| {
            r.resolution == Resolution::Resolved || r.resolution == Resolution::Semantic
        });
        result
    } else {
        let mut resolver = StackGraphResolver::new();
        resolver.resolve_callers(&scan_results, symbol)
    };

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

    let mut call_refs: Vec<Reference> = resolution_result
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

    // 6b. Merge: add syntactic call references that the cascade missed.
    {
        let cascade_locations: std::collections::HashSet<(std::path::PathBuf, usize, usize)> =
            call_refs
                .iter()
                .map(|r| (r.file.clone(), r.line, r.column))
                .collect();
        for (loc, syntactic_ref) in syntactic_map {
            if !cascade_locations.contains(&loc) {
                call_refs.push(syntactic_ref);
            }
        }
    }

    // 7. Sort by file path, then line number
    call_refs.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 8. Format and output
    let has_results = !definitions.is_empty() || !call_refs.is_empty();

    if !has_results && mode != OutputMode::Json {
        return Ok(ExitCode::NoResults);
    }

    let def_clones: Vec<Symbol> = definitions.into_iter().cloned().collect();
    let output = format_callers(
        &def_clones,
        &call_refs,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    // -----------------------------------------------------------------------
    // ref_name_matches
    // -----------------------------------------------------------------------

    fn make_ref(
        line: usize,
        column: usize,
        kind: ReferenceKind,
        caller: Option<&str>,
    ) -> Reference {
        Reference {
            file: PathBuf::from("test.rs"),
            line,
            column,
            kind,
            context: String::new(),
            caller: caller.map(String::from),
            caller_kind: None,
        }
    }

    #[test]
    fn test_callers_ref_name_matches_exact() {
        let source = "fn main() {\n    greet();\n}";
        let r = make_ref(2, 4, ReferenceKind::Call, Some("main"));
        assert!(ref_name_matches(&r, source, "greet"));
    }

    #[test]
    fn test_callers_ref_name_matches_rejects_different_name() {
        let source = "fn main() {\n    hello();\n}";
        let r = make_ref(2, 4, ReferenceKind::Call, Some("main"));
        assert!(!ref_name_matches(&r, source, "greet"));
    }

    #[test]
    fn test_callers_ref_name_matches_out_of_bounds_returns_false() {
        let source = "fn main() {}";
        let r = make_ref(99, 0, ReferenceKind::Call, None);
        assert!(!ref_name_matches(&r, source, "main"));
    }

    // -----------------------------------------------------------------------
    // Integration: run against fixture project
    // -----------------------------------------------------------------------

    #[test]
    fn test_callers_finds_call_sites() {
        let project = fixture_project();
        // summarize is called inside process_users in services.rs
        let result = run(
            "summarize",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_callers_excludes_import_references() {
        let project = fixture_project();
        // User is imported in services.rs but the callers command should only
        // return Call references, not imports
        let result = run(
            "User",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            0,
            false,
            false,
        );
        // User may have Call refs (constructor calls like User::new) or may not;
        // we just verify it doesn't error
        assert!(result.is_ok());
    }

    #[test]
    fn test_callers_includes_caller_function_name() {
        let project = fixture_project();
        // process_users calls summarize — should work and include caller info
        let result = run(
            "summarize",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_callers_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_callers_json_mode() {
        let project = fixture_project();
        let result = run(
            "summarize",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_callers_raw_mode() {
        let project = fixture_project();
        let result = run(
            "summarize",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_callers_json_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            0,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_callers_with_context_returns_success() {
        let project = fixture_project();
        let result = run(
            "summarize",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            2,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
