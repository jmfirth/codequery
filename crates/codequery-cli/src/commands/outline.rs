//! Outline command: list all symbols in a file with kind, visibility, and nesting.

use std::path::Path;

use codequery_core::detect_project_root_or;
use codequery_parse::{extract_symbols, RustParser};

use crate::args::ExitCode;
use crate::output::format_outline;

/// Run the outline command: list all symbols in a file.
///
/// Resolves the project root, parses the file with tree-sitter, extracts all
/// symbol definitions, formats the outline, and prints it to stdout.
///
/// # Errors
///
/// Returns an error if the parser cannot be created (language grammar failure).
pub fn run(file: &Path, project: Option<&Path>) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Validate file exists — resolve relative paths against cwd
    let absolute_file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };

    if !absolute_file.exists() {
        eprintln!("error: file not found: {}", file.display());
        return Ok(ExitCode::ProjectError);
    }

    // 3. Compute relative path for display
    let relative_path = absolute_file
        .canonicalize()?
        .strip_prefix(project_root.canonicalize()?)
        .map_or_else(|_| file.to_path_buf(), Path::to_path_buf);

    // 4. Parse
    let mut parser = RustParser::new()?;
    let (source, tree) = match parser.parse_file(&absolute_file) {
        Ok(result) => result,
        Err(codequery_parse::ParseError::Io(e)) => {
            eprintln!("error: cannot read file: {e}");
            return Ok(ExitCode::ProjectError);
        }
        Err(e) => return Err(e.into()),
    };

    let has_parse_errors = tree.root_node().has_error();

    // 5. Extract
    let symbols = extract_symbols(&source, &tree, &relative_path);

    // 6. Format
    let output = format_outline(&relative_path, &symbols);

    // 7. Output
    println!("{output}");

    // Determine exit code
    if symbols.is_empty() {
        Ok(ExitCode::NoResults)
    } else if has_parse_errors {
        Ok(ExitCode::ParseWarning)
    } else {
        Ok(ExitCode::Success)
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

    // Test 1: Valid Rust file produces symbols in output
    #[test]
    fn test_outline_valid_rust_file_produces_symbols() {
        let project = fixture_project();
        let file = project.join("src/lib.rs");
        let result = run(&file, Some(&project));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 2: Empty Rust file produces just the file header (NoResults)
    #[test]
    fn test_outline_empty_file_returns_no_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Create a minimal project with Cargo.toml so project detection works
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let empty_file = tmp.path().join("empty.rs");
        std::fs::write(&empty_file, "").unwrap();

        let result = run(&empty_file, Some(tmp.path()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 3: Nonexistent file returns ProjectError
    #[test]
    fn test_outline_nonexistent_file_returns_project_error() {
        let project = fixture_project();
        let file = project.join("src/nonexistent.rs");
        let result = run(&file, Some(&project));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    // Test 4: File with parse errors still produces partial results (ParseWarning)
    #[test]
    fn test_outline_file_with_parse_errors_returns_parse_warning() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let broken_file = tmp.path().join("broken.rs");
        std::fs::write(&broken_file, "fn good() {}\nfn broken( {}\nstruct S {}\n").unwrap();

        let result = run(&broken_file, Some(tmp.path()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ParseWarning);
    }
}
