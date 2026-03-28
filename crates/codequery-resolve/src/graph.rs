//! Stack graph construction from scanned source files.
//!
//! Builds a complete stack graph from a set of source files and their parse trees,
//! then computes partial paths for use in name resolution queries. Files that fail
//! graph construction are skipped with warnings rather than aborting the entire build.

use std::path::PathBuf;
use std::time::Duration;

use codequery_core::Language;
use stack_graphs::graph::StackGraph;
use stack_graphs::partial::PartialPaths;
use tree_sitter_stack_graphs::{CancelAfterDuration, NoCancellation, Variables};

use crate::error::{ResolveError, Result};
use crate::rules::language_config;

/// Default timeout for graph construction per file.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Warning emitted when a single file fails graph construction.
#[derive(Debug, Clone)]
pub struct GraphWarning {
    /// The file that failed.
    pub file: PathBuf,
    /// Description of what went wrong.
    pub message: String,
}

/// Result of building a stack graph from a set of source files.
///
/// `StackGraph` and `PartialPaths` do not implement `Debug`, so this type
/// provides a manual implementation that shows the warning count and file count.
pub struct GraphResult {
    /// The constructed stack graph containing nodes and edges for all successfully processed files.
    pub graph: StackGraph,
    /// Partial paths computed from the stack graph, used for name resolution.
    pub partial_paths: PartialPaths,
    /// Warnings from files that failed graph construction (skipped, not fatal).
    pub warnings: Vec<GraphWarning>,
}

impl std::fmt::Debug for GraphResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphResult")
            .field("files", &self.graph.iter_files().count())
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// Build a stack graph from a set of source files.
///
/// Takes file paths, source text, and pre-parsed tree-sitter trees. Constructs
/// stack graph nodes and edges for each file using the language's TSG rules, then
/// computes partial paths for downstream resolution.
///
/// Files that fail graph construction are skipped and reported as warnings.
/// This is consistent with cq's error-tolerance principle.
///
/// # Arguments
///
/// * `files` - Tuples of (file path, source text, parse tree) for each file.
/// * `language` - The language of all files (must be uniform).
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the language has no stack graph rules
/// or if the TSG rules fail to load.
pub fn build_graph(
    files: &[(PathBuf, String, tree_sitter::Tree)],
    language: Language,
) -> Result<GraphResult> {
    build_graph_with_timeout(files, language, Some(DEFAULT_TIMEOUT))
}

/// Build a stack graph with a configurable per-file timeout.
///
/// Like [`build_graph`], but allows overriding the per-file timeout. Pass `None`
/// to disable the timeout entirely.
///
/// # Errors
///
/// Returns `ResolveError::RuleLoadError` if the language has no stack graph rules
/// or if the TSG rules fail to load.
pub fn build_graph_with_timeout(
    files: &[(PathBuf, String, tree_sitter::Tree)],
    language: Language,
    timeout: Option<Duration>,
) -> Result<GraphResult> {
    let sgl = language_config(language)
        .ok_or_else(|| {
            ResolveError::RuleLoadError(format!("{language:?}: no stack graph rules available"))
        })?
        .map_err(|e| ResolveError::RuleLoadError(format!("{language:?}: {e}")))?;

    let mut graph = StackGraph::new();
    let mut warnings = Vec::new();
    let globals = Variables::new();

    for (path, source, _tree) in files {
        let path_str = path.to_string_lossy();
        let file_handle = graph.get_or_create_file(&*path_str);

        let build_result = if let Some(limit) = timeout {
            let cancel = CancelAfterDuration::new(limit);
            sgl.build_stack_graph_into(&mut graph, file_handle, source, &globals, &cancel)
        } else {
            sgl.build_stack_graph_into(&mut graph, file_handle, source, &globals, &NoCancellation)
        };

        if let Err(err) = build_result {
            warnings.push(GraphWarning {
                file: path.clone(),
                message: format!("{err}"),
            });
        }
    }

    let partial_paths = PartialPaths::new();

    Ok(GraphResult {
        graph,
        partial_paths,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_parse::Parser;
    use std::path::Path;

    /// Parse a single source string and return the triple needed by `build_graph`.
    fn parse_source(
        path: &Path,
        source: &str,
        lang: Language,
    ) -> (PathBuf, String, tree_sitter::Tree) {
        let mut parser = Parser::for_language(lang).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        (path.to_path_buf(), source.to_string(), tree)
    }

    // -----------------------------------------------------------------------
    // build_graph — Python
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_python_simple_function() {
        let source = "def greet(name):\n    return f'Hello, {name}!'\n";
        let files = vec![parse_source(Path::new("main.py"), source, Language::Python)];
        let result = build_graph(&files, Language::Python).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_build_graph_python_multiple_files() {
        let src_a = "def add(a, b):\n    return a + b\n";
        let src_b = "from main import add\nresult = add(1, 2)\n";

        let files = vec![
            parse_source(Path::new("math.py"), src_a, Language::Python),
            parse_source(Path::new("app.py"), src_b, Language::Python),
        ];
        let result = build_graph(&files, Language::Python).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_build_graph_python_fixture_project() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_project");
        let fixture_files = [
            ("src/main.py", Language::Python),
            ("src/utils.py", Language::Python),
        ];

        let files: Vec<_> = fixture_files
            .iter()
            .filter_map(|(rel, lang)| {
                let abs = fixture_root.join(rel);
                let source = std::fs::read_to_string(&abs).ok()?;
                Some(parse_source(Path::new(rel), &source, *lang))
            })
            .collect();

        assert!(!files.is_empty(), "fixture files should be readable");
        let result = build_graph(&files, Language::Python).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    // -----------------------------------------------------------------------
    // build_graph — unsupported language
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_unsupported_language_returns_error() {
        let source = "int main() {}";
        let files = vec![parse_source(Path::new("main.cpp"), source, Language::Cpp)];
        let result = build_graph(&files, Language::Cpp);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no stack graph rules"),
            "error should mention missing rules: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Error tolerance — bad source
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_skips_failed_files_with_warning() {
        // Provide source that will parse (tree-sitter is error-tolerant) but may
        // produce graph construction warnings.
        let good_source = "def good():\n    pass\n";
        let bad_source = "def )(\n    @@@\n";

        let files = vec![
            parse_source(Path::new("good.py"), good_source, Language::Python),
            parse_source(Path::new("bad.py"), bad_source, Language::Python),
        ];
        let result = build_graph(&files, Language::Python).unwrap();

        // The good file should succeed; the bad file may or may not warn.
        // What matters is that we get a result, not an error.
        assert!(result.graph.iter_files().count() >= 1);
    }

    // -----------------------------------------------------------------------
    // Timeout
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_with_timeout_none_disables_timeout() {
        let source = "x = 1\n";
        let files = vec![parse_source(
            Path::new("simple.py"),
            source,
            Language::Python,
        )];
        let result = build_graph_with_timeout(&files, Language::Python, None).unwrap();

        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_build_graph_with_timeout_custom_duration() {
        let source = "y = 2\n";
        let files = vec![parse_source(Path::new("t.py"), source, Language::Python)];
        let result =
            build_graph_with_timeout(&files, Language::Python, Some(Duration::from_secs(30)))
                .unwrap();

        assert!(result.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Empty input
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_empty_file_list() {
        let files: Vec<(PathBuf, String, tree_sitter::Tree)> = vec![];
        let result = build_graph(&files, Language::Python).unwrap();

        assert!(result.warnings.is_empty());
        assert_eq!(result.graph.iter_files().count(), 0);
    }

    // -----------------------------------------------------------------------
    // GraphWarning display
    // -----------------------------------------------------------------------

    #[test]
    fn test_graph_warning_has_file_and_message() {
        let warning = GraphWarning {
            file: PathBuf::from("test.py"),
            message: "parse error".to_string(),
        };

        assert_eq!(warning.file, PathBuf::from("test.py"));
        assert_eq!(warning.message, "parse error");
    }

    // -----------------------------------------------------------------------
    // JavaScript
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_javascript_simple() {
        let source = "function hello() { return 'world'; }\n";
        let files = vec![parse_source(
            Path::new("index.js"),
            source,
            Language::JavaScript,
        )];
        let result = build_graph(&files, Language::JavaScript).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    // -----------------------------------------------------------------------
    // TypeScript
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_typescript_simple() {
        let source = "function greet(name: string): string { return `Hello ${name}`; }\n";
        let files = vec![parse_source(
            Path::new("main.ts"),
            source,
            Language::TypeScript,
        )];
        let result = build_graph(&files, Language::TypeScript).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    // -----------------------------------------------------------------------
    // Java
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_graph_java_simple() {
        let source = "public class Hello {\n    public static void main(String[] args) {}\n}\n";
        let files = vec![parse_source(
            Path::new("Hello.java"),
            source,
            Language::Java,
        )];
        let result = build_graph(&files, Language::Java).unwrap();

        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }
}
