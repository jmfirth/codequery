//! Deps command: analyze internal dependencies of a function/method.

use std::collections::HashSet;
use std::path::Path;

use codequery_core::{
    detect_project_root_or, discover_files, language_for_file, ReferenceKind, Symbol, SymbolKind,
};
use codequery_index::{extract_references, scan_project, SymbolIndex};
use codequery_parse::{extract_symbols, Parser};

use crate::args::{ExitCode, OutputMode};
use crate::output::{format_deps, Dependency};

/// Run the deps command: analyze internal dependencies of a symbol.
///
/// Finds the target symbol using the narrow pipeline (text pre-filter + parse),
/// extracts references from its body, then resolves each reference against a
/// project-wide symbol index to determine where dependencies are defined.
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or the parser cannot be created.
pub fn run(
    symbol: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
) -> anyhow::Result<ExitCode> {
    // 1. Resolve project root
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    // 2. Find the target symbol using narrow pipeline (same as def)
    let files = discover_files(&project_root, scope)?;
    let (target, target_source) = find_target_symbol(&files, &project_root, symbol)?;

    let Some(target_sym) = target else {
        if mode == OutputMode::Json {
            print_output(&format_deps(None, &[], symbol, mode, pretty));
        }
        return Ok(ExitCode::NoResults);
    };

    if target_sym.body.is_none() {
        // Symbol found but has no body (e.g., a type alias)
        print_output(&format_deps(Some(&target_sym), &[], symbol, mode, pretty));
        return Ok(ExitCode::Success);
    }

    // 3. Extract references within the body, resolve via index, format output
    let source = target_source.as_deref().unwrap_or("");
    let body_refs = extract_body_references(source, &target_sym)?;
    let dependencies = resolve_dependencies(source, &body_refs, symbol, &project_root)?;

    let output = format_deps(Some(&target_sym), &dependencies, symbol, mode, pretty);
    print_output(&output);

    Ok(ExitCode::Success)
}

/// Find the target symbol across project files using text pre-filter and parsing.
fn find_target_symbol(
    files: &[std::path::PathBuf],
    project_root: &Path,
    symbol: &str,
) -> anyhow::Result<(Option<Symbol>, Option<String>)> {
    let mut current_parser: Option<(codequery_core::Language, Parser)> = None;

    for relative_path in files {
        let absolute_path = project_root.join(relative_path);

        let Some(language) = language_for_file(relative_path) else {
            continue;
        };

        let Ok(source) = std::fs::read_to_string(&absolute_path) else {
            continue;
        };

        // Text pre-filter: skip files that don't contain the symbol name
        if !source.contains(symbol) {
            continue;
        }

        // Reuse parser if same language, otherwise create a new one
        let parser = match &mut current_parser {
            Some((lang, p)) if *lang == language => p,
            _ => {
                current_parser = Some((language, Parser::for_language(language)?));
                &mut current_parser.as_mut().expect("just assigned").1
            }
        };

        let Ok(tree) = parser.parse(source.as_bytes()) else {
            continue;
        };

        let symbols = extract_symbols(&source, &tree, relative_path, language);

        if let Some(found) = find_matching_symbol(&symbols, symbol) {
            return Ok((Some(found), Some(source)));
        }
    }

    Ok((None, None))
}

/// Extract references from the target symbol's body line range.
fn extract_body_references(
    source: &str,
    target: &Symbol,
) -> anyhow::Result<Vec<codequery_core::Reference>> {
    let Some(lang) = language_for_file(&target.file) else {
        return Ok(Vec::new());
    };

    let mut parser = Parser::for_language(lang)?;
    let tree = parser.parse(source.as_bytes())?;
    let all_refs = extract_references(source, &tree, &target.file, lang);

    // Filter to references within the symbol's line range
    Ok(all_refs
        .into_iter()
        .filter(|r| r.line >= target.line && r.line <= target.end_line)
        .collect())
}

/// Resolve body references to named dependencies via project-wide symbol index.
fn resolve_dependencies(
    source: &str,
    body_refs: &[codequery_core::Reference],
    symbol: &str,
    project_root: &Path,
) -> anyhow::Result<Vec<Dependency>> {
    let scan = scan_project(project_root, None)?;
    let index = SymbolIndex::from_scan(&scan);

    let mut seen = HashSet::new();
    let mut dependencies = Vec::new();

    for reference in body_refs {
        let Some(name) = extract_ref_name(source, reference.line, reference.column) else {
            continue;
        };

        // Skip self-references and deduplicates
        if name == symbol || !seen.insert(name.clone()) {
            continue;
        }

        let dep_kind = match reference.kind {
            ReferenceKind::Call => "call",
            ReferenceKind::TypeUsage => "type_reference",
            ReferenceKind::Import => "import",
            ReferenceKind::Assignment => "assignment",
        };

        let definitions = index.find_by_name(&name);
        let defined_in = definitions
            .first()
            .map(|sym| sym.file.display().to_string());

        dependencies.push(Dependency {
            name,
            kind: dep_kind.to_string(),
            defined_in,
        });
    }

    Ok(dependencies)
}

/// Print output if non-empty.
fn print_output(output: &str) {
    if !output.is_empty() {
        println!("{output}");
    }
}

/// Find a symbol by name, including inside impl blocks.
fn find_matching_symbol(symbols: &[Symbol], query: &str) -> Option<Symbol> {
    for symbol in symbols {
        if symbol.kind != SymbolKind::Impl && symbol.name == query {
            return Some(symbol.clone());
        }
        for child in &symbol.children {
            if child.name == query {
                return Some(child.clone());
            }
        }
    }
    None
}

/// Extract the identifier name at a given line and column from source text.
fn extract_ref_name(source: &str, line: usize, column: usize) -> Option<String> {
    let source_line = source.lines().nth(line.checked_sub(1)?)?;
    if column >= source_line.len() {
        return None;
    }
    let rest = &source_line[column..];
    let end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{SymbolKind, Visibility};
    use std::path::PathBuf;

    /// Path to the fixture rust project.
    fn fixture_project() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
    }

    /// Helper: create a Symbol for testing.
    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        children: Vec<Symbol>,
        body: Option<&str>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility: Visibility::Public,
            children,
            doc: None,
            body: body.map(String::from),
            signature: None,
        }
    }

    // -----------------------------------------------------------------------
    // find_matching_symbol
    // -----------------------------------------------------------------------

    #[test]
    fn test_deps_find_symbol_top_level() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            vec![],
            Some("pub fn greet() {}"),
        )];
        let found = find_matching_symbol(&symbols, "greet");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "greet");
    }

    #[test]
    fn test_deps_find_symbol_inside_impl() {
        let method = make_symbol(
            "is_adult",
            SymbolKind::Method,
            "src/services.rs",
            16,
            4,
            vec![],
            Some("pub fn is_adult(&self) -> bool { self.age >= 18 }"),
        );
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            vec![method],
            None,
        );
        let found = find_matching_symbol(&[impl_block], "is_adult");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "is_adult");
    }

    #[test]
    fn test_deps_find_symbol_not_found() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            vec![],
            None,
        )];
        let found = find_matching_symbol(&symbols, "nonexistent");
        assert!(found.is_none());
    }

    // -----------------------------------------------------------------------
    // extract_ref_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_deps_extract_ref_name_simple() {
        let source = "fn main() {\n    greet();\n}";
        let name = extract_ref_name(source, 2, 4);
        assert_eq!(name.as_deref(), Some("greet"));
    }

    #[test]
    fn test_deps_extract_ref_name_at_end_of_line() {
        let source = "fn test() {\n    foo\n}";
        let name = extract_ref_name(source, 2, 4);
        assert_eq!(name.as_deref(), Some("foo"));
    }

    #[test]
    fn test_deps_extract_ref_name_invalid_line() {
        let source = "fn main() {}";
        let name = extract_ref_name(source, 5, 0);
        assert!(name.is_none());
    }

    #[test]
    fn test_deps_extract_ref_name_column_past_end() {
        let source = "fn main() {}";
        let name = extract_ref_name(source, 1, 999);
        assert!(name.is_none());
    }

    // -----------------------------------------------------------------------
    // Fixture integration tests
    // -----------------------------------------------------------------------

    // Test 1: Finds function calls in a function body
    #[test]
    fn test_deps_finds_function_calls_in_body() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 2: Finds type references in parameters/return types
    #[test]
    fn test_deps_finds_type_references() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 3: Unresolvable dependencies have defined_in: null
    #[test]
    fn test_deps_unresolvable_has_null_defined_in() {
        let project = fixture_project();
        // greet calls format! which won't resolve
        let result = run("greet", Some(&project), None, OutputMode::Json, true);
        assert!(result.is_ok());
    }

    // Test 4: Symbol not found returns exit code 1
    #[test]
    fn test_deps_symbol_not_found_returns_no_results() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }

    // Test 5: JSON includes best-effort metadata
    #[test]
    fn test_deps_json_includes_best_effort_metadata() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Json,
            true,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test 6: All output modes work
    #[test]
    fn test_deps_raw_mode_works() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Raw,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_deps_framed_mode_works() {
        let project = fixture_project();
        let result = run(
            "process_users",
            Some(&project),
            None,
            OutputMode::Framed,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // Test: JSON mode with symbol not found still produces JSON output
    #[test]
    fn test_deps_json_mode_symbol_not_found() {
        let project = fixture_project();
        let result = run(
            "nonexistent_symbol_xyz",
            Some(&project),
            None,
            OutputMode::Json,
            true,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::NoResults);
    }
}
