//! File discovery with `.gitignore`-aware walking and language detection.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::error::{CoreError, Result};

/// Supported source languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// The Rust programming language.
    Rust,
    // Future phases add: TypeScript, JavaScript, Python, Go, C, Cpp, Java, etc.
}

/// Detect the language of a file from its extension.
///
/// Phase 0 only recognizes `.rs` as `Language::Rust`. All other extensions return `None`.
#[must_use]
pub fn language_for_file(path: &Path) -> Option<Language> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "rs" => Some(Language::Rust),
            _ => None,
        })
}

/// Discover source files under `root`, optionally scoped to a subdirectory.
///
/// Uses the `ignore` crate for `.gitignore`-aware walking.
/// Filters to files with recognized source extensions.
/// Returns sorted paths (relative to `root`) for deterministic output.
///
/// # Errors
///
/// Returns `CoreError::Path` if the walk root (or scoped subdirectory) does not exist,
/// or if a filesystem error occurs during walking.
pub fn discover_files(root: &Path, scope: Option<&Path>) -> Result<Vec<PathBuf>> {
    let walk_root = match scope {
        Some(s) => root.join(s),
        None => root.to_path_buf(),
    };

    if !walk_root.exists() {
        return Err(CoreError::Path(format!(
            "discovery path does not exist: {}",
            walk_root.display()
        )));
    }

    let walker = WalkBuilder::new(&walk_root).build();

    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|e| CoreError::Path(format!("walk error: {e}")))?;

        // Skip directories — we only want files.
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }

        let path = entry.path();
        if language_for_file(path).is_some() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Path(format!("cannot make path relative: {e}")))?;
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create a minimal project structure in a temp dir.
    fn create_project(files: &[&str]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        // Create a .git dir so the ignore crate has a root context
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        for file in files {
            let path = tmp.path().join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, "// placeholder").unwrap();
        }
        tmp
    }

    #[test]
    fn test_discover_files_finds_rs_files() {
        let tmp = create_project(&["src/main.rs", "src/lib.rs", "src/util.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from("src/lib.rs"),
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/util.rs"),
            ]
        );
    }

    #[test]
    fn test_discover_files_respects_gitignore() {
        let tmp = create_project(&["src/main.rs", "src/generated.rs"]);
        std::fs::write(tmp.path().join(".gitignore"), "src/generated.rs\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_skips_target_directory() {
        let tmp = create_project(&["src/main.rs", "target/debug/build.rs"]);
        // The ignore crate skips hidden dirs and respects .gitignore.
        // `target/` is typically in .gitignore for Rust projects.
        std::fs::write(tmp.path().join(".gitignore"), "target/\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_scope_limits_to_subdirectory() {
        let tmp = create_project(&["src/main.rs", "tests/test_it.rs"]);

        let files = discover_files(tmp.path(), Some(Path::new("src"))).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_returns_relative_paths() {
        let tmp = create_project(&["src/main.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        for f in &files {
            assert!(
                f.is_relative(),
                "expected relative path, got: {}",
                f.display()
            );
        }
    }

    #[test]
    fn test_discover_files_returns_sorted_paths() {
        let tmp = create_project(&["z.rs", "a.rs", "m.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from("a.rs"),
                PathBuf::from("m.rs"),
                PathBuf::from("z.rs"),
            ]
        );
    }

    #[test]
    fn test_discover_files_empty_directory_returns_empty_vec() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_discover_files_nonexistent_scope_returns_error() {
        let tmp = TempDir::new().unwrap();

        let result = discover_files(tmp.path(), Some(Path::new("nonexistent")));
        assert!(result.is_err());
    }

    #[test]
    fn test_language_for_file_returns_rust_for_rs() {
        assert_eq!(
            language_for_file(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_language_for_file_returns_none_for_other_extensions() {
        assert_eq!(language_for_file(Path::new("script.py")), None);
        assert_eq!(language_for_file(Path::new("app.js")), None);
        assert_eq!(language_for_file(Path::new("readme.txt")), None);
        assert_eq!(language_for_file(Path::new("no_extension")), None);
    }
}
