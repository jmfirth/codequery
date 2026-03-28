//! Tree command: display a hierarchical symbol tree for the entire project.

use std::path::Path;

use codequery_core::detect_project_root_or;
use codequery_index::{scan_project_cached, FileSymbols};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_tree_output;

/// Run the tree command: display a hierarchical symbol tree.
///
/// Scans all source files in the project (optionally scoped by a path argument),
/// groups symbols by file, sorts files alphabetically, and outputs a nested
/// tree showing file > symbol > children structure.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or file scanning fails.
pub fn run(
    path: Option<&Path>,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
    depth: Option<usize>,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Determine effective scope: path argument takes precedence over --in flag
    let effective_scope = path.or(scope);

    // 3. Scan all files in parallel (with optional caching)
    let file_symbols = scan_project_cached(&project_root, effective_scope, use_cache)?;

    // 4. Apply depth limiting if requested
    let file_symbols = if let Some(max_depth) = depth {
        limit_depth(file_symbols, max_depth)
    } else {
        file_symbols
    };

    // 5. Format and output
    let all_empty = file_symbols.iter().all(|fs| fs.symbols.is_empty());
    let output = format_tree_output(&file_symbols, effective_scope, mode, pretty);

    if !output.is_empty() {
        println!("{output}");
    }

    if file_symbols.is_empty() || all_empty {
        Ok(ExitCode::NoResults)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Limit symbol nesting depth. Depth 1 means top-level only (no children).
fn limit_depth(file_symbols: Vec<FileSymbols>, max_depth: usize) -> Vec<FileSymbols> {
    file_symbols
        .into_iter()
        .map(|mut fs| {
            fs.symbols = truncate_children(fs.symbols, 1, max_depth);
            fs
        })
        .collect()
}

/// Recursively truncate children beyond the given depth.
fn truncate_children(
    symbols: Vec<codequery_core::Symbol>,
    current_depth: usize,
    max_depth: usize,
) -> Vec<codequery_core::Symbol> {
    symbols
        .into_iter()
        .map(|mut s| {
            if current_depth >= max_depth {
                s.children = Vec::new();
            } else {
                s.children = truncate_children(s.children, current_depth + 1, max_depth);
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{Symbol, SymbolKind, Visibility};
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Helper: create a symbol for testing.
    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        children: Vec<Symbol>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column: 0,
            end_line: line + 5,
            visibility: Visibility::Public,
            children,
            doc: None,
            body: None,
            signature: None,
        }
    }

    /// Create a dummy tree for testing (parse empty source with Rust parser).
    fn dummy_tree() -> tree_sitter::Tree {
        let mut parser = codequery_parse::Parser::for_language(codequery_core::Language::Rust)
            .expect("rust parser");
        parser.parse(b"").expect("parse empty source")
    }

    // Test 1: Tree shows all files with their symbols
    #[test]
    fn test_tree_fixture_shows_all_files() {
        let project = fixture_project();
        let result = run(
            None,
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 2: Tree with path scopes to subdirectory
    #[test]
    fn test_tree_path_scopes_to_subdirectory() {
        let project = fixture_project();
        let result = run(
            Some(Path::new("src/utils")),
            Some(&project),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 3: --depth 1 limits to top-level only
    #[test]
    fn test_tree_depth_limit_truncates_children() {
        let child = make_symbol("method", SymbolKind::Method, "lib.rs", 5, vec![]);
        let parent = make_symbol("MyImpl", SymbolKind::Impl, "lib.rs", 1, vec![child]);
        let fs = FileSymbols {
            file: PathBuf::from("lib.rs"),
            symbols: vec![parent],
            source: String::new(),
            tree: dummy_tree(),
        };
        let limited = limit_depth(vec![fs], 1);
        assert!(limited[0].symbols[0].children.is_empty());
    }

    // Test 4: --depth 2 preserves one level of children
    #[test]
    fn test_tree_depth_2_preserves_one_level_of_children() {
        let grandchild = make_symbol("inner", SymbolKind::Function, "lib.rs", 10, vec![]);
        let child = make_symbol("method", SymbolKind::Method, "lib.rs", 5, vec![grandchild]);
        let parent = make_symbol("MyImpl", SymbolKind::Impl, "lib.rs", 1, vec![child]);
        let fs = FileSymbols {
            file: PathBuf::from("lib.rs"),
            symbols: vec![parent],
            source: String::new(),
            tree: dummy_tree(),
        };
        let limited = limit_depth(vec![fs], 2);
        assert_eq!(limited[0].symbols[0].children.len(), 1);
        assert!(limited[0].symbols[0].children[0].children.is_empty());
    }

    // Test 5: JSON mode works
    #[test]
    fn test_tree_json_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            None,
            Some(&project),
            None,
            OutputMode::Json,
            true,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 6: Raw mode works
    #[test]
    fn test_tree_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            None,
            Some(&project),
            None,
            OutputMode::Raw,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 7: Empty project returns NoResults
    #[test]
    fn test_tree_empty_project_returns_no_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let result = run(
            None,
            Some(tmp.path()),
            None,
            OutputMode::Framed,
            false,
            None,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 8: Depth 0 removes all symbols' children
    #[test]
    fn test_tree_depth_zero_removes_all_children() {
        let child = make_symbol("method", SymbolKind::Method, "lib.rs", 5, vec![]);
        let parent = make_symbol("MyImpl", SymbolKind::Impl, "lib.rs", 1, vec![child]);
        let fs = FileSymbols {
            file: PathBuf::from("lib.rs"),
            symbols: vec![parent.clone()],
            source: String::new(),
            tree: dummy_tree(),
        };
        // depth 0 should still show top-level symbols, just no children
        let limited = limit_depth(vec![fs], 0);
        // With depth 0: current_depth (1) >= max_depth (0), so children are removed
        assert!(limited[0].symbols[0].children.is_empty());
    }
}
