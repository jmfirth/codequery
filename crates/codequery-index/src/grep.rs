//! Fast text pre-filter using memory-mapped files and byte search.
//!
//! Provides a grep-like word-boundary search to narrow the set of files
//! that need full tree-sitter parsing. Uses `memmap2` for zero-copy file
//! access and `memchr::memmem` for fast substring search.

use std::fs;
use std::path::Path;

use memchr::memmem;
use memmap2::Mmap;

use crate::error::Result;

/// Threshold below which we read the file into memory instead of mmap.
/// mmap overhead is not worth it for small files.
const MMAP_THRESHOLD: u64 = 32 * 1024;

/// Check whether a byte at the given position is a word character (alphanumeric or underscore).
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Check whether a match at `pos` with length `len` in `data` sits on word boundaries.
///
/// A word boundary means the character immediately before the match (if any) and
/// immediately after the match (if any) are not word characters.
fn is_word_boundary(data: &[u8], pos: usize, len: usize) -> bool {
    if pos > 0 && is_word_char(data[pos - 1]) {
        return false;
    }
    let end = pos + len;
    if end < data.len() && is_word_char(data[end]) {
        return false;
    }
    true
}

/// Search for `word` in `data` at a word boundary.
///
/// Returns `true` if `data` contains `word` as a standalone identifier
/// (bounded by non-word characters on both sides).
fn data_contains_word(data: &[u8], word: &str) -> bool {
    let needle = word.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let finder = memmem::Finder::new(needle);
    let mut start = 0;
    while start <= data.len() {
        let Some(pos) = finder.find(&data[start..]) else {
            break;
        };
        let absolute = start + pos;
        if is_word_boundary(data, absolute, needle.len()) {
            return true;
        }
        start = absolute + 1;
    }
    false
}

/// Check whether a file contains `word` at a word boundary.
///
/// For files smaller than 32 KB, reads the file into memory.
/// For larger files, uses memory mapping for zero-copy access.
///
/// # Errors
///
/// Returns an error if the file cannot be read or memory-mapped.
pub fn file_contains_word(path: &Path, word: &str) -> Result<bool> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();

    if size == 0 {
        return Ok(false);
    }

    if size < MMAP_THRESHOLD {
        let data = fs::read(path)?;
        return Ok(data_contains_word(&data, word));
    }

    let file = fs::File::open(path)?;
    // SAFETY: The file is opened read-only and we only read the mapped region.
    // The file must not be modified while mapped. This is safe for source files
    // during a single cq invocation (stateless, no concurrent writes expected).
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(data_contains_word(&mmap, word))
}

/// Filter a list of relative file paths to those containing `word` at a word boundary.
///
/// Each path in `files` is joined with `root` to form the absolute path for reading.
/// Files that cannot be read are silently skipped (error-tolerant).
#[must_use]
pub fn filter_files(
    files: &[std::path::PathBuf],
    root: &Path,
    word: &str,
) -> Vec<std::path::PathBuf> {
    files
        .iter()
        .filter(|f| {
            let full = root.join(f);
            file_contains_word(&full, word).unwrap_or(false)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    // -----------------------------------------------------------------------
    // Word boundary logic
    // -----------------------------------------------------------------------

    #[test]
    fn test_data_contains_word_exact_match_returns_true() {
        let data = b"fn greet(name: &str) -> String {";
        assert!(data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_substring_returns_false() {
        // "greet" inside "greeter" should not match at word boundary
        let data = b"fn greeter(name: &str) {}";
        assert!(!data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_at_start_of_data() {
        let data = b"greet(name)";
        assert!(data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_at_end_of_data() {
        let data = b"call greet";
        assert!(data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_prefix_boundary_fail() {
        let data = b"_greet()";
        assert!(!data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_suffix_boundary_fail() {
        let data = b"greet_all()";
        assert!(!data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_empty_data() {
        assert!(!data_contains_word(b"", "greet"));
    }

    #[test]
    fn test_data_contains_word_empty_word() {
        // Empty word is not a meaningful identifier search — returns false
        assert!(!data_contains_word(b"anything", ""));
    }

    #[test]
    fn test_data_contains_word_dot_boundary_succeeds() {
        let data = b"self.greet()";
        assert!(data_contains_word(data, "greet"));
    }

    #[test]
    fn test_data_contains_word_colon_boundary_succeeds() {
        let data = b"module::greet()";
        assert!(data_contains_word(data, "greet"));
    }

    // -----------------------------------------------------------------------
    // file_contains_word
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_contains_word_small_file_exact_match() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "small.rs", "fn greet() {}");
        assert!(file_contains_word(&path, "greet").unwrap());
    }

    #[test]
    fn test_file_contains_word_small_file_no_match() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "small.rs", "fn hello() {}");
        assert!(!file_contains_word(&path, "greet").unwrap());
    }

    #[test]
    fn test_file_contains_word_small_file_substring_no_match() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "small.rs", "fn greeter() {}");
        assert!(!file_contains_word(&path, "greet").unwrap());
    }

    #[test]
    fn test_file_contains_word_empty_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "empty.rs", "");
        assert!(!file_contains_word(&path, "greet").unwrap());
    }

    #[test]
    fn test_file_contains_word_large_file_uses_mmap() {
        let tmp = TempDir::new().unwrap();
        // Create a file larger than MMAP_THRESHOLD (32KB)
        let mut content = String::new();
        for i in 0..2000 {
            content.push_str(&format!("fn func_{i}() {{ }}\n"));
        }
        content.push_str("fn target_symbol() {}\n");
        let path = write_file(tmp.path(), "large.rs", &content);
        assert!(path.metadata().unwrap().len() >= MMAP_THRESHOLD);
        assert!(file_contains_word(&path, "target_symbol").unwrap());
    }

    #[test]
    fn test_file_contains_word_large_file_no_match() {
        let tmp = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 0..2000 {
            content.push_str(&format!("fn func_{i}() {{ }}\n"));
        }
        let path = write_file(tmp.path(), "large.rs", &content);
        assert!(path.metadata().unwrap().len() >= MMAP_THRESHOLD);
        assert!(!file_contains_word(&path, "nonexistent_symbol").unwrap());
    }

    #[test]
    fn test_file_contains_word_nonexistent_file_returns_error() {
        let result = file_contains_word(Path::new("/nonexistent/file.rs"), "test");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // filter_files
    // -----------------------------------------------------------------------

    #[test]
    fn test_filter_files_reduces_file_set() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn greet() {}");
        write_file(tmp.path(), "b.rs", "fn hello() {}");
        write_file(tmp.path(), "c.rs", "fn greet_again() { greet(); }");

        let files = vec![
            PathBuf::from("a.rs"),
            PathBuf::from("b.rs"),
            PathBuf::from("c.rs"),
        ];

        let filtered = filter_files(&files, tmp.path(), "greet");
        // a.rs has "greet" as standalone word
        // b.rs does not
        // c.rs has "greet" in "greet_again" (no word boundary) but also "greet();" (word boundary)
        assert!(filtered.contains(&PathBuf::from("a.rs")));
        assert!(!filtered.contains(&PathBuf::from("b.rs")));
        assert!(filtered.contains(&PathBuf::from("c.rs")));
    }

    #[test]
    fn test_filter_files_empty_input() {
        let tmp = TempDir::new().unwrap();
        let files: Vec<PathBuf> = vec![];
        let filtered = filter_files(&files, tmp.path(), "anything");
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_files_skips_unreadable_files() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "exists.rs", "fn greet() {}");

        let files = vec![PathBuf::from("exists.rs"), PathBuf::from("missing.rs")];

        let filtered = filter_files(&files, tmp.path(), "greet");
        assert_eq!(filtered, vec![PathBuf::from("exists.rs")]);
    }
}
