//! Symbols command: list all symbols in the project.

use std::path::Path;

use codequery_core::{detect_project_root_or, Symbol, SymbolKind};
use codequery_index::{scan_project_cached, SymbolIndex};

use crate::args::{ExitCode, OutputMode};
use crate::output::format_symbols;

/// Run the symbols command: list all symbols in the project.
///
/// Scans the entire project in parallel using codequery-index's scanner,
/// builds a `SymbolIndex`, applies optional `--kind` and `--limit` filters,
/// and prints results in the requested output mode.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected or scanning fails.
pub fn run(
    project: Option<&Path>,
    scope: Option<&Path>,
    kind: Option<&str>,
    limit: Option<usize>,
    mode: OutputMode,
    pretty: bool,
    use_cache: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Parallel scan all source files (with optional caching)
    let scan = scan_project_cached(&project_root, scope, use_cache)?;

    // 3. Build index
    let index = SymbolIndex::from_scan(&scan);

    // 4. Collect symbols, applying --kind filter if provided
    let mut symbols: Vec<Symbol> = if let Some(kind_str) = kind {
        let kind = parse_kind(kind_str)?;
        index.find_by_kind(kind).into_iter().cloned().collect()
    } else {
        index.all_symbols().to_vec()
    };

    // 5. Sort by file path, then by line number for deterministic output
    symbols.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    // 6. Apply --limit if provided
    if let Some(limit) = limit {
        symbols.truncate(limit);
    }

    // 7. Format and output
    if symbols.is_empty() && mode != OutputMode::Json {
        Ok(ExitCode::NoResults)
    } else {
        let output = format_symbols(&symbols, mode, pretty);
        if !output.is_empty() {
            println!("{output}");
        }
        if symbols.is_empty() {
            Ok(ExitCode::NoResults)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

/// Parse a `--kind` filter string into a `SymbolKind`.
///
/// Accepts the lowercase display form of each kind (e.g. "function", "struct").
///
/// # Errors
///
/// Returns an error if the string doesn't match any known symbol kind.
fn parse_kind(s: &str) -> anyhow::Result<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" => Ok(SymbolKind::Function),
        "method" => Ok(SymbolKind::Method),
        "struct" => Ok(SymbolKind::Struct),
        "class" => Ok(SymbolKind::Class),
        "trait" => Ok(SymbolKind::Trait),
        "interface" => Ok(SymbolKind::Interface),
        "enum" => Ok(SymbolKind::Enum),
        "type" => Ok(SymbolKind::Type),
        "const" => Ok(SymbolKind::Const),
        "static" => Ok(SymbolKind::Static),
        "module" => Ok(SymbolKind::Module),
        "impl" => Ok(SymbolKind::Impl),
        "test" => Ok(SymbolKind::Test),
        _ => Err(anyhow::anyhow!(
            "unknown symbol kind: {s}. valid kinds: function, method, struct, class, trait, \
             interface, enum, type, const, static, module, impl, test"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    // -----------------------------------------------------------------------
    // parse_kind
    // -----------------------------------------------------------------------

    #[test]
    fn test_symbols_parse_kind_function() {
        assert_eq!(parse_kind("function").unwrap(), SymbolKind::Function);
    }

    #[test]
    fn test_symbols_parse_kind_case_insensitive() {
        assert_eq!(parse_kind("Function").unwrap(), SymbolKind::Function);
        assert_eq!(parse_kind("STRUCT").unwrap(), SymbolKind::Struct);
    }

    #[test]
    fn test_symbols_parse_kind_all_variants() {
        let cases = [
            ("function", SymbolKind::Function),
            ("method", SymbolKind::Method),
            ("struct", SymbolKind::Struct),
            ("class", SymbolKind::Class),
            ("trait", SymbolKind::Trait),
            ("interface", SymbolKind::Interface),
            ("enum", SymbolKind::Enum),
            ("type", SymbolKind::Type),
            ("const", SymbolKind::Const),
            ("static", SymbolKind::Static),
            ("module", SymbolKind::Module),
            ("impl", SymbolKind::Impl),
            ("test", SymbolKind::Test),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_kind(input).unwrap(), expected, "failed for {input}");
        }
    }

    #[test]
    fn test_symbols_parse_kind_unknown_returns_error() {
        assert!(parse_kind("unknown").is_err());
    }

    // -----------------------------------------------------------------------
    // Integration tests against fixture
    // -----------------------------------------------------------------------

    #[test]
    fn test_symbols_returns_all_symbols_across_project() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            None,
            None,
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_symbols_kind_function_filters_to_functions_only() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            None,
            Some("function"),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_symbols_limit_caps_results() {
        let project = fixture_project();
        // The fixture has more than 5 symbols, so limiting to 5 should still succeed
        let result = run(
            Some(&project),
            None,
            None,
            Some(5),
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_symbols_json_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            None,
            None,
            None,
            OutputMode::Json,
            true,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_symbols_raw_mode_returns_success() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            None,
            None,
            None,
            OutputMode::Raw,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_symbols_empty_project_returns_no_results() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let result = run(
            Some(tmp.path()),
            None,
            None,
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    #[test]
    fn test_symbols_unknown_kind_returns_error() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            None,
            Some("unknown_kind"),
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_symbols_with_scope_limits_search() {
        let project = fixture_project();
        let result = run(
            Some(&project),
            Some(Path::new("src/utils")),
            None,
            None,
            OutputMode::Framed,
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
