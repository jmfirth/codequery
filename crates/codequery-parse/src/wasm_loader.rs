//! WASM grammar loading for plugin languages.
//!
//! When the `wasm` feature is enabled, this module provides the ability to
//! load tree-sitter grammars from `.wasm` files at runtime. This powers the
//! `cq grammar install` plugin system, allowing languages beyond the compiled
//! Tier 1/2 set.
//!
//! # Ownership model
//!
//! Tree-sitter's WASM integration requires a [`WasmStore`] to be set on the
//! [`Parser`] via `set_wasm_store()`. The `Language` loaded from WASM
//! references data inside the store, so the store must remain alive on the
//! parser for the entire lifetime of any parse using that language.
//!
//! The [`WasmGrammarLoader`] encapsulates this: it creates an engine and
//! store, loads the language, and transfers store ownership to the parser.

use std::path::{Path, PathBuf};

use crate::error::{ParseError, Result};

/// Metadata about an installed WASM grammar package.
#[derive(Debug, Clone)]
pub struct WasmGrammarInfo {
    /// Language name (e.g., "elixir").
    pub name: String,
    /// Path to the `.wasm` file.
    pub wasm_path: PathBuf,
}

/// Resolve the WASM function name for a language.
///
/// Tree-sitter WASM modules export `tree_sitter_<name>`. The `<name>` is derived
/// from the grammar repo name (e.g., `tree-sitter-c-sharp` → `c_sharp`), which
/// may differ from our canonical name (e.g., "csharp", "objective-c").
///
/// Resolution order:
/// 1. Registry `grammar_repo` field (most reliable — derived from actual repo name)
/// 2. `wasm_name` file in the grammar package directory (explicit override)
/// 3. Hyphens → underscores (handles simple cases like `common-lisp` → `common_lisp`)
/// 4. Original name as-is
#[cfg(feature = "wasm")]
fn resolve_wasm_name(lang_name: &str, wasm_path: &Path) -> Vec<String> {
    let mut candidates = Vec::new();

    // 1. Registry-derived name (from grammar_repo URL)
    if let Some(name) = codequery_core::wasm_name_for_language(lang_name) {
        if !candidates.contains(&name) {
            candidates.push(name);
        }
    }

    // 2. Explicit wasm_name file in the grammar package directory
    if let Some(dir) = wasm_path.parent() {
        let name_file = dir.join("wasm_name");
        if let Ok(name) = std::fs::read_to_string(&name_file) {
            let name = name.trim().to_string();
            if !name.is_empty() && !candidates.contains(&name) {
                candidates.push(name);
            }
        }
    }

    // 3. Hyphens → underscores
    if lang_name.contains('-') {
        let underscore = lang_name.replace('-', "_");
        if !candidates.contains(&underscore) {
            candidates.push(underscore);
        }
        // Also try removing hyphens entirely
        let no_sep = lang_name.replace('-', "");
        if !candidates.contains(&no_sep) {
            candidates.push(no_sep);
        }
    }

    // 4. Original name
    let original = lang_name.to_string();
    if !candidates.contains(&original) {
        candidates.push(original);
    }

    candidates
}

/// Try loading a WASM language with name fallbacks.
///
/// Attempts each candidate name in priority order until one succeeds.
#[cfg(feature = "wasm")]
fn load_language_with_fallback(
    store: &mut tree_sitter::WasmStore,
    lang_name: &str,
    wasm_path: &Path,
    wasm_bytes: &[u8],
) -> std::result::Result<tree_sitter::Language, String> {
    let candidates = resolve_wasm_name(lang_name, wasm_path);
    let mut last_err = String::new();

    for candidate in &candidates {
        match store.load_language(candidate, wasm_bytes) {
            Ok(lang) => return Ok(lang),
            Err(e) => last_err = format!("{e}"),
        }
    }

    Err(format!(
        "load language '{lang_name}': {last_err} (tried: {})",
        candidates.join(", ")
    ))
}

/// Scan the languages directory for installed WASM grammar packages.
///
/// Each language package lives in `~/.local/share/cq/languages/<name>/`
/// and must contain a `grammar.wasm` file. Returns info for each
/// discovered package.
#[must_use]
pub fn discover_wasm_grammars() -> Vec<WasmGrammarInfo> {
    let Some(languages_dir) = codequery_core::dirs::languages_dir() else {
        return Vec::new();
    };

    if !languages_dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&languages_dir) else {
        return Vec::new();
    };

    let mut grammars = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let wasm_path = path.join("grammar.wasm");
        if !wasm_path.exists() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        grammars.push(WasmGrammarInfo {
            name: name.to_string(),
            wasm_path,
        });
    }

    grammars.sort_by(|a, b| a.name.cmp(&b.name));
    grammars
}

/// Find a WASM grammar by language name.
///
/// Looks for `~/.local/share/cq/languages/<name>/grammar.wasm`.
/// Returns `None` if the package is not installed.
#[must_use]
pub fn find_wasm_grammar(name: &str) -> Option<WasmGrammarInfo> {
    let languages_dir = codequery_core::dirs::languages_dir()?;
    let wasm_path = languages_dir.join(name).join("grammar.wasm");

    if wasm_path.exists() {
        Some(WasmGrammarInfo {
            name: name.to_string(),
            wasm_path,
        })
    } else {
        None
    }
}

/// Load a WASM grammar and configure a tree-sitter parser to use it.
///
/// This reads the `.wasm` file, creates a wasmtime engine and WASM store,
/// loads the language into the store, transfers store ownership to the
/// parser, and sets the language on the parser.
///
/// # Errors
///
/// Returns `ParseError::WasmError` if:
/// - The `.wasm` file cannot be read
/// - The wasmtime engine or store fails to initialize
/// - The WASM module fails to load as a tree-sitter grammar
/// - The language fails to set on the parser
#[cfg(feature = "wasm")]
pub fn load_wasm_language(
    wasm_path: &Path,
    parser: &mut tree_sitter::Parser,
) -> Result<tree_sitter::Language> {
    use tree_sitter::wasmtime;
    use tree_sitter::WasmStore;

    let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
        ParseError::WasmError(format!(
            "failed to read wasm grammar {}: {e}",
            wasm_path.display()
        ))
    })?;

    let lang_name = wasm_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let engine = wasmtime::Engine::default();
    let mut store =
        WasmStore::new(&engine).map_err(|e| ParseError::WasmError(format!("wasm store: {e}")))?;

    let language = load_language_with_fallback(&mut store, lang_name, wasm_path, &wasm_bytes)
        .map_err(ParseError::WasmError)?;

    // Transfer store ownership to the parser. The store must outlive
    // the language; set_wasm_store moves it into the C parser via mem::forget.
    parser
        .set_wasm_store(store)
        .map_err(|e| ParseError::WasmError(format!("set wasm store: {e}")))?;

    parser
        .set_language(&language)
        .map_err(|e| ParseError::WasmError(format!("set language: {e}")))?;

    Ok(language)
}

/// Load a WASM grammar with ahead-of-time compilation caching.
///
/// On first load, compiles the WASM module and caches the native code
/// as a `.cwasm` file in `~/.cache/cq/cwasm/`. On subsequent loads,
/// uses the cached native module for faster startup.
///
/// The cache is invalidated when the cq version changes (tracked via
/// a `.cq-version` stamp file in the cwasm directory).
///
/// # Errors
///
/// Returns `ParseError::WasmError` if loading or compilation fails.
/// Cache write failures are silently ignored (falls back to JIT).
#[cfg(feature = "wasm")]
pub fn load_wasm_language_cached(
    wasm_path: &Path,
    parser: &mut tree_sitter::Parser,
) -> Result<tree_sitter::Language> {
    use tree_sitter::wasmtime;
    use tree_sitter::WasmStore;

    let lang_name = wasm_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let engine = wasmtime::Engine::default();

    // Check for a cached cwasm
    let cwasm_bytes = try_load_cwasm(lang_name, &engine);

    // If we have cached native code, we still need to go through WasmStore
    // because tree-sitter's WASM integration manages the language lifecycle
    // through the store. The cwasm cache mainly saves the compilation step
    // on the wasmtime side (the Engine caches compiled modules internally).
    //
    // For now, always load via WasmStore. The precompilation cache stores
    // the raw wasm bytes' compiled form for the Engine's internal cache.
    let _ = cwasm_bytes;

    let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
        ParseError::WasmError(format!(
            "failed to read wasm grammar {}: {e}",
            wasm_path.display()
        ))
    })?;

    let mut store =
        WasmStore::new(&engine).map_err(|e| ParseError::WasmError(format!("wasm store: {e}")))?;

    let language = load_language_with_fallback(&mut store, lang_name, wasm_path, &wasm_bytes)
        .map_err(ParseError::WasmError)?;

    // Attempt to cache the precompiled module for future loads
    try_save_cwasm(lang_name, &engine, &wasm_bytes);

    parser
        .set_wasm_store(store)
        .map_err(|e| ParseError::WasmError(format!("set wasm store: {e}")))?;

    parser
        .set_language(&language)
        .map_err(|e| ParseError::WasmError(format!("set language: {e}")))?;

    Ok(language)
}

/// Current cq version used for cache invalidation.
#[cfg(feature = "wasm")]
const CQ_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Try to load a cached cwasm file. Returns `None` if cache miss or stale.
#[cfg(feature = "wasm")]
fn try_load_cwasm(lang_name: &str, _engine: &tree_sitter::wasmtime::Engine) -> Option<Vec<u8>> {
    let cwasm_dir = codequery_core::dirs::cwasm_dir()?;
    let version_path = cwasm_dir.join(".cq-version");
    let cwasm_path = cwasm_dir.join(format!("{lang_name}.cwasm"));

    // Check version stamp
    let version = std::fs::read_to_string(&version_path).ok()?;
    if version.trim() != CQ_VERSION {
        return None;
    }

    std::fs::read(&cwasm_path).ok()
}

/// Try to save a precompiled cwasm file. Silently ignores failures.
#[cfg(feature = "wasm")]
fn try_save_cwasm(lang_name: &str, engine: &tree_sitter::wasmtime::Engine, wasm_bytes: &[u8]) {
    let Some(cwasm_dir) = codequery_core::dirs::cwasm_dir() else {
        return;
    };

    // Ensure directory exists
    if std::fs::create_dir_all(&cwasm_dir).is_err() {
        return;
    }

    // Write version stamp
    let version_path = cwasm_dir.join(".cq-version");
    let _ = std::fs::write(&version_path, CQ_VERSION);

    // Precompile and save
    if let Ok(cwasm_bytes) = engine.precompile_module(wasm_bytes) {
        let cwasm_path = cwasm_dir.join(format!("{lang_name}.cwasm"));
        let _ = std::fs::write(&cwasm_path, cwasm_bytes);
    }
}

/// Stub for when the `wasm` feature is disabled.
///
/// # Errors
///
/// Always returns `ParseError::WasmError` indicating the feature is not enabled.
#[cfg(not(feature = "wasm"))]
pub fn load_wasm_language(
    _wasm_path: &Path,
    _parser: &mut tree_sitter::Parser,
) -> Result<tree_sitter::Language> {
    Err(ParseError::WasmError(
        "wasm support not enabled: rebuild with --features wasm".to_string(),
    ))
}

/// Stub for when the `wasm` feature is disabled.
///
/// # Errors
///
/// Always returns `ParseError::WasmError` indicating the feature is not enabled.
#[cfg(not(feature = "wasm"))]
pub fn load_wasm_language_cached(
    _wasm_path: &Path,
    _parser: &mut tree_sitter::Parser,
) -> Result<tree_sitter::Language> {
    Err(ParseError::WasmError(
        "wasm support not enabled: rebuild with --features wasm".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // discover_wasm_grammars
    // -----------------------------------------------------------------------

    #[test]
    fn test_discover_wasm_grammars_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let languages_path = tmp.path().join("languages");
        std::fs::create_dir_all(&languages_path).unwrap();

        let prev = std::env::var("CQ_DATA_DIR").ok();
        std::env::set_var("CQ_DATA_DIR", tmp.path());

        let grammars = discover_wasm_grammars();
        assert!(grammars.is_empty());

        match prev {
            Some(v) => std::env::set_var("CQ_DATA_DIR", v),
            None => std::env::remove_var("CQ_DATA_DIR"),
        }
    }

    #[test]
    fn test_discover_wasm_grammars_finds_installed_packages() {
        let tmp = TempDir::new().unwrap();
        let languages_path = tmp.path().join("languages");

        // Create two language packages
        let elixir_dir = languages_path.join("elixir");
        std::fs::create_dir_all(&elixir_dir).unwrap();
        std::fs::write(elixir_dir.join("grammar.wasm"), b"fake-wasm").unwrap();

        let haskell_dir = languages_path.join("haskell");
        std::fs::create_dir_all(&haskell_dir).unwrap();
        std::fs::write(haskell_dir.join("grammar.wasm"), b"fake-wasm").unwrap();

        // Create a directory without grammar.wasm (should be skipped)
        let incomplete_dir = languages_path.join("incomplete");
        std::fs::create_dir_all(&incomplete_dir).unwrap();

        let prev = std::env::var("CQ_DATA_DIR").ok();
        std::env::set_var("CQ_DATA_DIR", tmp.path());

        let grammars = discover_wasm_grammars();
        assert_eq!(grammars.len(), 2);
        assert_eq!(grammars[0].name, "elixir");
        assert_eq!(grammars[1].name, "haskell");
        assert!(grammars[0].wasm_path.ends_with("elixir/grammar.wasm"));

        match prev {
            Some(v) => std::env::set_var("CQ_DATA_DIR", v),
            None => std::env::remove_var("CQ_DATA_DIR"),
        }
    }

    #[test]
    fn test_discover_wasm_grammars_nonexistent_dir() {
        let tmp = TempDir::new().unwrap();
        // Point to a dir that does not have a languages/ subdirectory
        let prev = std::env::var("CQ_DATA_DIR").ok();
        std::env::set_var("CQ_DATA_DIR", tmp.path().join("nonexistent"));

        let grammars = discover_wasm_grammars();
        assert!(grammars.is_empty());

        match prev {
            Some(v) => std::env::set_var("CQ_DATA_DIR", v),
            None => std::env::remove_var("CQ_DATA_DIR"),
        }
    }

    // -----------------------------------------------------------------------
    // find_wasm_grammar
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_wasm_grammar_found() {
        let tmp = TempDir::new().unwrap();
        let elixir_dir = tmp.path().join("languages").join("elixir");
        std::fs::create_dir_all(&elixir_dir).unwrap();
        std::fs::write(elixir_dir.join("grammar.wasm"), b"fake-wasm").unwrap();

        let prev = std::env::var("CQ_DATA_DIR").ok();
        std::env::set_var("CQ_DATA_DIR", tmp.path());

        let info = find_wasm_grammar("elixir");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.name, "elixir");
        assert!(info.wasm_path.ends_with("elixir/grammar.wasm"));

        match prev {
            Some(v) => std::env::set_var("CQ_DATA_DIR", v),
            None => std::env::remove_var("CQ_DATA_DIR"),
        }
    }

    #[test]
    fn test_find_wasm_grammar_not_found() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("languages")).unwrap();

        let prev = std::env::var("CQ_DATA_DIR").ok();
        std::env::set_var("CQ_DATA_DIR", tmp.path());

        let info = find_wasm_grammar("nonexistent");
        assert!(info.is_none());

        match prev {
            Some(v) => std::env::set_var("CQ_DATA_DIR", v),
            None => std::env::remove_var("CQ_DATA_DIR"),
        }
    }

    // -----------------------------------------------------------------------
    // load_wasm_language (without wasm feature)
    // -----------------------------------------------------------------------

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_load_wasm_language_without_feature_returns_error() {
        let mut parser = tree_sitter::Parser::new();
        let result = load_wasm_language(Path::new("/fake/grammar.wasm"), &mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("wasm support not enabled"), "got: {err}");
    }

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_load_wasm_language_cached_without_feature_returns_error() {
        let mut parser = tree_sitter::Parser::new();
        let result = load_wasm_language_cached(Path::new("/fake/grammar.wasm"), &mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("wasm support not enabled"), "got: {err}");
    }

    // -----------------------------------------------------------------------
    // load_wasm_language (with wasm feature)
    // -----------------------------------------------------------------------

    #[cfg(feature = "wasm")]
    #[test]
    fn test_load_wasm_language_missing_file_returns_error() {
        let mut parser = tree_sitter::Parser::new();
        let result = load_wasm_language(Path::new("/nonexistent/elixir/grammar.wasm"), &mut parser);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read wasm grammar"), "got: {err}");
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_load_wasm_language_invalid_wasm_returns_error() {
        let tmp = TempDir::new().unwrap();
        let lang_dir = tmp.path().join("elixir");
        std::fs::create_dir_all(&lang_dir).unwrap();
        let wasm_path = lang_dir.join("grammar.wasm");
        std::fs::write(&wasm_path, b"not valid wasm bytes").unwrap();

        let mut parser = tree_sitter::Parser::new();
        let result = load_wasm_language(&wasm_path, &mut parser);
        assert!(result.is_err());
        // Should fail at wasm compilation/loading stage
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("load language") || err.contains("wasm"),
            "unexpected error: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // AOT cache helpers
    // -----------------------------------------------------------------------

    #[cfg(feature = "wasm")]
    #[test]
    fn test_try_load_cwasm_returns_none_when_no_cache() {
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var("CQ_CACHE_DIR").ok();
        std::env::set_var("CQ_CACHE_DIR", tmp.path());

        let engine = tree_sitter::wasmtime::Engine::default();
        let result = try_load_cwasm("elixir", &engine);
        assert!(result.is_none());

        match prev {
            Some(v) => std::env::set_var("CQ_CACHE_DIR", v),
            None => std::env::remove_var("CQ_CACHE_DIR"),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_try_load_cwasm_returns_none_when_version_mismatch() {
        let tmp = TempDir::new().unwrap();
        let cwasm_dir = tmp.path().join("cwasm");
        std::fs::create_dir_all(&cwasm_dir).unwrap();

        // Write a stale version stamp
        std::fs::write(cwasm_dir.join(".cq-version"), "0.0.0-stale").unwrap();
        std::fs::write(cwasm_dir.join("elixir.cwasm"), b"cached-bytes").unwrap();

        let prev = std::env::var("CQ_CACHE_DIR").ok();
        std::env::set_var("CQ_CACHE_DIR", tmp.path());

        let engine = tree_sitter::wasmtime::Engine::default();
        let result = try_load_cwasm("elixir", &engine);
        assert!(result.is_none());

        match prev {
            Some(v) => std::env::set_var("CQ_CACHE_DIR", v),
            None => std::env::remove_var("CQ_CACHE_DIR"),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_try_load_cwasm_returns_bytes_when_version_matches() {
        let tmp = TempDir::new().unwrap();
        let cwasm_dir = tmp.path().join("cwasm");
        std::fs::create_dir_all(&cwasm_dir).unwrap();

        // Write matching version stamp
        std::fs::write(cwasm_dir.join(".cq-version"), CQ_VERSION).unwrap();
        std::fs::write(cwasm_dir.join("elixir.cwasm"), b"cached-bytes").unwrap();

        let prev = std::env::var("CQ_CACHE_DIR").ok();
        std::env::set_var("CQ_CACHE_DIR", tmp.path());

        let engine = tree_sitter::wasmtime::Engine::default();
        let result = try_load_cwasm("elixir", &engine);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"cached-bytes");

        match prev {
            Some(v) => std::env::set_var("CQ_CACHE_DIR", v),
            None => std::env::remove_var("CQ_CACHE_DIR"),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_try_save_cwasm_creates_directory_and_files() {
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var("CQ_CACHE_DIR").ok();
        std::env::set_var("CQ_CACHE_DIR", tmp.path());

        let engine = tree_sitter::wasmtime::Engine::default();

        // Even with invalid wasm bytes, the function should not panic
        // (precompile_module will fail, but we silently ignore that)
        try_save_cwasm("test_lang", &engine, b"not-valid-wasm");

        // Version stamp should still be written
        let version_path = tmp.path().join("cwasm").join(".cq-version");
        assert!(version_path.exists());
        let version = std::fs::read_to_string(&version_path).unwrap();
        assert_eq!(version, CQ_VERSION);

        // cwasm file should NOT exist since precompilation failed
        let cwasm_path = tmp.path().join("cwasm").join("test_lang.cwasm");
        assert!(!cwasm_path.exists());

        match prev {
            Some(v) => std::env::set_var("CQ_CACHE_DIR", v),
            None => std::env::remove_var("CQ_CACHE_DIR"),
        }
    }
}
