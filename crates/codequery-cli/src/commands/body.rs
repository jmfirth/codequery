//! Body command: extract the full source body of a symbol.

use std::path::Path;

use codequery_core::Language;

use super::common::find_symbols_by_name;
use crate::args::{ExitCode, OutputMode};
use crate::output::format_body;

/// Run the body command: extract the full source body of a symbol.
///
/// Discovers all source files in the project (optionally scoped by `--in`),
/// applies a text pre-filter to avoid parsing irrelevant files, parses
/// candidates with tree-sitter, extracts symbols (with body text populated),
/// filters by name, and prints bodies in the requested output mode.
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
        let output = format_body(&matches, symbol, mode, pretty);
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
    fn test_body_fixture_greet_returns_body() {
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
    fn test_body_fixture_user_returns_body() {
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
    fn test_body_fixture_nonexistent_returns_no_results() {
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
    fn test_body_fixture_json_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Json, true, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_body_fixture_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run("greet", Some(&project), None, OutputMode::Raw, false, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_body_fixture_scope_limits_search() {
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

    #[test]
    fn test_body_fixture_multiple_matches() {
        let project = fixture_project();
        let result = run(
            "helper",
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
    fn test_body_json_no_results() {
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
}
