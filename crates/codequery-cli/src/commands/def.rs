//! Def command: find where a symbol is defined across the project.

use std::path::Path;

use codequery_core::Language;
use codequery_lsp::{oneshot, pid};

use super::common::find_symbols_by_name;
use crate::args::{ExitCode, OutputMode};
use crate::output::format_def;

/// Run the def command: find where a symbol is defined.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols, filters by name, and
/// prints results in the requested output mode.
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
    use_semantic: bool,
) -> anyhow::Result<ExitCode> {
    let matches = find_symbols_by_name(symbol, project, scope, lang_filter)?;

    // When semantic resolution is available and there are multiple matches,
    // try LSP definition to disambiguate to the most precise result.
    let matches = if use_semantic && matches.len() > 1 {
        disambiguate_with_lsp(&matches, symbol, project).unwrap_or(matches)
    } else {
        matches
    };

    if matches.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_def(&matches, symbol, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if matches.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

/// Try to disambiguate multiple definition matches using LSP `textDocument/definition`.
///
/// Uses the first match as the query location. If the LSP returns a single result
/// that matches one of our candidates, narrows the result set. Falls back to `None`
/// (keeping all matches) if the daemon is not running, semantic resolution is not
/// available, or the LSP result doesn't help narrow candidates.
fn disambiguate_with_lsp(
    matches: &[codequery_core::Symbol],
    _symbol: &str,
    project: Option<&Path>,
) -> Option<Vec<codequery_core::Symbol>> {
    // Only attempt if a daemon is running (avoid expensive oneshot for def disambiguation)
    if !pid::is_daemon_running() {
        return None;
    }

    let cwd = std::env::current_dir().ok()?;
    let project_root = codequery_core::detect_project_root_or(&cwd, project).ok()?;
    let first = matches.first()?;
    let language = codequery_core::language_for_file(&first.file)?;

    let lsp_results = oneshot::semantic_definition(
        &project_root,
        language,
        &first.file,
        first.line,
        first.column,
    )
    .ok()?;

    if lsp_results.len() == 1 {
        let lsp_ref = &lsp_results[0];
        // Find the match that corresponds to the LSP result
        let narrowed: Vec<codequery_core::Symbol> = matches
            .iter()
            .filter(|m| m.file == lsp_ref.ref_file && m.line == lsp_ref.ref_line)
            .cloned()
            .collect();
        if !narrowed.is_empty() {
            return Some(narrowed);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    // -- Correctness scenario tests against the fixture project --

    #[test]
    fn test_def_fixture_finds_greet_function() {
        let project = fixture_project();
        let result = run(
            "greet",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_fixture_finds_user_struct() {
        let project = fixture_project();
        let result = run(
            "User",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_fixture_finds_duplicate_helper() {
        let project = fixture_project();
        let result = run(
            "helper",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_fixture_finds_is_adult_method() {
        let project = fixture_project();
        let result = run(
            "is_adult",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_fixture_nonexistent_symbol_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
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

    #[test]
    fn test_def_fixture_scoped_search_finds_only_utils_helper() {
        let project = fixture_project();
        let result = run(
            "helper",
            Some(&project),
            Some(Path::new("src/utils")),
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_json_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            "greet",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_json_mode_no_results_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_def_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            "greet",
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
    fn test_def_lang_filter_rust_finds_results() {
        let project = fixture_project();
        let result = run(
            "greet",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            Some(Language::Rust),
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_def_lang_filter_python_excludes_rust_results() {
        let project = fixture_project();
        let result = run(
            "greet",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            Some(Language::Python),
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }
}
