//! Runtime loading of tree-sitter grammar shared libraries (Tier 3).
//!
//! Scans a well-known directory for `.so` / `.dylib` files named
//! `tree-sitter-<lang>` and loads them via `libloading`. Each library
//! must export a C function `tree_sitter_<lang>` returning a
//! `*const ()` that is really a `tree_sitter::Language` pointer.
//!
//! This is inherently unsafe (dlopen + FFI). The unsafe surface is
//! kept minimal and every unsafe block has a `// SAFETY:` comment.

use std::path::{Path, PathBuf};

use crate::error::{ParseError, Result};

/// File extension for shared libraries on the current platform.
#[cfg(target_os = "macos")]
const LIB_EXT: &str = "dylib";

#[cfg(target_os = "linux")]
const LIB_EXT: &str = "so";

#[cfg(target_os = "windows")]
const LIB_EXT: &str = "dll";

/// Return the directory where runtime grammars are expected.
///
/// Checks `$XDG_DATA_HOME/cq/grammars/` first, then falls back to
/// `$HOME/.local/share/cq/grammars/`.
///
/// Returns `None` if neither environment variable is set.
fn grammar_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let dir = PathBuf::from(xdg).join("cq/grammars");
        return Some(dir);
    }

    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".local/share/cq/grammars"));
    }

    None
}

/// Load a runtime tree-sitter grammar by language name.
///
/// Looks for `tree-sitter-<name>.<ext>` in the grammar directory and
/// loads the `tree_sitter_<name>` symbol from it.
///
/// # Errors
///
/// Returns `ParseError::LanguageError` if:
/// - The grammar directory cannot be determined
/// - The shared library file does not exist
/// - The library cannot be opened or the symbol is missing
/// - The loaded pointer cannot be converted to a valid grammar
pub fn load_runtime_grammar(name: &str) -> Result<tree_sitter::Language> {
    let dir = grammar_dir().ok_or_else(|| {
        ParseError::LanguageError(
            "cannot determine grammar directory: neither XDG_DATA_HOME nor HOME is set".to_string(),
        )
    })?;

    let lib_filename = format!("tree-sitter-{name}.{LIB_EXT}");
    let lib_path = dir.join(&lib_filename);

    if !lib_path.exists() {
        return Err(ParseError::LanguageError(format!(
            "runtime grammar not found: {}",
            lib_path.display()
        )));
    }

    load_grammar_from_path(&lib_path, name)
}

/// Load a tree-sitter grammar from a specific shared library path.
///
/// The library must export a symbol named `tree_sitter_<name>` that
/// returns a language pointer compatible with tree-sitter's ABI.
fn load_grammar_from_path(path: &Path, name: &str) -> Result<tree_sitter::Language> {
    let symbol_name = format!("tree_sitter_{name}");

    // SAFETY: We are loading a shared library from a user-controlled path.
    // The caller (the user) is responsible for placing valid tree-sitter
    // grammar libraries in the grammar directory. Loading an arbitrary
    // shared library can execute arbitrary code — this is an inherent
    // property of dlopen and is accepted for the Tier 3 extensibility
    // use case. The library is kept alive for the lifetime of the process
    // via `mem::forget` because tree-sitter references the grammar data
    // for the entire parse session.
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| {
        ParseError::LanguageError(format!(
            "failed to load grammar library {}: {e}",
            path.display()
        ))
    })?;

    // SAFETY: We look up a symbol that should be a function with the
    // tree-sitter grammar ABI: `extern "C" fn() -> *const TSLanguage`.
    // The tree-sitter convention is that grammars export exactly this
    // signature under the name `tree_sitter_<language>`. If the symbol
    // has a different signature, behavior is undefined — but this is
    // the standard tree-sitter grammar contract that all published
    // grammars follow.
    let language = unsafe {
        let func: libloading::Symbol<
            unsafe extern "C" fn() -> *const tree_sitter::ffi::TSLanguage,
        > = lib.get(symbol_name.as_bytes()).map_err(|e| {
            ParseError::LanguageError(format!(
                "symbol `{symbol_name}` not found in {}: {e}",
                path.display()
            ))
        })?;

        let raw_ptr = func();

        // SAFETY: The raw pointer returned by the grammar function is
        // the tree-sitter Language pointer. `tree_sitter::Language` can
        // be constructed from this via `from_raw()`. The pointer must
        // remain valid for the lifetime of any parser using this language,
        // which we ensure by leaking the library handle below.
        tree_sitter::Language::from_raw(raw_ptr)
    };

    // Leak the library so it stays loaded for the process lifetime.
    // Tree-sitter grammars reference static data inside the shared
    // library, so unloading it would cause use-after-free.
    std::mem::forget(lib);

    Ok(language)
}

/// List the names of all runtime grammars available in the grammar directory.
///
/// Scans for files matching `tree-sitter-<name>.<ext>` and returns the
/// `<name>` portion. Returns an empty vec if the directory does not exist
/// or cannot be read.
#[must_use]
pub fn list_runtime_grammars() -> Vec<String> {
    let Some(dir) = grammar_dir() else {
        return Vec::new();
    };

    if !dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let suffix = format!(".{LIB_EXT}");
    let mut names = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };

        if let Some(rest) = name_str.strip_prefix("tree-sitter-") {
            if let Some(lang_name) = rest.strip_suffix(&suffix) {
                if !lang_name.is_empty() {
                    names.push(lang_name.to_string());
                }
            }
        }
    }

    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // grammar_dir
    // -----------------------------------------------------------------------

    #[test]
    fn test_grammar_dir_uses_xdg_data_home_when_set() {
        let tmp = TempDir::new().unwrap();
        let xdg_path = tmp.path().to_str().unwrap();

        // Temporarily set XDG_DATA_HOME
        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", xdg_path);

        let dir = grammar_dir();
        assert_eq!(dir, Some(tmp.path().join("cq/grammars")));

        // Restore
        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    #[test]
    fn test_grammar_dir_falls_back_to_home() {
        let prev_xdg = std::env::var("XDG_DATA_HOME").ok();
        std::env::remove_var("XDG_DATA_HOME");

        let dir = grammar_dir();
        // Should use HOME
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(
                dir,
                Some(PathBuf::from(home).join(".local/share/cq/grammars"))
            );
        }

        // Restore
        if let Some(v) = prev_xdg {
            std::env::set_var("XDG_DATA_HOME", v);
        }
    }

    // -----------------------------------------------------------------------
    // list_runtime_grammars
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_runtime_grammars_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let grammar_path = tmp.path().join("cq/grammars");
        std::fs::create_dir_all(&grammar_path).unwrap();

        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", tmp.path());

        let grammars = list_runtime_grammars();
        assert!(grammars.is_empty());

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    #[test]
    fn test_list_runtime_grammars_finds_libraries() {
        let tmp = TempDir::new().unwrap();
        let grammar_path = tmp.path().join("cq/grammars");
        std::fs::create_dir_all(&grammar_path).unwrap();

        // Create fake library files
        let ext = LIB_EXT;
        std::fs::write(
            grammar_path.join(format!("tree-sitter-haskell.{ext}")),
            b"fake",
        )
        .unwrap();
        std::fs::write(
            grammar_path.join(format!("tree-sitter-elixir.{ext}")),
            b"fake",
        )
        .unwrap();
        // Non-matching file
        std::fs::write(grammar_path.join("something-else.txt"), b"fake").unwrap();

        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", tmp.path());

        let grammars = list_runtime_grammars();
        assert_eq!(grammars, vec!["elixir".to_string(), "haskell".to_string()]);

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    #[test]
    fn test_list_runtime_grammars_nonexistent_dir() {
        let tmp = TempDir::new().unwrap();
        // Point to a dir that does not have cq/grammars
        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", tmp.path());

        let grammars = list_runtime_grammars();
        assert!(grammars.is_empty());

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    // -----------------------------------------------------------------------
    // load_runtime_grammar
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_runtime_grammar_missing_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        let grammar_path = tmp.path().join("cq/grammars");
        std::fs::create_dir_all(&grammar_path).unwrap();

        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", tmp.path());

        let result = load_runtime_grammar("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("runtime grammar not found"),
            "unexpected error: {err}"
        );

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    #[test]
    fn test_load_runtime_grammar_invalid_library_returns_error() {
        let tmp = TempDir::new().unwrap();
        let grammar_path = tmp.path().join("cq/grammars");
        std::fs::create_dir_all(&grammar_path).unwrap();

        // Create a file that is not a valid shared library
        let ext = LIB_EXT;
        std::fs::write(
            grammar_path.join(format!("tree-sitter-fake.{ext}")),
            b"not a real shared library",
        )
        .unwrap();

        let prev = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_DATA_HOME", tmp.path());

        let result = load_runtime_grammar("fake");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to load grammar library"),
            "unexpected error: {err}"
        );

        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    // -----------------------------------------------------------------------
    // load_grammar_from_path
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_grammar_from_path_nonexistent_returns_error() {
        let result = load_grammar_from_path(Path::new("/nonexistent/path.so"), "test");
        assert!(result.is_err());
    }
}
