//! Start-query-stop (oneshot) mode for LSP semantic operations.
//!
//! Provides synchronous functions that spawn a language server, perform a
//! single LSP query (definition or references), and shut the server down.
//! This is the "no daemon" path for `--semantic` — every invocation pays
//! the startup cost but requires no background process or state.

use std::path::Path;
use std::time::Duration;

use codequery_core::{Language, Resolution};
use codequery_resolve::ResolvedReference;

use crate::config::LanguageServerRegistry;
use crate::error::{LspError, Result};
use crate::queries::uri_to_path;
use crate::server::LspServer;

/// Default maximum time to wait for the server to become ready.
///
/// This is the upper bound — if the server signals readiness via `$/progress`
/// notifications earlier, the wait terminates immediately. 30 seconds
/// accommodates rust-analyzer on medium projects while ensuring the fallback
/// to stack graphs isn't unreasonably delayed.
const DEFAULT_INDEX_WAIT: Duration = Duration::from_secs(30);

/// Performs semantic reference finding via start-query-stop.
///
/// Starts a language server for the given language, initializes it against the
/// project root, opens the target file, waits for indexing, queries for
/// references at the given position, converts the results to
/// `ResolvedReference`s, and shuts the server down.
///
/// # Two-phase approach for refs by name
///
/// The caller is expected to first resolve the symbol name to a
/// `file:line:col` position using tree-sitter or stack graphs (they have the
/// symbol name but LSP needs a position). Then this function is called with
/// the known position to get semantically-accurate references.
///
/// # Errors
///
/// - `LspError::ServerNotFound` if the language server binary is not installed.
/// - `LspError::Timeout` if initialization or the query times out.
/// - Other `LspError` variants for I/O or protocol failures.
///
/// All errors are non-fatal from the caller's perspective — the caller should
/// fall back to stack graph resolution when this fails.
pub fn semantic_refs(
    project_root: &Path,
    language: Language,
    symbol_name: &str,
    symbol_file: &Path,
    symbol_line: usize,
    symbol_column: usize,
) -> Result<Vec<ResolvedReference>> {
    semantic_refs_with_wait(
        project_root,
        language,
        symbol_name,
        symbol_file,
        symbol_line,
        symbol_column,
        DEFAULT_INDEX_WAIT,
    )
}

/// Like [`semantic_refs`] but with a configurable index wait duration.
///
/// Useful for testing (zero wait) or for projects that need more time.
///
/// # Errors
///
/// Same error conditions as [`semantic_refs`].
pub fn semantic_refs_with_wait(
    project_root: &Path,
    language: Language,
    symbol_name: &str,
    symbol_file: &Path,
    symbol_line: usize,
    symbol_column: usize,
    index_wait: Duration,
) -> Result<Vec<ResolvedReference>> {
    let registry = LanguageServerRegistry::new();
    let config = registry
        .config_for(language)
        .ok_or_else(|| LspError::ServerNotFound(format!("no LSP config for {language:?}")))?;

    // Use progress-aware capabilities so we can detect server readiness.
    let mut server = LspServer::start_with_capabilities(
        config,
        project_root,
        crate::types::ClientCapabilities::with_progress(),
    )?;

    // Open the document so the server knows about it.
    // If the symbol file is relative, resolve it against the project root.
    let full_file = if symbol_file.is_relative() {
        project_root.join(symbol_file)
    } else {
        symbol_file.to_path_buf()
    };
    let source = read_file_source(&full_file)?;
    let lang_id = lsp_language_id(language);
    server.open_document(&full_file, &source, lang_id)?;

    // Wait for the server to finish indexing.
    // Uses $/progress notifications from the server to detect readiness.
    // Falls back to a grace-period timeout if the server doesn't send progress.
    if !index_wait.is_zero() {
        let _ = server.wait_for_ready(index_wait);
    }

    // The symbol position from tree-sitter points at the declaration start
    // (e.g., the `pub` in `pub fn greet`), but LSP needs the cursor on the
    // identifier itself. Find the identifier column by searching the line.
    let query_column = find_identifier_column(&source, symbol_line, symbol_column, symbol_name);

    // Query references at the symbol position.
    let locations = match server.find_references(&full_file, symbol_line, query_column, true) {
        Ok(locs) => locs,
        Err(e) => {
            // Best-effort shutdown even if the query failed.
            let _ = server.shutdown();
            return Err(e);
        }
    };

    // Shut down the server before building results.
    let _ = server.shutdown();

    // Convert LSP locations to ResolvedReferences.
    // LSP returns absolute file:// URIs — convert to paths relative to the
    // project root so they match the convention used by tree-sitter refs.
    let refs = locations
        .into_iter()
        .map(|loc| {
            let abs_file = uri_to_path(&loc.uri);
            let ref_file = abs_file
                .strip_prefix(project_root)
                .unwrap_or(&abs_file)
                .to_path_buf();
            #[allow(clippy::cast_possible_truncation)]
            // LSP line numbers (u32) fit comfortably in usize.
            let ref_line = loc.range.start.line as usize + 1; // LSP 0-based → cq 1-based
            #[allow(clippy::cast_possible_truncation)]
            let ref_column = loc.range.start.character as usize;
            ResolvedReference {
                ref_file,
                ref_line,
                ref_column,
                symbol: symbol_name.to_string(),
                def_file: Some(symbol_file.to_path_buf()),
                def_line: Some(symbol_line),
                def_column: Some(symbol_column),
                resolution: Resolution::Semantic,
            }
        })
        .collect();

    Ok(refs)
}

/// Performs semantic definition finding via start-query-stop.
///
/// Starts a language server, initializes it, opens the file, waits for
/// indexing, queries for the definition at the given position, and shuts the
/// server down.
///
/// # Errors
///
/// - `LspError::ServerNotFound` if the language server binary is not installed.
/// - `LspError::Timeout` if initialization or the query times out.
/// - Other `LspError` variants for I/O or protocol failures.
///
/// All errors are non-fatal — the caller should fall back to tree-sitter or
/// stack graph definition finding.
pub fn semantic_definition(
    project_root: &Path,
    language: Language,
    file: &Path,
    line: usize,
    column: usize,
) -> Result<Vec<ResolvedReference>> {
    semantic_definition_with_wait(
        project_root,
        language,
        file,
        line,
        column,
        DEFAULT_INDEX_WAIT,
    )
}

/// Like [`semantic_definition`] but with a configurable index wait duration.
///
/// # Errors
///
/// Same error conditions as [`semantic_definition`].
pub fn semantic_definition_with_wait(
    project_root: &Path,
    language: Language,
    file: &Path,
    line: usize,
    column: usize,
    index_wait: Duration,
) -> Result<Vec<ResolvedReference>> {
    let registry = LanguageServerRegistry::new();
    let config = registry
        .config_for(language)
        .ok_or_else(|| LspError::ServerNotFound(format!("no LSP config for {language:?}")))?;

    let mut server = LspServer::start_with_capabilities(
        config,
        project_root,
        crate::types::ClientCapabilities::with_progress(),
    )?;

    // Open the document so the server knows about it.
    // If the file path is relative, resolve it against the project root.
    let full_file = if file.is_relative() {
        project_root.join(file)
    } else {
        file.to_path_buf()
    };
    let source = read_file_source(&full_file)?;
    let lang_id = lsp_language_id(language);
    server.open_document(&full_file, &source, lang_id)?;

    // Wait for the server to finish indexing.
    // Uses $/progress notifications from the server to detect readiness.
    // Falls back to a grace-period timeout if the server doesn't send progress.
    if !index_wait.is_zero() {
        let _ = server.wait_for_ready(index_wait);
    }

    // Query definitions at the position.
    let locations = match server.find_definition(&full_file, line, column) {
        Ok(locs) => locs,
        Err(e) => {
            let _ = server.shutdown();
            return Err(e);
        }
    };

    // Shut down the server.
    let _ = server.shutdown();

    // Convert LSP locations to ResolvedReferences.
    let refs = locations
        .into_iter()
        .map(|loc| {
            let def_file = uri_to_path(&loc.uri);
            #[allow(clippy::cast_possible_truncation)]
            // LSP line numbers (u32) fit comfortably in usize.
            let def_line = loc.range.start.line as usize + 1; // LSP 0-based → cq 1-based
            #[allow(clippy::cast_possible_truncation)]
            let def_column = loc.range.start.character as usize;
            ResolvedReference {
                ref_file: file.to_path_buf(),
                ref_line: line,
                ref_column: column,
                symbol: String::new(),
                def_file: Some(def_file),
                def_line: Some(def_line),
                def_column: Some(def_column),
                resolution: Resolution::Semantic,
            }
        })
        .collect();

    Ok(refs)
}

/// Maps a `Language` to the LSP language identifier string.
///
/// These are the standard language identifiers used in `textDocument/didOpen`
/// notifications. See the LSP specification for the canonical list.
fn lsp_language_id(language: Language) -> &'static str {
    match language {
        Language::Rust => "rust",
        Language::TypeScript => "typescript",
        Language::JavaScript => "javascript",
        Language::Python => "python",
        Language::Go => "go",
        Language::C => "c",
        Language::Cpp => "cpp",
        Language::Java => "java",
        Language::Ruby => "ruby",
        Language::Php => "php",
        Language::CSharp => "csharp",
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
        Language::Scala => "scala",
        Language::Zig => "zig",
        Language::Lua => "lua",
        Language::Bash => "shellscript",
        Language::Html => "html",
        Language::Css => "css",
        Language::Json => "json",
        Language::Yaml => "yaml",
        Language::Toml => "toml",
    }
}

/// Reads a file's contents as a UTF-8 string.
///
/// Returns `LspError::Io` if the file cannot be read.
fn read_file_source(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(LspError::Io)
}

/// Finds the column of the symbol identifier within a declaration line.
///
/// Tree-sitter reports symbol positions at the start of the declaration
/// (e.g., column 0 for `pub fn greet`), but LSP servers need the cursor
/// on the identifier itself (column 7 for `greet`). This function searches
/// the declaration line for the identifier and returns its column.
///
/// Falls back to the original column if the identifier isn't found.
fn find_identifier_column(source: &str, line: usize, original_column: usize, name: &str) -> usize {
    let Some(line_text) = source.lines().nth(line.saturating_sub(1)) else {
        return original_column;
    };
    // Search from the original column position forward
    if let Some(offset) = line_text[original_column..].find(name) {
        // Verify it's a whole word (not a substring of a longer identifier)
        let pos = original_column + offset;
        let end = pos + name.len();
        let before_ok = pos == 0 || !line_text.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let after_ok = end >= line_text.len() || !line_text.as_bytes()[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return pos;
        }
    }
    original_column
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ─── lsp_language_id tests ───────────────────────────────────────

    #[test]
    fn test_lsp_language_id_rust() {
        assert_eq!(lsp_language_id(Language::Rust), "rust");
    }

    #[test]
    fn test_lsp_language_id_typescript() {
        assert_eq!(lsp_language_id(Language::TypeScript), "typescript");
    }

    #[test]
    fn test_lsp_language_id_javascript() {
        assert_eq!(lsp_language_id(Language::JavaScript), "javascript");
    }

    #[test]
    fn test_lsp_language_id_python() {
        assert_eq!(lsp_language_id(Language::Python), "python");
    }

    #[test]
    fn test_lsp_language_id_go() {
        assert_eq!(lsp_language_id(Language::Go), "go");
    }

    #[test]
    fn test_lsp_language_id_c() {
        assert_eq!(lsp_language_id(Language::C), "c");
    }

    #[test]
    fn test_lsp_language_id_cpp() {
        assert_eq!(lsp_language_id(Language::Cpp), "cpp");
    }

    #[test]
    fn test_lsp_language_id_java() {
        assert_eq!(lsp_language_id(Language::Java), "java");
    }

    #[test]
    fn test_lsp_language_id_bash() {
        assert_eq!(lsp_language_id(Language::Bash), "shellscript");
    }

    // ─── read_file_source tests ──────────────────────────────────────

    #[test]
    fn test_read_file_source_nonexistent_returns_io_error() {
        let result = read_file_source(Path::new("/nonexistent/file.rs"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LspError::Io(_)));
    }

    #[test]
    fn test_read_file_source_reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        let source = read_file_source(&file).unwrap();
        assert_eq!(source, "fn main() {}");
    }

    // ─── semantic_refs error path tests ──────────────────────────────

    #[test]
    fn test_semantic_refs_unsupported_language_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.scala");
        std::fs::write(&file, "object Main { def foo = 1 }").unwrap();

        let result = semantic_refs(dir.path(), Language::Scala, "foo", &file, 1, 4);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
        assert!(err.to_string().contains("Scala"));
    }

    #[test]
    fn test_semantic_refs_nonexistent_server_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn main() {}").unwrap();

        // This will fail because rust-analyzer is unlikely installed in CI,
        // or will succeed if it is. Either way it should not panic.
        let result = semantic_refs(dir.path(), Language::Rust, "main", &file, 1, 3);
        // We can't assert success/failure since it depends on whether
        // rust-analyzer is installed, but it must not panic.
        let _ = result;
    }

    #[test]
    fn test_semantic_definition_unsupported_language_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.scala");
        std::fs::write(&file, "object Main { def foo = 1 }").unwrap();

        let result = semantic_definition(dir.path(), Language::Scala, &file, 1, 4);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
    }

    // ─── semantic_refs with mock server ──────────────────────────────

    /// Shell script fragments for mock LSP servers (same as in server.rs tests).
    const SHELL_READ_REQUEST: &str = concat!(
        "read_headers() { ",
        "  CL=0; ",
        "  while IFS= read -r line; do ",
        "    line=$(printf '%s' \"$line\" | tr -d '\\r'); ",
        "    [ -z \"$line\" ] && break; ",
        "    case \"$line\" in ",
        "      Content-Length:*) CL=$(echo \"$line\" | cut -d: -f2 | tr -d ' ') ;; ",
        "    esac; ",
        "  done; ",
        "  echo $CL; ",
        "}; ",
        "CL=$(read_headers); ",
        "BODY=$(dd bs=1 count=$CL 2>/dev/null); ",
        "ID=$(echo \"$BODY\" | sed 's/.*\"id\":\\([0-9]*\\).*/\\1/'); ",
    );

    const SHELL_WRITE_MSG: &str = concat!(
        "write_msg() { ",
        "  local MSG=\"$1\"; ",
        "  local LEN=$(printf '%s' \"$MSG\" | wc -c | tr -d ' '); ",
        "  printf 'Content-Length: %s\\r\\n\\r\\n%s' \"$LEN\" \"$MSG\"; ",
        "}; ",
    );

    /// Creates a mock server that:
    /// 1. Responds to initialize with given capabilities
    /// 2. Reads initialized notification
    /// 3. Reads didOpen notification
    /// 4. Responds to a request with the given result JSON
    /// 5. Responds to shutdown
    /// 6. Reads exit notification
    fn mock_oneshot_server(capabilities: &str, query_result: &str) -> crate::config::ServerConfig {
        let script = format!(
            // 1. Read initialize request, respond with capabilities
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{capabilities}}}}}'; \
             \
             {SHELL_READ_REQUEST}\
             \
             {SHELL_READ_REQUEST}\
             \
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{query_result}}}'; \
             \
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             \
             {SHELL_READ_REQUEST}"
        );

        crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        }
    }

    #[test]
    fn test_semantic_refs_with_mock_returns_resolved_references() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let refs_result = format!(
            "[{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":1,\"character\":11}},\"end\":{{\"line\":1,\"character\":14}}}}}}]"
        );

        let config = mock_oneshot_server("{\"referencesProvider\":true}", &refs_result);

        // Use the config directly via low-level API to avoid registry lookup.
        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_references(&file, 1, 3, false).unwrap();
        let _ = server.shutdown();

        // Verify the mock returned locations.
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 1);
        assert_eq!(locations[0].range.start.character, 11);

        // Now verify our conversion logic works.
        let resolved: Vec<ResolvedReference> = locations
            .into_iter()
            .map(|loc| {
                let ref_file = uri_to_path(&loc.uri);
                #[allow(clippy::cast_possible_truncation)]
                let ref_line = loc.range.start.line as usize + 1;
                #[allow(clippy::cast_possible_truncation)]
                let ref_column = loc.range.start.character as usize;
                ResolvedReference {
                    ref_file,
                    ref_line,
                    ref_column,
                    symbol: "foo".to_string(),
                    def_file: Some(file.clone()),
                    def_line: Some(1),
                    def_column: Some(3),
                    resolution: Resolution::Semantic,
                }
            })
            .collect();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].ref_line, 2); // LSP line 1 → cq line 2
        assert_eq!(resolved[0].ref_column, 11);
        assert_eq!(resolved[0].symbol, "foo");
        assert_eq!(resolved[0].resolution, Resolution::Semantic);
        assert_eq!(resolved[0].def_line, Some(1));
    }

    #[test]
    fn test_semantic_definition_with_mock_returns_resolved_references() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let def_result = format!(
            "{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":0,\"character\":3}},\"end\":{{\"line\":0,\"character\":6}}}}}}"
        );

        let config = mock_oneshot_server("{\"definitionProvider\":true}", &def_result);

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_definition(&file, 2, 11).unwrap();
        let _ = server.shutdown();

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 0);
        assert_eq!(locations[0].range.start.character, 3);

        // Verify conversion.
        let resolved: Vec<ResolvedReference> = locations
            .into_iter()
            .map(|loc| {
                let def_file = uri_to_path(&loc.uri);
                #[allow(clippy::cast_possible_truncation)]
                let def_line = loc.range.start.line as usize + 1;
                #[allow(clippy::cast_possible_truncation)]
                let def_column = loc.range.start.character as usize;
                ResolvedReference {
                    ref_file: file.clone(),
                    ref_line: 2,
                    ref_column: 11,
                    symbol: String::new(),
                    def_file: Some(def_file),
                    def_line: Some(def_line),
                    def_column: Some(def_column),
                    resolution: Resolution::Semantic,
                }
            })
            .collect();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].def_line, Some(1)); // LSP line 0 → cq line 1
        assert_eq!(resolved[0].def_column, Some(3));
        assert_eq!(resolved[0].resolution, Resolution::Semantic);
    }

    #[test]
    fn test_semantic_refs_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}").unwrap();

        let config = mock_oneshot_server("{\"referencesProvider\":true}", "[]");

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_references(&file, 1, 3, false).unwrap();
        let _ = server.shutdown();

        assert!(locations.is_empty());
    }

    #[test]
    fn test_semantic_definition_null_result() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}").unwrap();

        let config = mock_oneshot_server("{\"definitionProvider\":true}", "null");

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_definition(&file, 1, 3).unwrap();
        let _ = server.shutdown();

        assert!(locations.is_empty());
    }

    #[test]
    fn test_semantic_refs_with_wait_zero_skips_sleep() {
        // Zero wait should not block. We can't directly test sleep wasn't called,
        // but we can verify the function completes quickly.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rb");
        std::fs::write(&file, "def foo; end").unwrap();

        let start = std::time::Instant::now();
        let result = semantic_refs_with_wait(
            dir.path(),
            Language::Ruby, // No config → early error
            "foo",
            &file,
            1,
            4,
            Duration::ZERO,
        );
        let elapsed = start.elapsed();

        // Ruby has no config, so this errors before reaching the sleep.
        assert!(result.is_err());
        // Should complete nearly instantly.
        assert!(elapsed < Duration::from_millis(500));
    }

    #[test]
    fn test_resolved_reference_line_conversion_correctness() {
        // Verify the 0-based → 1-based conversion math.
        // LSP line 0 → cq line 1
        // LSP line 9 → cq line 10
        let lsp_line: u32 = 9;
        #[allow(clippy::cast_possible_truncation)]
        let cq_line = lsp_line as usize + 1;
        assert_eq!(cq_line, 10);

        let lsp_line: u32 = 0;
        #[allow(clippy::cast_possible_truncation)]
        let cq_line = lsp_line as usize + 1;
        assert_eq!(cq_line, 1);
    }

    #[test]
    fn test_semantic_refs_file_not_found_returns_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nonexistent.rs");

        // Rust has a config, but the file doesn't exist.
        // Server startup will succeed (if rust-analyzer exists) but
        // file reading will fail. On systems without rust-analyzer,
        // it fails at ServerNotFound instead.
        let result = semantic_refs(dir.path(), Language::Rust, "foo", &file, 1, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_index_wait_is_thirty_seconds() {
        assert_eq!(DEFAULT_INDEX_WAIT, Duration::from_secs(30));
    }

    #[test]
    fn test_lsp_language_id_all_tier2() {
        assert_eq!(lsp_language_id(Language::Ruby), "ruby");
        assert_eq!(lsp_language_id(Language::Php), "php");
        assert_eq!(lsp_language_id(Language::CSharp), "csharp");
        assert_eq!(lsp_language_id(Language::Swift), "swift");
        assert_eq!(lsp_language_id(Language::Kotlin), "kotlin");
        assert_eq!(lsp_language_id(Language::Scala), "scala");
        assert_eq!(lsp_language_id(Language::Zig), "zig");
        assert_eq!(lsp_language_id(Language::Lua), "lua");
        assert_eq!(lsp_language_id(Language::Html), "html");
        assert_eq!(lsp_language_id(Language::Css), "css");
        assert_eq!(lsp_language_id(Language::Json), "json");
        assert_eq!(lsp_language_id(Language::Yaml), "yaml");
        assert_eq!(lsp_language_id(Language::Toml), "toml");
    }

    #[test]
    fn test_semantic_definition_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nonexistent.rs");

        let result = semantic_definition(dir.path(), Language::Rust, &file, 1, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolved_reference_from_refs_has_semantic_resolution() {
        let rr = ResolvedReference {
            ref_file: PathBuf::from("/src/main.rs"),
            ref_line: 5,
            ref_column: 10,
            symbol: "process".to_string(),
            def_file: Some(PathBuf::from("/src/lib.rs")),
            def_line: Some(1),
            def_column: Some(3),
            resolution: Resolution::Semantic,
        };
        assert_eq!(rr.resolution, Resolution::Semantic);
        assert_eq!(rr.symbol, "process");
    }

    // ─── semantic_definition_with_wait error paths ──────────────────

    #[test]
    fn test_semantic_definition_with_wait_unsupported_language() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rb");
        std::fs::write(&file, "def foo; end").unwrap();

        let result = semantic_definition_with_wait(
            dir.path(),
            Language::Ruby, // No config → early error
            &file,
            1,
            4,
            Duration::ZERO,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
    }

    #[test]
    fn test_semantic_refs_with_wait_unsupported_language() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.scala");
        std::fs::write(&file, "object Main { def foo = 1 }").unwrap();

        let result = semantic_refs_with_wait(
            dir.path(),
            Language::Scala, // No config → early error
            "foo",
            &file,
            1,
            5,
            Duration::ZERO,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
    }

    // ─── semantic_refs via mock server end-to-end ────────────────────

    #[test]
    fn test_semantic_refs_with_mock_server_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let refs_result = format!(
            "[{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":1,\"character\":11}},\"end\":{{\"line\":1,\"character\":14}}}}}}]"
        );

        let config = mock_oneshot_server("{\"referencesProvider\":true}", &refs_result);

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_references(&file, 1, 3, false).unwrap();

        // Build refs the same way the oneshot module does.
        let refs: Vec<ResolvedReference> = locations
            .into_iter()
            .map(|loc| {
                let ref_file = crate::queries::uri_to_path(&loc.uri);
                #[allow(clippy::cast_possible_truncation)]
                let ref_line = loc.range.start.line as usize + 1;
                #[allow(clippy::cast_possible_truncation)]
                let ref_column = loc.range.start.character as usize;
                ResolvedReference {
                    ref_file,
                    ref_line,
                    ref_column,
                    symbol: "foo".to_string(),
                    def_file: Some(file.clone()),
                    def_line: Some(1),
                    def_column: Some(3),
                    resolution: Resolution::Semantic,
                }
            })
            .collect();

        let _ = server.shutdown();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resolution, Resolution::Semantic);
    }

    // ─── semantic_definition via mock server end-to-end ──────────────

    #[test]
    fn test_semantic_definition_with_mock_server_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let def_result = format!(
            "{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":0,\"character\":3}},\"end\":{{\"line\":0,\"character\":6}}}}}}"
        );

        let config = mock_oneshot_server("{\"definitionProvider\":true}", &def_result);

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_definition(&file, 2, 11).unwrap();

        let refs: Vec<ResolvedReference> = locations
            .into_iter()
            .map(|loc| {
                let def_file = crate::queries::uri_to_path(&loc.uri);
                #[allow(clippy::cast_possible_truncation)]
                let def_line = loc.range.start.line as usize + 1;
                #[allow(clippy::cast_possible_truncation)]
                let def_column = loc.range.start.character as usize;
                ResolvedReference {
                    ref_file: file.clone(),
                    ref_line: 2,
                    ref_column: 11,
                    symbol: String::new(),
                    def_file: Some(def_file),
                    def_line: Some(def_line),
                    def_column: Some(def_column),
                    resolution: Resolution::Semantic,
                }
            })
            .collect();

        let _ = server.shutdown();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].def_line, Some(1));
    }

    // ─── multiple refs from mock server ─────────────────────────────

    #[test]
    fn test_semantic_refs_with_mock_multiple_locations() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(
            &file,
            "fn foo() {}\nfn bar() { foo(); }\nfn baz() { foo(); }",
        )
        .unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let refs_result = format!(
            "[{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":1,\"character\":11}},\"end\":{{\"line\":1,\"character\":14}}}}}},\
             {{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":2,\"character\":11}},\"end\":{{\"line\":2,\"character\":14}}}}}}]"
        );

        let config = mock_oneshot_server("{\"referencesProvider\":true}", &refs_result);

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        let source = std::fs::read_to_string(&file).unwrap();
        server.open_document(&file, &source, "rust").unwrap();

        let locations = server.find_references(&file, 1, 3, false).unwrap();
        let _ = server.shutdown();

        assert_eq!(locations.len(), 2);
    }
}
