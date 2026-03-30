//! Search command: structural AST pattern matching across a project.

use std::path::Path;

use codequery_core::{detect_project_root_or, language_for_file};
use codequery_index::scan_project_cached;
use codequery_parse::{search_file, search_file_raw, SearchMatch};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_search;

/// Run the search command: find AST patterns across the project.
///
/// Scans the project in parallel, then runs the pattern against each file.
/// When `mode` is `OutputMode::Raw`, the pattern is treated as a raw
/// tree-sitter S-expression query and [`search_file_raw`] is used instead
/// of the structural [`search_file`].
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, scanning fails,
/// or the pattern is invalid.
#[allow(clippy::too_many_arguments)]
// CLI command runners naturally take one parameter per flag; splitting would obscure the pipeline
pub fn run(
    pattern: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    limit: Option<usize>,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Parallel scan all source files
    let scan = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Run pattern against each file, collecting matches
    let is_raw = mode == OutputMode::Raw;
    let mut all_matches: Vec<SearchMatch> = Vec::new();
    let mut files_searched = 0usize;
    let mut last_query_error: Option<String> = None;

    for file_entry in &scan {
        let absolute = project_root.join(&file_entry.file);
        let matches_result = if is_raw {
            search_file_raw(
                pattern,
                &file_entry.source,
                &file_entry.tree,
                &file_entry.file,
            )
        } else {
            let Some(language) = language_for_file(&absolute) else {
                continue;
            };
            search_file(
                pattern,
                &file_entry.source,
                &file_entry.tree,
                &file_entry.file,
                language,
            )
        };

        match matches_result {
            Ok(matches) => {
                files_searched += 1;
                all_matches.extend(matches);
            }
            Err(
                codequery_parse::ParseError::PatternError(ref msg)
                | codequery_parse::ParseError::QueryError(ref msg),
            ) => {
                // Pattern or query is invalid for this language's grammar.
                // Expected when searching a multi-language project — a Rust
                // pattern/query won't compile against TOML/JSON/YAML grammars.
                // Track the error in case it fails on ALL files.
                last_query_error = Some(msg.clone());
            }
            Err(codequery_parse::ParseError::LanguageError(_)) => {
                // Language grammar not available (feature not compiled in, no WASM).
                // Skip the file — don't prevent results from other files.
            }
            Err(e) => {
                // Other errors (I/O, parse failures) are fatal
                return Err(anyhow::anyhow!("{e}"));
            }
        }
    }

    // If the pattern/query failed for every file, it's likely invalid — report as error
    if files_searched == 0 {
        if let Some(err) = last_query_error {
            return Err(anyhow::anyhow!("pattern error: {err}"));
        }
    }

    // 4. Sort by file, then line, then column for deterministic output
    all_matches.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });

    // 5. Apply --limit if provided
    if let Some(limit) = limit {
        all_matches.truncate(limit);
    }

    // 6. Format and output
    //    For search, --raw means S-expression mode, so we format as Framed
    //    unless --json was specified
    let output_mode = if is_raw { OutputMode::Framed } else { mode };

    if all_matches.is_empty() && output_mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_search(&all_matches, pattern, output_mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if all_matches.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Path to the fixture python project.
    fn fixture_python_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_project")
    }

    // -----------------------------------------------------------------------
    // Structural search
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_structural_finds_functions_in_rust_project() {
        let project = fixture_project();
        let result = run(
            "fn $NAME($ARGS) -> String { $BODY }",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        // The fixture has at least one function returning String
    }

    #[test]
    fn test_search_structural_no_match_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "fn zzz_nonexistent_function_xyz() {}",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // -----------------------------------------------------------------------
    // Raw S-expression search
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_raw_finds_function_names_in_rust_project() {
        let project = fixture_project();
        let result = run(
            "(function_item name: (identifier) @name)",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_search_raw_finds_functions_in_python_project() {
        let project = fixture_python_project();
        let result = run(
            "(function_definition name: (identifier) @name)",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // -----------------------------------------------------------------------
    // JSON output
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_json_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            "fn $NAME() {}",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
            false,
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Limit
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_limit_caps_results() {
        let project = fixture_project();
        let result = run(
            "(function_item name: (identifier) @name)",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            Some(1),
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // -----------------------------------------------------------------------
    // Invalid pattern
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_raw_invalid_query_returns_error() {
        let project = fixture_project();
        let result = run(
            "(not_a_real_node @name)",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            None,
            false,
        );
        // Should error because query is invalid for Rust grammar
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Empty project
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_empty_project_returns_no_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let result = run(
            "fn $NAME() {}",
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // -----------------------------------------------------------------------
    // Scope filtering
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_with_scope_limits_search() {
        let project = fixture_project();
        let result = run(
            "(function_item name: (identifier) @name)",
            Some(&project),
            Some(Path::new("src")),
            OutputMode::Raw,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
    }
}
