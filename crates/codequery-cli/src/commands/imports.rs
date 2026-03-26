//! Imports command: list all imports/dependencies in a file.

use std::path::Path;

use codequery_core::{detect_project_root_or, language_for_file};
use codequery_parse::{extract_imports, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_imports_output;

/// Run the imports command: list all imports/dependencies in a file.
///
/// Resolves the project root, detects the file's language, parses with
/// tree-sitter, extracts all import declarations, formats the results
/// in the requested mode, and prints them to stdout.
///
/// # Errors
///
/// Returns an error if the parser cannot be created (language grammar failure).
pub fn run(
    file: &Path,
    project: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
) -> anyhow::Result<ExitCode> {
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

    // 3. Detect language from file extension
    let Some(language) = language_for_file(&absolute_file) else {
        eprintln!("error: unsupported file type: {}", absolute_file.display());
        return Ok(ExitCode::ProjectError);
    };

    // 4. Compute relative path for display
    let relative_path = absolute_file
        .canonicalize()?
        .strip_prefix(project_root.canonicalize()?)
        .map_or_else(|_| file.to_path_buf(), Path::to_path_buf);

    // 5. Parse
    let mut parser = Parser::for_language(language)?;
    let (source, tree) = match parser.parse_file(&absolute_file) {
        Ok(result) => result,
        Err(codequery_parse::ParseError::Io(e)) => {
            eprintln!("error: cannot read file: {e}");
            return Ok(ExitCode::ProjectError);
        }
        Err(e) => return Err(e.into()),
    };

    let has_parse_errors = tree.root_node().has_error();

    // 6. Extract imports
    let imports = extract_imports(&source, &tree, language);

    // 7. Format
    let output = format_imports_output(&relative_path, &imports, mode, pretty);

    // 8. Output
    println!("{output}");

    // Determine exit code
    if imports.is_empty() {
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
    fn rust_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Path to the fixture typescript project.
    fn ts_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/typescript_project")
    }

    /// Path to the fixture C project.
    fn c_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/c_project")
    }

    /// Path to the fixture Go project.
    fn go_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/go_project")
    }

    /// Path to the fixture Java project.
    fn java_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/java_project")
    }

    // Test 1: Rust use statements extracted
    #[test]
    fn test_imports_rust_file_extracts_use_statements() {
        let project = rust_fixture();
        let file = project.join("src/services.rs");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 2: TypeScript import statements extracted
    #[test]
    fn test_imports_typescript_file_extracts_imports() {
        let project = ts_fixture();
        let file = project.join("src/services.ts");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 3: File with no imports returns NoResults
    #[test]
    fn test_imports_file_with_no_imports_returns_no_results() {
        let project = rust_fixture();
        let file = project.join("src/models.rs");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 4: Nonexistent file returns ProjectError
    #[test]
    fn test_imports_nonexistent_file_returns_project_error() {
        let project = rust_fixture();
        let file = project.join("src/nonexistent.rs");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    // Test 5: JSON mode returns correct exit code
    #[test]
    fn test_imports_json_mode_returns_success() {
        let project = rust_fixture();
        let file = project.join("src/services.rs");
        let result = run(&file, Some(&project), OutputMode::Json, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 6: Raw mode returns correct exit code
    #[test]
    fn test_imports_raw_mode_returns_success() {
        let project = rust_fixture();
        let file = project.join("src/services.rs");
        let result = run(&file, Some(&project), OutputMode::Raw, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 7: C includes extracted
    #[test]
    fn test_imports_c_file_extracts_includes() {
        let project = c_fixture();
        let file = project.join("main.c");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 8: Go imports extracted
    #[test]
    fn test_imports_go_file_extracts_imports() {
        let project = go_fixture();
        let file = project.join("main.go");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 9: Java imports extracted
    #[test]
    fn test_imports_java_file_extracts_imports() {
        let project = java_fixture();
        let file = project.join("src/main/java/com/example/Main.java");
        let result = run(&file, Some(&project), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 10: Unsupported file type returns ProjectError
    #[test]
    fn test_imports_unsupported_file_type_returns_project_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let file = tmp.path().join("readme.txt");
        std::fs::write(&file, "Just text").unwrap();

        let result = run(&file, Some(tmp.path()), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::ProjectError);
    }

    // Test 11: Empty file returns NoResults
    #[test]
    fn test_imports_empty_file_returns_no_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let file = tmp.path().join("empty.rs");
        std::fs::write(&file, "").unwrap();

        let result = run(&file, Some(tmp.path()), OutputMode::Framed, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }
}
