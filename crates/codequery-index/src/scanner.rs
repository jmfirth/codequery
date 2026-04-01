//! Parallel file scanner for project-wide symbol extraction.
//!
//! Discovers source files, parses them in parallel with rayon, and extracts
//! symbols from each file. This is the core infrastructure for wide commands
//! (refs, callers, symbols, tree).

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use codequery_core::{discover_files, language_for_file, Symbol};
use codequery_parse::{extract_symbols, Parser};

use crate::cache::{self, CacheStore, CachedFile};
use crate::error::Result;
use crate::grep;

/// Result of scanning a single file: the file path, extracted symbols, source text, and parse tree.
///
/// Retaining the `tree_sitter::Tree` avoids re-parsing when downstream consumers
/// (reference extraction, Phase 2 stack graph construction) need the AST.
#[derive(Debug)]
pub struct FileSymbols {
    /// The relative path to the file (relative to project root).
    pub file: PathBuf,
    /// Symbols extracted from this file.
    pub symbols: Vec<Symbol>,
    /// The full source text of the file.
    pub source: String,
    /// The tree-sitter parse tree for this file.
    pub tree: tree_sitter::Tree,
}

/// Scan and parse a single file, extracting its symbols.
///
/// Returns `None` if the file's language is unrecognized or if parsing fails
/// (error-tolerant: parse failures are silently skipped).
fn scan_single_file(root: &Path, relative: &Path) -> Option<FileSymbols> {
    let absolute = root.join(relative);

    // Try builtin language with compiled-in grammar (safe for parallel execution).
    // WASM grammar loading is NOT thread-safe in rayon workers — tree-sitter's
    // wasmtime engine can crash (SIGBUS) when multiple threads create engines
    // concurrently. So we only use compiled-in grammars in the parallel scanner.
    // Runtime languages are handled by the sequential CLI command paths.
    let language = language_for_file(&absolute)?;
    codequery_parse::compiled_grammar(language)?;

    let mut parser = Parser::for_language(language).ok()?;
    let (source, tree) = parser.parse_file(&absolute).ok()?;
    let symbols = extract_symbols(&source, &tree, relative, language);
    Some(FileSymbols {
        file: relative.to_path_buf(),
        symbols,
        source,
        tree,
    })
}

/// Scan all source files in a project, parsing them in parallel.
///
/// Discovers files under `root` (optionally scoped to a subdirectory),
/// then parses each file in parallel with rayon and extracts symbols.
/// Files that fail to parse are silently skipped (error-tolerant).
///
/// Results are sorted by file path for deterministic output regardless
/// of parallel execution order.
///
/// # Errors
///
/// Returns an error if file discovery itself fails (e.g., path does not exist).
pub fn scan_project(root: &Path, scope: Option<&Path>) -> Result<Vec<FileSymbols>> {
    let files = discover_files(root, scope)?;

    let mut results: Vec<FileSymbols> = files
        .par_iter()
        .filter_map(|f| scan_single_file(root, f))
        .collect();

    results.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(results)
}

/// Scan files matching a text pre-filter, parsing in parallel.
///
/// Like [`scan_project`], but first applies a grep pre-filter so only files
/// containing `filter` (at a word boundary) are parsed. This avoids unnecessary
/// parsing for narrow commands.
///
/// # Errors
///
/// Returns an error if file discovery fails.
pub fn scan_with_filter(
    root: &Path,
    scope: Option<&Path>,
    filter: &str,
) -> Result<Vec<FileSymbols>> {
    let files = discover_files(root, scope)?;
    let filtered = grep::filter_files(&files, root, filter);

    let mut results: Vec<FileSymbols> = filtered
        .par_iter()
        .filter_map(|f| scan_single_file(root, f))
        .collect();

    results.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(results)
}

/// Scan all source files in a project, using disk cache when available.
///
/// When `use_cache` is true and a valid cache exists for the project,
/// returns symbols from the cache without re-parsing. On cache miss
/// or invalidation, performs a full scan and stores the result in cache.
///
/// When `use_cache` is false, behaves identically to [`scan_project`].
///
/// # Errors
///
/// Returns an error if file discovery itself fails (e.g., path does not exist).
pub fn scan_project_cached(
    root: &Path,
    scope: Option<&Path>,
    use_cache: bool,
) -> Result<Vec<FileSymbols>> {
    if !use_cache {
        return scan_project(root, scope);
    }

    // Attempt to load and validate the cache
    if let Some(store) = CacheStore::new(root) {
        if let Ok(cached) = store.load() {
            if store.is_valid(&cached, root) {
                // Cache hit — rebuild FileSymbols from cached data.
                // Note: we don't have the source text or tree in the cache,
                // so we need to re-read source and re-parse for FileSymbols.
                // The cache saves us the symbol extraction step.
                // For now, we re-parse but skip extraction if cache is valid.
                // A simpler approach: on cache hit, do a full re-parse but
                // use the cached symbols instead of re-extracting.
                return Ok(rebuild_from_cache(&cached, root, scope));
            }
        }
    }

    // Cache miss — full scan, then store
    let results = scan_project(root, scope)?;

    if let Some(store) = CacheStore::new(root) {
        let entries = build_cache_entries(&results, root);
        // Silently ignore write failures (read-only fs, etc.)
        let _ = store.store(&entries);
    }

    Ok(results)
}

/// Rebuild `FileSymbols` from cached data by re-reading source and re-parsing trees,
/// but using cached symbols to skip extraction.
///
/// If scope is set, only files within the scope are included.
fn rebuild_from_cache(
    cached: &[CachedFile],
    root: &Path,
    scope: Option<&Path>,
) -> Vec<FileSymbols> {
    let mut results: Vec<FileSymbols> = cached
        .par_iter()
        .filter(|entry| scope.is_none_or(|s| entry.path.starts_with(s)))
        .filter_map(|entry| {
            let absolute = root.join(&entry.path);

            // Only use compiled-in grammars in parallel context (WASM not thread-safe)
            let language = language_for_file(&absolute)?;
            codequery_parse::compiled_grammar(language)?;
            let mut parser = Parser::for_language(language).ok()?;

            let (source, tree) = parser.parse_file(&absolute).ok()?;

            Some(FileSymbols {
                file: entry.path.clone(),
                symbols: entry.symbols.clone(),
                source,
                tree,
            })
        })
        .collect();

    results.sort_by(|a, b| a.file.cmp(&b.file));
    results
}

/// Build cache entries from scan results, including file metadata.
fn build_cache_entries(results: &[FileSymbols], root: &Path) -> Vec<CachedFile> {
    results
        .iter()
        .filter_map(|fs| {
            let abs_path = root.join(&fs.file);
            let (mtime, size) = cache::get_file_mtime_size(&abs_path)?;
            Some(CachedFile {
                path: fs.file.clone(),
                mtime,
                size,
                symbols: fs.symbols.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    /// Create a minimal project with source files.
    fn create_project(files: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        for (name, content) in files {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }
        tmp
    }

    // -----------------------------------------------------------------------
    // scan_project
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_project_finds_rust_symbols_in_fixture() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let results = scan_project(&fixture, None).unwrap();

        // Should find results for multiple files
        assert!(!results.is_empty());

        // Collect all symbol names
        let all_names: Vec<&str> = results
            .iter()
            .flat_map(|fs| fs.symbols.iter().map(|s| s.name.as_str()))
            .collect();

        // The fixture has at least "greet" in lib.rs
        assert!(
            all_names.contains(&"greet"),
            "expected 'greet' in symbols, got: {all_names:?}"
        );
    }

    #[test]
    fn test_scan_project_returns_results_for_all_files() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let results = scan_project(&fixture, None).unwrap();

        let scanned_files: HashSet<&Path> = results.iter().map(|fs| fs.file.as_path()).collect();

        // The fixture has multiple .rs files
        assert!(
            scanned_files.len() > 1,
            "expected multiple files, got: {scanned_files:?}"
        );
    }

    #[test]
    fn test_scan_project_parallel_matches_sequential() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");

        // Run scan twice — parallel execution with rayon should produce
        // deterministic results because we sort by file path.
        let results1 = scan_project(&fixture, None).unwrap();
        let results2 = scan_project(&fixture, None).unwrap();

        assert_eq!(results1.len(), results2.len());
        for (a, b) in results1.iter().zip(results2.iter()) {
            assert_eq!(a.file, b.file);
            assert_eq!(a.symbols.len(), b.symbols.len());
            for (sa, sb) in a.symbols.iter().zip(b.symbols.iter()) {
                assert_eq!(sa.name, sb.name);
                assert_eq!(sa.kind, sb.kind);
            }
        }
    }

    #[test]
    fn test_scan_project_empty_project_returns_empty() {
        let tmp = create_project(&[]);
        let results = scan_project(tmp.path(), None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_project_includes_source_text() {
        let tmp = create_project(&[("main.rs", "fn hello() {}\n")]);
        let results = scan_project(tmp.path(), None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "fn hello() {}\n");
    }

    #[test]
    fn test_scan_project_with_scope() {
        let tmp = create_project(&[
            ("src/a.rs", "fn in_src() {}"),
            ("tests/b.rs", "fn in_tests() {}"),
        ]);
        let results = scan_project(tmp.path(), Some(Path::new("src"))).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file, PathBuf::from("src/a.rs"));
    }

    // -----------------------------------------------------------------------
    // scan_with_filter
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_with_filter_only_parses_matching_files() {
        let tmp = create_project(&[
            ("a.rs", "fn greet() {}"),
            ("b.rs", "fn hello() {}"),
            ("c.rs", "fn farewell() { greet(); }"),
        ]);

        let results = scan_with_filter(tmp.path(), None, "greet").unwrap();
        let files: Vec<&Path> = results.iter().map(|fs| fs.file.as_path()).collect();

        // a.rs contains "greet" — matches
        assert!(files.contains(&Path::new("a.rs")));
        // b.rs does not contain "greet" — should not be parsed
        assert!(!files.contains(&Path::new("b.rs")));
        // c.rs contains "greet()" — matches at word boundary
        assert!(files.contains(&Path::new("c.rs")));
    }

    #[test]
    fn test_scan_with_filter_empty_project() {
        let tmp = create_project(&[]);
        let results = scan_with_filter(tmp.path(), None, "anything").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_with_filter_no_matches() {
        let tmp = create_project(&[("a.rs", "fn hello() {}"), ("b.rs", "fn world() {}")]);
        let results = scan_with_filter(tmp.path(), None, "nonexistent").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_with_filter_respects_word_boundaries() {
        let tmp = create_project(&[
            ("a.rs", "fn greeter() {}"), // "greet" is a substring, not word boundary
            ("b.rs", "fn greet() {}"),   // "greet" at word boundary
        ]);

        let results = scan_with_filter(tmp.path(), None, "greet").unwrap();
        let files: Vec<&Path> = results.iter().map(|fs| fs.file.as_path()).collect();

        assert!(!files.contains(&Path::new("a.rs")));
        assert!(files.contains(&Path::new("b.rs")));
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_project_nonexistent_root_returns_error() {
        let result = scan_project(Path::new("/nonexistent/project"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_with_filter_nonexistent_root_returns_error() {
        let result = scan_with_filter(Path::new("/nonexistent/project"), None, "test");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // scan_project_cached
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_project_cached_without_cache_same_as_scan_project() {
        let tmp = create_project(&[("main.rs", "fn hello() {}\n")]);
        let results = scan_project_cached(tmp.path(), None, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "fn hello() {}\n");
    }

    #[test]
    fn test_scan_project_cached_populates_and_reuses_cache() {
        let cache_tmp = TempDir::new().unwrap();
        std::env::set_var("CQ_CACHE_DIR", cache_tmp.path());

        let project = create_project(&[("lib.rs", "fn cached_func() {}\n")]);

        // First scan — cache miss, should scan and store
        let results1 = scan_project_cached(project.path(), None, true).unwrap();
        assert_eq!(results1.len(), 1);
        assert_eq!(results1[0].symbols[0].name, "cached_func");

        // Second scan — cache hit, should use cached symbols
        let results2 = scan_project_cached(project.path(), None, true).unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].symbols[0].name, "cached_func");

        // Results should match
        assert_eq!(results1[0].file, results2[0].file);
        assert_eq!(results1[0].symbols.len(), results2[0].symbols.len());

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_scan_project_cached_invalidates_on_file_change() {
        let cache_tmp = TempDir::new().unwrap();
        std::env::set_var("CQ_CACHE_DIR", cache_tmp.path());

        let project = create_project(&[("lib.rs", "fn original() {}\n")]);

        // First scan — populates cache
        let results1 = scan_project_cached(project.path(), None, true).unwrap();
        assert_eq!(results1[0].symbols[0].name, "original");

        // Modify the file (change content and thus size)
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(project.path().join("lib.rs"), "fn modified_function() {}\n").unwrap();

        // Second scan — cache should be invalidated, re-scans
        let results2 = scan_project_cached(project.path(), None, true).unwrap();
        assert_eq!(results2[0].symbols[0].name, "modified_function");

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_scan_project_cached_empty_project() {
        let cache_tmp = TempDir::new().unwrap();
        std::env::set_var("CQ_CACHE_DIR", cache_tmp.path());

        let project = create_project(&[]);
        let results = scan_project_cached(project.path(), None, true).unwrap();
        assert!(results.is_empty());

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_scan_project_cached_nonexistent_root_returns_error() {
        let result = scan_project_cached(Path::new("/nonexistent/project"), None, true);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // build_cache_entries
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_cache_entries_captures_metadata() {
        let tmp = create_project(&[("a.rs", "fn test_func() {}")]);

        let results = scan_project(tmp.path(), None).unwrap();
        let entries = build_cache_entries(&results, tmp.path());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("a.rs"));
        assert!(entries[0].mtime > 0);
        assert!(entries[0].size > 0);
        assert_eq!(entries[0].symbols.len(), results[0].symbols.len());
    }
}
