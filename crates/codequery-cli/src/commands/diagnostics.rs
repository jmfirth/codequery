//! Diagnostics command: show syntax errors (and future: LSP diagnostics) for a file or project.

use std::path::Path;

use codequery_core::{
    detect_project_root_or, language_for_file, language_name_for_file, Diagnostic,
    DiagnosticSeverity,
};
use codequery_index::scan_project_cached;
use codequery_parse::{extract_syntax_errors, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_diagnostics;

/// Run the diagnostics command: collect syntax errors for a file or project.
///
/// For each file in scope, parses with tree-sitter and extracts all ERROR and
/// MISSING nodes as `Diagnostic` values. Results are sorted by severity (errors
/// first), then by file path, then by line number.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or scanning fails.
#[allow(clippy::too_many_arguments)]
// CLI command runners naturally take one parameter per flag
pub fn run(
    file: Option<&Path>,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Collect diagnostics — single file or whole project
    let mut diagnostics: Vec<Diagnostic> = if let Some(file_path) = file {
        // Single-file mode
        let absolute = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            cwd.join(file_path)
        };

        if !absolute.exists() {
            eprintln!("error: file not found: {}", file_path.display());
            return Ok(ExitCode::ProjectError);
        }

        let mut parser = if let Some(language) = language_for_file(&absolute) {
            Parser::for_language(language)?
        } else if let Some(lang_name) = language_name_for_file(&absolute) {
            Parser::for_name(&lang_name)?
        } else {
            eprintln!("error: unsupported file type: {}", absolute.display());
            return Ok(ExitCode::ProjectError);
        };
        let (source, tree) = match parser.parse_file(&absolute) {
            Ok(r) => r,
            Err(codequery_parse::ParseError::Io(e)) => {
                eprintln!("error: cannot read file: {e}");
                return Ok(ExitCode::ProjectError);
            }
            Err(e) => return Err(e.into()),
        };

        // Use relative display path when possible
        let display_path = absolute
            .strip_prefix(&project_root)
            .unwrap_or(&absolute)
            .to_path_buf();

        extract_syntax_errors(&source, &tree, &display_path)
    } else {
        // Project-wide mode
        let scan = scan_project_cached(&project_root, scope, use_cache)?;

        scan.iter()
            .flat_map(|entry| extract_syntax_errors(&entry.source, &entry.tree, &entry.file))
            .collect()
    };

    // 3. Sort: errors first, then by file, then by line
    diagnostics.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });

    // 4. Format and output
    if diagnostics.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_diagnostics(&diagnostics, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if diagnostics.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            // Return ParseWarning when diagnostics are found — the project has
            // parse errors but we still produced results
            let has_errors = diagnostics
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error);
            if has_errors {
                Ok(ExitCode::ParseWarning)
            } else {
                Ok(ExitCode::Success)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use std::path::PathBuf;

    /// Return a path to the shared Rust fixture project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Create a temporary directory containing a `.git` folder so it's treated
    /// as a project root.
    fn temp_git_dir() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

    // -----------------------------------------------------------------------
    // Project-wide scan
    // -----------------------------------------------------------------------

    #[test]
    fn test_diagnostics_clean_project_returns_no_results() {
        let project = fixture_project();
        let result = run(None, Some(&project), None, OutputMode::Framed, false, false);
        assert!(result.is_ok());
        // The fixture project should be clean (no syntax errors)
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_diagnostics_clean_project_json_returns_success() {
        let project = fixture_project();
        let result = run(None, Some(&project), None, OutputMode::Json, true, false);
        assert!(result.is_ok());
        // JSON mode always emits output even for zero results
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_diagnostics_project_with_syntax_error_returns_parse_warning() {
        let tmp = temp_git_dir();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir(&src_dir).unwrap();
        let broken = src_dir.join("broken.rs");
        std::fs::File::create(&broken)
            .unwrap()
            .write_all(b"fn main() {")
            .unwrap();

        let result = run(
            None,
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ParseWarning);
    }

    #[test]
    fn test_diagnostics_project_with_syntax_error_json_mode() {
        let tmp = temp_git_dir();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir(&src_dir).unwrap();
        let broken = src_dir.join("broken.rs");
        std::fs::File::create(&broken)
            .unwrap()
            .write_all(b"fn main() {")
            .unwrap();

        let result = run(None, Some(tmp.path()), None, OutputMode::Json, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ParseWarning);
    }

    #[test]
    fn test_diagnostics_empty_project_returns_no_results() {
        let tmp = temp_git_dir();
        let result = run(
            None,
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // -----------------------------------------------------------------------
    // Single-file mode
    // -----------------------------------------------------------------------

    #[test]
    fn test_diagnostics_single_clean_file_returns_no_results() {
        let tmp = temp_git_dir();
        let file = tmp.path().join("clean.rs");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"fn main() { let x = 42; }")
            .unwrap();

        let result = run(
            Some(&file),
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_diagnostics_single_broken_file_returns_parse_warning() {
        let tmp = temp_git_dir();
        let file = tmp.path().join("broken.rs");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"fn main() {")
            .unwrap();

        let result = run(
            Some(&file),
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ParseWarning);
    }

    #[test]
    fn test_diagnostics_single_nonexistent_file_returns_project_error() {
        let tmp = temp_git_dir();
        let file = tmp.path().join("does_not_exist.rs");

        let result = run(
            Some(&file),
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    #[test]
    fn test_diagnostics_raw_mode_broken_file() {
        let tmp = temp_git_dir();
        let file = tmp.path().join("broken.rs");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"fn main() {")
            .unwrap();

        let result = run(
            Some(&file),
            Some(tmp.path()),
            None,
            OutputMode::Raw,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ParseWarning);
    }
}
