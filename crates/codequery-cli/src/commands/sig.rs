//! Sig command: extract the type signature of a symbol without its body.

use std::path::Path;

use codequery_core::Language;

use super::common::find_symbols_by_name;
use crate::args::{ExitCode, OutputMode};
use crate::output::format_sig;

/// Run the sig command: extract the signature of a symbol.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols, filters by name, and
/// prints signature results in the requested output mode.
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
    let matches = find_symbols_by_name(symbol, project, scope, lang_filter)?;

    if matches.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_sig(&matches, symbol, mode, pretty);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    #[test]
    fn test_sig_fixture_finds_greet_function() {
        let project = fixture_project();
        let result = run(
            "greet",
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
    fn test_sig_fixture_finds_user_struct() {
        let project = fixture_project();
        let result = run(
            "User",
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
    fn test_sig_fixture_finds_validate_trait() {
        let project = fixture_project();
        let result = run(
            "Validate",
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
    fn test_sig_fixture_nonexistent_symbol_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
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
    fn test_sig_json_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Json, true, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_sig_json_mode_no_results_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol",
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_sig_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Raw, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_sig_scope_limits_search_to_subdirectory() {
        let project = fixture_project();
        let result = run(
            "helper",
            Some(&project),
            Some(Path::new("src/utils")),
            OutputMode::Framed,
            false,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
