//! Optional disk caching for project symbol data.
//!
//! When enabled, caches the results of file scanning (symbols, mtimes, sizes)
//! to disk so repeated queries on unchanged projects skip re-parsing.
//! Cache is opt-in only — cq is stateless by default.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use codequery_core::{Symbol, SymbolKind, Visibility};

/// A symbol representation optimized for binary cache serialization.
///
/// Unlike `codequery_core::Symbol`, this type does not use `skip_serializing_if`
/// attributes, which would cause misalignment in non-self-describing formats
/// like bincode.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedSymbol {
    name: String,
    kind: SymbolKind,
    file: PathBuf,
    line: usize,
    column: usize,
    end_line: usize,
    visibility: Visibility,
    children: Vec<CachedSymbol>,
    doc: Option<String>,
    body: Option<String>,
    signature: Option<String>,
}

impl From<&Symbol> for CachedSymbol {
    fn from(s: &Symbol) -> Self {
        Self {
            name: s.name.clone(),
            kind: s.kind,
            file: s.file.clone(),
            line: s.line,
            column: s.column,
            end_line: s.end_line,
            visibility: s.visibility,
            children: s.children.iter().map(CachedSymbol::from).collect(),
            doc: s.doc.clone(),
            body: s.body.clone(),
            signature: s.signature.clone(),
        }
    }
}

impl From<CachedSymbol> for Symbol {
    fn from(s: CachedSymbol) -> Self {
        Self {
            name: s.name,
            kind: s.kind,
            file: s.file,
            line: s.line,
            column: s.column,
            end_line: s.end_line,
            visibility: s.visibility,
            children: s.children.into_iter().map(Symbol::from).collect(),
            doc: s.doc,
            body: s.body,
            signature: s.signature,
        }
    }
}

/// A single cached file entry with its metadata and extracted symbols.
#[derive(Debug, Clone)]
pub struct CachedFile {
    /// Relative path from project root.
    pub path: PathBuf,
    /// Last modification time as seconds since the Unix epoch.
    pub mtime: u64,
    /// File size in bytes.
    pub size: u64,
    /// Symbols extracted from this file.
    pub symbols: Vec<Symbol>,
}

/// Internal serializable representation of a cached file.
#[derive(Debug, Serialize, Deserialize)]
struct CachedFileData {
    path: PathBuf,
    mtime: u64,
    size: u64,
    symbols: Vec<CachedSymbol>,
}

impl From<&CachedFile> for CachedFileData {
    fn from(f: &CachedFile) -> Self {
        Self {
            path: f.path.clone(),
            mtime: f.mtime,
            size: f.size,
            symbols: f.symbols.iter().map(CachedSymbol::from).collect(),
        }
    }
}

impl From<CachedFileData> for CachedFile {
    fn from(f: CachedFileData) -> Self {
        Self {
            path: f.path,
            mtime: f.mtime,
            size: f.size,
            symbols: f.symbols.into_iter().map(Symbol::from).collect(),
        }
    }
}

/// The full cache manifest stored on disk.
#[derive(Debug, Serialize, Deserialize)]
struct CacheManifest {
    /// Cache format version for forward compatibility.
    version: u32,
    /// The cached file entries.
    files: Vec<CachedFileData>,
}

/// Current cache format version. Bump when the serialization format changes.
const CACHE_VERSION: u32 = 1;

/// Manages disk cache storage for a project.
///
/// Cache location is determined by (in order of precedence):
/// 1. `$CQ_CACHE_DIR`
/// 2. `$XDG_CACHE_HOME/cq/`
/// 3. `$HOME/.cache/cq/`
///
/// Cache key: first 16 characters of SHA-256 of the canonicalized project root.
#[derive(Debug)]
pub struct CacheStore {
    /// The path to the cache file for this project.
    cache_path: PathBuf,
    /// The cache directory (for `clear_all` operations).
    cache_dir: PathBuf,
}

/// Errors that can occur during cache operations.
///
/// These are all non-fatal — callers should fall back to a full scan on any error.
#[derive(Debug)]
pub enum CacheError {
    /// Cache file does not exist (first run or cleared).
    NotFound,
    /// Cache data is corrupt or incompatible version.
    Corrupt(String),
    /// I/O error reading or writing cache.
    Io(std::io::Error),
    /// Cache is stale (files changed since cache was written).
    Stale,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "cache not found"),
            Self::Corrupt(msg) => write!(f, "corrupt cache: {msg}"),
            Self::Io(e) => write!(f, "cache I/O error: {e}"),
            Self::Stale => write!(f, "cache is stale"),
        }
    }
}

impl From<std::io::Error> for CacheError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl CacheStore {
    /// Create a new `CacheStore` for the given project root.
    ///
    /// Returns `None` if no suitable cache directory can be determined
    /// (e.g., no `HOME`, no `XDG_CACHE_HOME`, and no `CQ_CACHE_DIR`).
    #[must_use]
    pub fn new(project_root: &Path) -> Option<Self> {
        let cache_dir = resolve_cache_dir()?;
        let key = cache_key(project_root);
        let cache_path = cache_dir.join(format!("{key}.bin"));
        Some(Self {
            cache_path,
            cache_dir,
        })
    }

    /// Create a `CacheStore` with an explicit cache directory (for testing).
    #[cfg(test)]
    fn with_dir(project_root: &Path, cache_dir: PathBuf) -> Self {
        let key = cache_key(project_root);
        let cache_path = cache_dir.join(format!("{key}.bin"));
        Self {
            cache_path,
            cache_dir,
        }
    }

    /// Load cached file data from disk.
    ///
    /// Returns the cached entries if the cache file exists, is valid,
    /// and passes format version checks.
    ///
    /// # Errors
    ///
    /// Returns `CacheError` if the cache is missing, corrupt, or has
    /// an incompatible version.
    pub fn load(&self) -> std::result::Result<Vec<CachedFile>, CacheError> {
        let data = match fs::read(&self.cache_path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(CacheError::NotFound),
            Err(e) => return Err(CacheError::Io(e)),
        };

        let manifest: CacheManifest =
            bincode::deserialize(&data).map_err(|e| CacheError::Corrupt(e.to_string()))?;

        if manifest.version != CACHE_VERSION {
            return Err(CacheError::Corrupt(format!(
                "version mismatch: expected {CACHE_VERSION}, got {}",
                manifest.version
            )));
        }

        Ok(manifest.files.into_iter().map(CachedFile::from).collect())
    }

    /// Store file data to the disk cache.
    ///
    /// Creates the cache directory if it does not exist. Silently returns
    /// `Err` on write failures (e.g., read-only filesystem).
    ///
    /// # Errors
    ///
    /// Returns `CacheError::Io` if directory creation or file writing fails.
    pub fn store(&self, files: &[CachedFile]) -> std::result::Result<(), CacheError> {
        let manifest = CacheManifest {
            version: CACHE_VERSION,
            files: files.iter().map(CachedFileData::from).collect(),
        };

        let data = bincode::serialize(&manifest)
            .map_err(|e| CacheError::Corrupt(format!("serialization failed: {e}")))?;

        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(&self.cache_path)?;
        file.write_all(&data)?;
        Ok(())
    }

    /// Remove the cache file for this project.
    ///
    /// Returns `Ok(true)` if the file was removed, `Ok(false)` if it
    /// did not exist. Returns `Err` on I/O failures.
    ///
    /// # Errors
    ///
    /// Returns `CacheError::Io` if file removal fails for a reason other
    /// than the file not existing.
    pub fn clear(&self) -> std::result::Result<bool, CacheError> {
        match fs::remove_file(&self.cache_path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Check whether the cached data is still valid against the current
    /// filesystem state.
    ///
    /// Compares each cached file's mtime and size against its current
    /// state on disk. Any mismatch means the cache is stale.
    #[must_use]
    pub fn is_valid(&self, cached: &[CachedFile], project_root: &Path) -> bool {
        for entry in cached {
            let abs_path = project_root.join(&entry.path);
            match file_metadata(&abs_path) {
                Some((mtime, size)) => {
                    if mtime != entry.mtime || size != entry.size {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }

    /// Return the path to the cache file (for diagnostics/testing).
    #[must_use]
    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}

/// Clear all cq caches across all projects.
///
/// Removes the entire cache directory. Returns `Ok(true)` if the directory
/// existed and was removed, `Ok(false)` if it did not exist.
///
/// # Errors
///
/// Returns an I/O error if directory removal fails.
pub fn clear_all_caches() -> std::result::Result<bool, std::io::Error> {
    let Some(cache_dir) = resolve_cache_dir() else {
        return Ok(false);
    };
    match fs::remove_dir_all(&cache_dir) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Resolve the cache directory from environment variables.
///
/// Precedence: `$CQ_CACHE_DIR` > `$XDG_CACHE_HOME/cq/` > `$HOME/.cache/cq/`
fn resolve_cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CQ_CACHE_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }

    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("cq"));
        }
    }

    #[allow(deprecated)]
    // Using deprecated std::env::home_dir for broad compatibility;
    // dirs crate not justified for a single call
    std::env::home_dir().map(|h| h.join(".cache").join("cq"))
}

/// Compute the cache key for a project root.
///
/// Returns the first 16 hex characters of the SHA-256 hash of the
/// canonicalized project root path.
fn cache_key(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let hash = Sha256::digest(canonical.to_string_lossy().as_bytes());
    format!("{hash:x}")[..16].to_string()
}

/// Get the mtime (as Unix epoch seconds) and size of a file.
///
/// Returns `None` if the file does not exist or metadata cannot be read.
fn file_metadata(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let size = meta.len();
    Some((mtime, size))
}

/// Get the mtime and size for a file, suitable for building `CachedFile` entries.
///
/// Public so the scanner can build cache entries from scan results.
#[must_use]
pub fn get_file_mtime_size(path: &Path) -> Option<(u64, u64)> {
    file_metadata(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{SymbolKind, Visibility};
    use tempfile::TempDir;

    fn make_symbol(name: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("test.rs"),
            line: 1,
            column: 0,
            end_line: 3,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        }
    }

    fn make_cached_file(path: &str, symbols: Vec<Symbol>) -> CachedFile {
        CachedFile {
            path: PathBuf::from(path),
            mtime: 1000,
            size: 100,
            symbols,
        }
    }

    // -----------------------------------------------------------------------
    // cache_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_key_returns_16_hex_chars() {
        let key = cache_key(Path::new("/some/project"));
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_cache_key_deterministic() {
        let key1 = cache_key(Path::new("/some/project"));
        let key2 = cache_key(Path::new("/some/project"));
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_different_for_different_roots() {
        let key1 = cache_key(Path::new("/project/a"));
        let key2 = cache_key(Path::new("/project/b"));
        assert_ne!(key1, key2);
    }

    // -----------------------------------------------------------------------
    // resolve_cache_dir
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_cache_dir_returns_some() {
        // In normal environments, HOME is set so this should return Some
        let dir = resolve_cache_dir();
        assert!(dir.is_some());
    }

    // -----------------------------------------------------------------------
    // CacheStore::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_store_new_returns_some_for_valid_root() {
        let store = CacheStore::new(Path::new("/tmp/test-project"));
        assert!(store.is_some());
    }

    #[test]
    fn test_cache_store_cache_path_ends_with_bin() {
        let store = CacheStore::new(Path::new("/tmp/test-project")).unwrap();
        assert!(store.cache_path().to_string_lossy().ends_with(".bin"));
    }

    // -----------------------------------------------------------------------
    // store + load round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_store_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(
            Path::new("/tmp/round-trip-project"),
            tmp.path().to_path_buf(),
        );

        let files = vec![
            make_cached_file("src/main.rs", vec![make_symbol("main")]),
            make_cached_file(
                "src/lib.rs",
                vec![make_symbol("greet"), make_symbol("hello")],
            ),
        ];

        store.store(&files).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(loaded[0].symbols.len(), 1);
        assert_eq!(loaded[0].symbols[0].name, "main");
        assert_eq!(loaded[1].path, PathBuf::from("src/lib.rs"));
        assert_eq!(loaded[1].symbols.len(), 2);
    }

    // -----------------------------------------------------------------------
    // load — missing cache
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_load_missing_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(
            Path::new("/tmp/nonexistent-project"),
            tmp.path().to_path_buf(),
        );
        let result = store.load();
        assert!(matches!(result, Err(CacheError::NotFound)));
    }

    // -----------------------------------------------------------------------
    // load — corrupt data
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_load_corrupt_data_returns_corrupt() {
        let tmp = TempDir::new().unwrap();
        let store =
            CacheStore::with_dir(Path::new("/tmp/corrupt-project"), tmp.path().to_path_buf());
        fs::create_dir_all(store.cache_path().parent().unwrap()).unwrap();
        fs::write(store.cache_path(), b"not valid bincode data").unwrap();

        let result = store.load();
        assert!(matches!(result, Err(CacheError::Corrupt(_))));
    }

    // -----------------------------------------------------------------------
    // clear
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_clear_removes_existing_cache() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(Path::new("/tmp/clear-project"), tmp.path().to_path_buf());
        store.store(&[make_cached_file("a.rs", vec![])]).unwrap();

        let removed = store.clear().unwrap();
        assert!(removed);
        assert!(!store.cache_path().exists());
    }

    #[test]
    fn test_cache_clear_nonexistent_returns_false() {
        let tmp = TempDir::new().unwrap();
        let store =
            CacheStore::with_dir(Path::new("/tmp/no-cache-project"), tmp.path().to_path_buf());
        let removed = store.clear().unwrap();
        assert!(!removed);
    }

    // -----------------------------------------------------------------------
    // is_valid
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_is_valid_with_matching_files() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, "fn main() {}").unwrap();

        let (mtime, size) = file_metadata(&file_path).unwrap();
        let cached = vec![CachedFile {
            path: PathBuf::from("src/main.rs"),
            mtime,
            size,
            symbols: vec![],
        }];

        let store_tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(tmp.path(), store_tmp.path().to_path_buf());

        assert!(store.is_valid(&cached, tmp.path()));
    }

    #[test]
    fn test_cache_is_valid_returns_false_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let cached = vec![CachedFile {
            path: PathBuf::from("src/gone.rs"),
            mtime: 1000,
            size: 100,
            symbols: vec![],
        }];

        let store_tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(tmp.path(), store_tmp.path().to_path_buf());

        assert!(!store.is_valid(&cached, tmp.path()));
    }

    #[test]
    fn test_cache_is_valid_returns_false_when_size_changed() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a.rs");
        fs::write(&file_path, "fn a() {}").unwrap();

        let (mtime, _size) = file_metadata(&file_path).unwrap();
        let cached = vec![CachedFile {
            path: PathBuf::from("a.rs"),
            mtime,
            size: 999, // wrong size
            symbols: vec![],
        }];

        let store_tmp = TempDir::new().unwrap();
        let store = CacheStore::with_dir(tmp.path(), store_tmp.path().to_path_buf());

        assert!(!store.is_valid(&cached, tmp.path()));
    }

    // -----------------------------------------------------------------------
    // file_metadata
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_metadata_returns_some_for_existing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, "hello").unwrap();

        let result = file_metadata(&path);
        assert!(result.is_some());
        let (mtime, size) = result.unwrap();
        assert!(mtime > 0);
        assert_eq!(size, 5);
    }

    #[test]
    fn test_file_metadata_returns_none_for_missing_file() {
        let result = file_metadata(Path::new("/nonexistent/file.txt"));
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // get_file_mtime_size (public API)
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_file_mtime_size_returns_correct_values() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("data.bin");
        fs::write(&path, "abcdef").unwrap();

        let (mtime, size) = get_file_mtime_size(&path).unwrap();
        assert!(mtime > 0);
        assert_eq!(size, 6);
    }

    #[test]
    fn test_get_file_mtime_size_returns_none_for_missing_file() {
        assert!(get_file_mtime_size(Path::new("/no/such/file")).is_none());
    }

    // -----------------------------------------------------------------------
    // clear_all_caches
    // -----------------------------------------------------------------------

    #[test]
    fn test_clear_all_caches_removes_cache_directory() {
        let tmp = TempDir::new().unwrap();
        let cache_dir = tmp.path().join("cq-cache-test");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_dir.join("some.bin"), b"data").unwrap();

        std::env::set_var("CQ_CACHE_DIR", &cache_dir);
        let removed = clear_all_caches().unwrap();
        assert!(removed);
        assert!(!cache_dir.exists());

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_clear_all_caches_nonexistent_returns_false() {
        let tmp = TempDir::new().unwrap();
        let cache_dir = tmp.path().join("nonexistent-cache");

        std::env::set_var("CQ_CACHE_DIR", &cache_dir);
        let removed = clear_all_caches().unwrap();
        assert!(!removed);

        std::env::remove_var("CQ_CACHE_DIR");
    }

    // -----------------------------------------------------------------------
    // CacheError display
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_error_display_not_found() {
        let err = CacheError::NotFound;
        assert_eq!(err.to_string(), "cache not found");
    }

    #[test]
    fn test_cache_error_display_corrupt() {
        let err = CacheError::Corrupt("bad data".to_string());
        assert!(err.to_string().contains("bad data"));
    }

    #[test]
    fn test_cache_error_display_stale() {
        let err = CacheError::Stale;
        assert_eq!(err.to_string(), "cache is stale");
    }

    #[test]
    fn test_cache_error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = CacheError::from(io_err);
        assert!(err.to_string().contains("denied"));
    }
}
