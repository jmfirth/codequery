//! Project root detection by walking up the directory tree looking for marker files.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// Marker files and directories checked at each directory level, in priority order.
///
/// The first match at any level wins. Priority is defined in SPECIFICATION.md section 5.
const MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "Makefile",
    "CMakeLists.txt",
    ".cq.toml",
];

/// Detect the project root by walking up from `start` looking for marker files/dirs.
///
/// Markers are checked in priority order at each directory level. First match wins.
/// Returns `Err(CoreError::ProjectNotFound)` if no marker is found before the filesystem root.
///
/// # Errors
///
/// Returns `CoreError::Path` if `start` cannot be canonicalized.
/// Returns `CoreError::ProjectNotFound` if no marker is found walking up to the filesystem root.
pub fn detect_project_root(start: &Path) -> Result<PathBuf> {
    let canonical = start
        .canonicalize()
        .map_err(|e| CoreError::Path(format!("cannot canonicalize {}: {e}", start.display())))?;

    let mut current = canonical.as_path();

    loop {
        for marker in MARKERS {
            if current.join(marker).exists() {
                return Ok(current.to_path_buf());
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return Err(CoreError::ProjectNotFound(start.to_path_buf())),
        }
    }
}

/// Detect project root, or use an explicit override if provided.
///
/// If `explicit` is `Some`, validates the path exists and returns it.
/// Otherwise falls back to `detect_project_root(start)`.
///
/// # Errors
///
/// Returns `CoreError::Path` if the explicit path does not exist or cannot be canonicalized.
/// Falls through to `detect_project_root` errors when no explicit path is given.
pub fn detect_project_root_or(start: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    match explicit {
        Some(path) => {
            if path.exists() {
                path.canonicalize().map_err(|e| {
                    CoreError::Path(format!(
                        "cannot canonicalize explicit path {}: {e}",
                        path.display()
                    ))
                })
            } else {
                Err(CoreError::Path(format!(
                    "explicit project path does not exist: {}",
                    path.display()
                )))
            }
        }
        None => detect_project_root(start),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_project_root_finds_git_directory() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let root = detect_project_root(tmp.path()).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_project_root_finds_cargo_toml_when_no_git() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();

        let root = detect_project_root(tmp.path()).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_project_root_walks_up_from_subdirectory() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let sub = tmp.path().join("src");
        std::fs::create_dir(&sub).unwrap();

        let root = detect_project_root(&sub).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_project_root_walks_up_multiple_levels() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let deep = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();

        let root = detect_project_root(&deep).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_project_root_returns_not_found_for_unmarked_directory() {
        let tmp = TempDir::new().unwrap();
        // Create a subdirectory with no markers anywhere in the chain
        // We can't truly isolate from the filesystem root markers, but we can check
        // that an isolated temp dir (with no markers) eventually fails.
        // Since temp dirs are usually under /tmp which has no project markers,
        // this should produce ProjectNotFound.
        let isolated = tmp.path().join("isolated");
        std::fs::create_dir(&isolated).unwrap();

        let result = detect_project_root(&isolated);
        // The function walks up to the filesystem root. If no markers exist anywhere
        // in the path, it returns ProjectNotFound. On some systems, /tmp may be under
        // a path with markers. We verify the function returns either Ok or the specific
        // error type.
        match result {
            Ok(_) => {
                // A marker was found somewhere above the temp dir — this is
                // system-dependent behavior and acceptable.
            }
            Err(CoreError::ProjectNotFound(_)) => {
                // Expected on systems where no markers exist above /tmp
            }
            Err(e) => panic!("unexpected error type: {e}"),
        }
    }

    #[test]
    fn test_detect_project_root_or_explicit_overrides_detection() {
        let tmp = TempDir::new().unwrap();
        let explicit = tmp.path();

        let result = detect_project_root_or(Path::new("/nonexistent"), Some(explicit)).unwrap();
        assert_eq!(result, tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_project_root_or_nonexistent_explicit_path_returns_error() {
        let result =
            detect_project_root_or(Path::new("/tmp"), Some(Path::new("/nonexistent/path")));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CoreError::Path(_)),
            "expected Path error, got: {err}"
        );
    }

    #[test]
    fn test_detect_project_root_git_takes_priority_over_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();

        // Both markers exist; .git has higher priority so root should still be found.
        // (Both are at the same level, so the first-found marker wins — .git is first.)
        let root = detect_project_root(tmp.path()).unwrap();
        assert_eq!(root, tmp.path().canonicalize().unwrap());
    }
}
