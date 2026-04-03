//! Four-step resolution cascade for cq queries.
//!
//! Implements the automatic resolution strategy: daemon first, then oneshot
//! LSP, then stack graph resolution, with graceful fallback at each level.
//! The cascade returns the same `ResolutionResult` type regardless of which
//! tier produced the result — downstream commands read the metadata to know
//! the precision level.

use std::path::Path;

use codequery_core::{Language, Resolution};
use codequery_index::FileSymbols;
use codequery_resolve::{ResolutionResult, ResolvedReference, StackGraphResolver};

use crate::client::DaemonClient;
use crate::daemon_file;
use crate::oneshot;
use crate::queries::uri_to_path;

/// Controls how cq resolves cross-references via language servers.
///
/// Precedence: `--no-semantic` (force off) > `--semantic` (force oneshot) >
/// `CQ_SEMANTIC` env var > default (off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMode {
    /// No LSP resolution. Tiers 1-2 only (tree-sitter + stack graphs).
    Off,
    /// Cold-start a language server per query, then shut it down.
    Oneshot,
    /// Use a persistent daemon. Auto-starts one if not already running.
    Daemon,
}

/// Resolves references using a four-step cascade of increasing cost.
///
/// The cascade tries the following strategies in order, falling through on
/// failure:
///
/// 1. **Daemon** -- If a cq daemon is running (or `SemanticMode::Daemon`
///    requests auto-start), query it for references via a warm language
///    server. Fastest path (sub-50ms).
/// 2. **Oneshot LSP** -- If `SemanticMode::Oneshot` is active and no daemon
///    is running, start a language server, query, and shut it down. Correct
///    but slow (2-5s).
/// 3. **Stack graph** -- Use `StackGraphResolver` which provides `Resolved`
///    or `Syntactic` precision depending on language support.
/// 4. **Fallback** -- If steps 1-2 error, fall through to step 3.
///
/// The return type is always `ResolutionResult`, so callers never need to
/// know which tier was used -- they check `Resolution` metadata on each
/// reference.
#[must_use]
#[allow(clippy::too_many_arguments)]
// All parameters are essential to the cascade logic; splitting would obscure the API.
pub fn resolve_with_cascade(
    project_root: &Path,
    language: Language,
    symbol_name: &str,
    symbol_file: &Path,
    symbol_line: usize,
    symbol_column: usize,
    scan_results: &[FileSymbols],
    semantic_mode: SemanticMode,
) -> ResolutionResult {
    // Step 1: Try the daemon if it's running and mode is not Off.
    if semantic_mode != SemanticMode::Off {
        let daemon_running = daemon_file::is_daemon_running(project_root);

        // In Daemon mode, auto-start the daemon if not already running.
        let daemon_running = if !daemon_running && semantic_mode == SemanticMode::Daemon {
            try_auto_start_daemon(project_root)
        } else {
            daemon_running
        };

        if daemon_running {
            if let Ok(result) = try_daemon_refs(
                project_root,
                language,
                symbol_name,
                symbol_file,
                symbol_line,
                symbol_column,
            ) {
                return result;
            }
            // Daemon connection or query failed; fall through.
        }
    }

    // Step 2: Try oneshot LSP if mode is Oneshot (not Daemon — Daemon uses the daemon path).
    if semantic_mode == SemanticMode::Oneshot {
        match try_oneshot_refs(
            project_root,
            language,
            symbol_name,
            symbol_file,
            symbol_line,
            symbol_column,
        ) {
            Ok(result) if !result.references.is_empty() => {
                return result;
            }
            _ => {
                // Oneshot returned empty results or failed — fall through.
            }
        }
        // Oneshot failed; fall through to stack graph.
    }

    // Step 2b: Daemon mode fallback — if daemon start failed or query failed,
    // fall back to oneshot as a last resort before stack graphs.
    if semantic_mode == SemanticMode::Daemon {
        match try_oneshot_refs(
            project_root,
            language,
            symbol_name,
            symbol_file,
            symbol_line,
            symbol_column,
        ) {
            Ok(result) if !result.references.is_empty() => {
                return result;
            }
            _ => {}
        }
    }

    // Step 3: Stack graph resolution (always available).
    let mut resolver = StackGraphResolver::new();
    resolver.resolve_refs(scan_results, symbol_name)
}

/// Attempt to auto-start the cq daemon for a project.
///
/// Looks for the `cq` binary on PATH (or via `CQ_BIN` env var), spawns
/// `cq daemon start --project <root>` as a detached process, then polls
/// `is_daemon_running()` for up to 3 seconds. Returns `true` if the daemon
/// became available.
fn try_auto_start_daemon(project_root: &Path) -> bool {
    let cq_bin = std::env::var("CQ_BIN").unwrap_or_else(|_| "cq".to_string());

    let project_str = project_root.to_string_lossy();
    let spawn_result = std::process::Command::new(&cq_bin)
        .args(["daemon", "start", "--project", &project_str])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    if spawn_result.is_err() {
        return false;
    }

    // Poll for daemon availability with 100ms intervals, up to 3 seconds.
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if daemon_file::is_daemon_running(project_root) {
            return true;
        }
    }

    false
}

/// Attempts to resolve references via the daemon.
fn try_daemon_refs(
    project_root: &Path,
    language: Language,
    symbol_name: &str,
    symbol_file: &Path,
    symbol_line: usize,
    symbol_column: usize,
) -> crate::error::Result<ResolutionResult> {
    let mut client = DaemonClient::connect(project_root)?;
    let locations = client.query_refs(
        project_root,
        language,
        symbol_file,
        symbol_line,
        symbol_column,
    )?;

    let references: Vec<ResolvedReference> = locations
        .into_iter()
        .map(|loc| {
            let ref_file = uri_to_path(&loc.uri);
            #[allow(clippy::cast_possible_truncation)]
            // LSP line numbers (u32) fit comfortably in usize.
            let ref_line = loc.range.start.line as usize + 1; // LSP 0-based -> cq 1-based
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

    Ok(ResolutionResult {
        references,
        warnings: Vec::new(),
    })
}

/// Attempts to resolve references via oneshot (start-query-stop) LSP.
fn try_oneshot_refs(
    project_root: &Path,
    language: Language,
    symbol_name: &str,
    symbol_file: &Path,
    symbol_line: usize,
    symbol_column: usize,
) -> crate::error::Result<ResolutionResult> {
    let references = oneshot::semantic_refs(
        project_root,
        language,
        symbol_name,
        symbol_file,
        symbol_line,
        symbol_column,
    )?;

    Ok(ResolutionResult {
        references,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_parse::Parser;
    use std::path::PathBuf;

    /// Check if grammar and TSG rules are available for a language.
    fn grammar_and_tsg_available(lang: Language) -> bool {
        let source = match lang {
            Language::Python => "x = 1\n",
            Language::Rust => "fn main() {}\n",
            _ => "x = 1\n",
        };
        let Ok(mut parser) = Parser::for_language(lang) else {
            return false;
        };
        let Ok(tree) = parser.parse(source.as_bytes()) else {
            return false;
        };
        let file = PathBuf::from("test.tmp");
        let files = vec![(file, source.to_string(), tree)];
        codequery_resolve::build_graph(&files, lang).is_ok()
    }

    /// Create a `FileSymbols` from source text, path, and language.
    fn make_file_symbols(path: &str, source: &str, lang: Language) -> FileSymbols {
        let mut parser = Parser::for_language(lang).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let file = PathBuf::from(path);
        let symbols = codequery_parse::extract_symbols(source, &tree, &file, lang);
        FileSymbols {
            file,
            symbols,
            source: source.to_string(),
            tree,
        }
    }

    // ─── cascade falls through to stack graph ─────────────────────────

    #[test]
    fn test_cascade_no_daemon_no_semantic_uses_stack_graph() {
        if !grammar_and_tsg_available(Language::Python) {
            eprintln!("skipping: python grammar or TSG rules not available");
            return;
        }
        let source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Python,
            "greet",
            Path::new("app.py"),
            1,
            4,
            &[fs],
            SemanticMode::Off, // no semantic requested
        );

        // Should get results from stack graph resolver (Resolved or Syntactic).
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert!(
                r.resolution == Resolution::Resolved || r.resolution == Resolution::Syntactic,
                "expected Resolved or Syntactic, got {:?}",
                r.resolution
            );
        }
    }

    #[test]
    fn test_cascade_semantic_requested_but_no_server_falls_to_stack_graph() {
        let source = "def greet(name):\n    pass\n\ngreet('world')\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        // Semantic is requested but no server is available (Ruby has no config).
        // This forces the cascade through step 2 (which will fail) and into step 3.
        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Ruby,
            "greet",
            Path::new("app.rb"),
            1,
            4,
            &[fs],
            SemanticMode::Oneshot, // semantic requested
        );

        // The Python scan results don't match a Ruby query, so we may get
        // empty results from the stack graph. The key is: no panic, no error.
        let _ = result;
    }

    #[test]
    fn test_cascade_empty_scan_results_returns_empty() {
        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Rust,
            "foo",
            Path::new("main.rs"),
            1,
            0,
            &[],
            SemanticMode::Off,
        );

        assert!(result.references.is_empty());
    }

    // ─── cascade with C++ (syntactic fallback) ────────────────────────

    #[test]
    fn test_cascade_cpp_uses_stack_graph_resolution() {
        let source = "void greet() {}\nint main() { greet(); return 0; }\n";
        let fs = make_file_symbols("main.cpp", source, Language::Cpp);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Cpp,
            "greet",
            Path::new("main.cpp"),
            1,
            5,
            &[fs],
            SemanticMode::Off,
        );

        // C++ now has stack graph rules — references should be resolved.
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
            assert_eq!(r.resolution, Resolution::Resolved);
        }
    }

    // ─── try_daemon_refs when no daemon running ───────────────────────

    #[test]
    fn test_try_daemon_refs_fails_when_no_daemon() {
        let result = try_daemon_refs(
            Path::new("/project"),
            Language::Rust,
            "foo",
            Path::new("/project/main.rs"),
            1,
            0,
        );
        assert!(result.is_err());
    }

    // ─── try_oneshot_refs with unsupported language ───────────────────

    #[test]
    fn test_try_oneshot_refs_unsupported_language_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rb");
        std::fs::write(&file, "def foo; end").unwrap();

        let result = try_oneshot_refs(dir.path(), Language::Ruby, "foo", &file, 1, 4);
        assert!(result.is_err());
    }

    // ─── cascade with Rust fixture ────────────────────────────────────

    #[test]
    fn test_cascade_rust_same_file() {
        let source = "fn greet() -> String {\n    String::from(\"hello\")\n}\nfn main() {\n    greet();\n}\n";
        let fs = make_file_symbols("main.rs", source, Language::Rust);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Rust,
            "greet",
            Path::new("main.rs"),
            1,
            3,
            &[fs],
            SemanticMode::Off,
        );

        for r in &result.references {
            assert_eq!(r.symbol, "greet");
        }
    }

    // ─── daemon running check is respected ────────────────────────────

    #[test]
    fn test_cascade_skips_daemon_when_not_running() {
        // With no daemon running, the cascade should skip step 1 entirely
        // and go to step 2 (if semantic) or step 3 (if not).
        assert!(!daemon_file::is_daemon_running(Path::new("/tmp/project")));

        let source = "x = 1\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Python,
            "x",
            Path::new("app.py"),
            1,
            0,
            &[fs],
            SemanticMode::Off,
        );

        // Should complete without error regardless.
        let _ = result;
    }

    // ─── ResolutionResult metadata ────────────────────────────────────

    #[test]
    fn test_cascade_result_has_correct_type() {
        let source = "def foo():\n    pass\n\nfoo()\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Python,
            "foo",
            Path::new("app.py"),
            1,
            4,
            &[fs],
            SemanticMode::Off,
        );

        // Result is a ResolutionResult — verify its structure.
        assert!(result.references.iter().all(|r| r.symbol == "foo"));
    }

    // ─── try_oneshot_refs error handling ─────────────────────────────

    #[test]
    fn test_try_oneshot_refs_nonexistent_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nonexistent.py");
        // File does not exist — should fail at read_file_source.
        let result = try_oneshot_refs(dir.path(), Language::Python, "foo", &file, 1, 4);
        assert!(result.is_err());
    }

    // ─── cascade with multiple scan results ─────────────────────────

    #[test]
    fn test_cascade_multiple_files_with_matching_symbol() {
        if !grammar_and_tsg_available(Language::Python) {
            eprintln!("skipping: python grammar or TSG rules not available");
            return;
        }
        let source1 = "def greet():\n    pass\n";
        let source2 = "from app import greet\ngreet()\n";
        let fs1 = make_file_symbols("app.py", source1, Language::Python);
        let fs2 = make_file_symbols("main.py", source2, Language::Python);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Python,
            "greet",
            Path::new("app.py"),
            1,
            4,
            &[fs1, fs2],
            SemanticMode::Off,
        );

        // Stack graph should find references across both files.
        for r in &result.references {
            assert_eq!(r.symbol, "greet");
        }
    }

    // ─── cascade with semantic_requested=true and no server ─────────

    #[test]
    fn test_cascade_semantic_requested_no_daemon_no_server_falls_to_stack_graph() {
        // Semantic is requested, but no daemon is running and the LSP
        // server binary doesn't exist. Should fall through to stack graph.
        let source = "fn foo() {}\nfn bar() { foo(); }\n";
        let fs = make_file_symbols("main.rs", source, Language::Rust);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Rust,
            "foo",
            Path::new("main.rs"),
            1,
            3,
            &[fs],
            SemanticMode::Oneshot, // semantic requested, but no real server available
        );

        // Should succeed (fell through to stack graph), not panic.
        for r in &result.references {
            assert_eq!(r.symbol, "foo");
        }
    }

    // ─── daemon not running path is exercised ───────────────────────

    #[test]
    fn test_cascade_no_daemon_semantic_true_unsupported_lang_falls_to_stack_graph() {
        let source = "x = 1\nprint(x)\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        // Ruby has no LSP config, so oneshot will fail even with semantic=Oneshot.
        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Ruby,
            "x",
            Path::new("app.rb"),
            1,
            0,
            &[fs],
            SemanticMode::Oneshot,
        );

        // Should not panic — falls through to stack graph.
        let _ = result;
    }

    // ─── ResolutionResult warnings field ────────────────────────────

    #[test]
    fn test_cascade_result_warnings_empty_for_stack_graph() {
        if !grammar_and_tsg_available(Language::Python) {
            eprintln!("skipping: python grammar or TSG rules not available");
            return;
        }
        let source = "def foo():\n    pass\n\nfoo()\n";
        let fs = make_file_symbols("app.py", source, Language::Python);

        let result = resolve_with_cascade(
            Path::new("/tmp/project"),
            Language::Python,
            "foo",
            Path::new("app.py"),
            1,
            4,
            &[fs],
            SemanticMode::Off,
        );

        // Stack graph resolution should not produce warnings for valid input.
        assert!(result.warnings.is_empty());
    }
}
