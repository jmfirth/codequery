//! Shared path resolution utilities.
//!
//! Consolidates the display path logic used by commands like `outline` and `def`
//! to compute relative paths for output.

use std::path::{Path, PathBuf};

/// Compute the display path for a file relative to the project root.
///
/// Returns the path with the project root prefix stripped. Falls back to the
/// original path if `strip_prefix` fails (e.g., if the file is outside the
/// project root).
#[must_use]
pub fn resolve_display_path(file: &Path, project_root: &Path) -> PathBuf {
    file.strip_prefix(project_root)
        .map_or_else(|_| file.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_display_path_strips_project_root_prefix() {
        let file = Path::new("/home/user/project/src/main.rs");
        let root = Path::new("/home/user/project");
        let result = resolve_display_path(file, root);
        assert_eq!(result, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn test_resolve_display_path_falls_back_when_outside_project() {
        let file = Path::new("/other/location/file.rs");
        let root = Path::new("/home/user/project");
        let result = resolve_display_path(file, root);
        assert_eq!(result, PathBuf::from("/other/location/file.rs"));
    }

    #[test]
    fn test_resolve_display_path_handles_file_at_root() {
        let file = Path::new("/home/user/project/Cargo.toml");
        let root = Path::new("/home/user/project");
        let result = resolve_display_path(file, root);
        assert_eq!(result, PathBuf::from("Cargo.toml"));
    }
}
