//! Shared narrow-command pipeline for symbol-by-name searches.
//!
//! The `def`, `body`, `sig`, and `deps` commands all share the same search
//! pipeline: discover files, text pre-filter, parse, extract symbols, match
//! by name. This module provides the common implementation so each command
//! only needs to handle formatting.

use std::path::{Path, PathBuf};

use codequery_core::{
    detect_project_root_or, discover_files, language_for_file, language_name_for_file, Language,
    Symbol, SymbolKind,
};
use codequery_parse::{extract_symbols, extract_symbols_by_name, Parser};

/// Search the project for symbols matching `name`, returning all matches sorted
/// by file path then line number.
///
/// Implements the narrow-command pipeline:
/// 1. Resolve project root
/// 2. Discover files (optionally scoped, optionally filtered by language)
/// 3. For each file: detect language, read source, text pre-filter, parse, extract, match
/// 4. Sort results by file path then line number
///
/// # Errors
///
/// Returns an error if the project root cannot be detected, file discovery
/// fails, or a parser cannot be created.
pub fn find_symbols_by_name(
    name: &str,
    project: Option<&Path>,
    scope: Option<&Path>,
    lang_filter: Option<Language>,
) -> anyhow::Result<Vec<Symbol>> {
    let cwd = std::env::current_dir()?;
    let project_root = detect_project_root_or(&cwd, project)?;

    let files = discover_files(&project_root, scope)?;

    let mut matches = search_files_for_symbol(&files, &project_root, name, lang_filter)?;

    matches.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    Ok(matches)
}

/// Search the project for the first symbol matching `name`, returning the symbol
/// and its file's source text.
///
/// Used by `deps` which needs the source for reference extraction.
///
/// # Errors
///
/// Returns an error if a parser cannot be created.
pub fn find_first_symbol_with_source(
    files: &[PathBuf],
    project_root: &Path,
    name: &str,
    lang_filter: Option<Language>,
) -> anyhow::Result<(Option<Symbol>, Option<String>)> {
    let mut current_parser: Option<(Language, Parser)> = None;
    let mut runtime_parser: Option<(String, Parser)> = None;

    for relative_path in files {
        let absolute_path = project_root.join(relative_path);

        let (symbols, source) = if let Some(language) = language_for_file(relative_path) {
            if let Some(filter) = lang_filter {
                if language != filter {
                    continue;
                }
            }

            let Ok(source) = std::fs::read_to_string(&absolute_path) else {
                continue;
            };
            if !source.contains(name) {
                continue;
            }

            let parser = get_or_create_parser(&mut current_parser, language)?;
            let Ok(tree) = parser.parse(source.as_bytes()) else {
                continue;
            };
            (
                extract_symbols(&source, &tree, relative_path, language),
                source,
            )
        } else if let Some(lang_name) = language_name_for_file(relative_path) {
            if lang_filter.is_some() {
                continue;
            }

            let Ok(source) = std::fs::read_to_string(&absolute_path) else {
                continue;
            };
            if !source.contains(name) {
                continue;
            }

            let Ok(parser) = get_or_create_runtime_parser(&mut runtime_parser, &lang_name) else {
                continue;
            };
            let Ok(tree) = parser.parse(source.as_bytes()) else {
                continue;
            };
            (
                extract_symbols_by_name(&source, &tree, relative_path, &lang_name),
                source,
            )
        } else {
            continue;
        };

        for symbol in &symbols {
            if symbol.kind != SymbolKind::Impl && symbol.name == name {
                return Ok((Some(symbol.clone()), Some(source)));
            }
            for child in &symbol.children {
                if child.name == name {
                    return Ok((Some(child.clone()), Some(source)));
                }
            }
        }
    }

    Ok((None, None))
}

/// Collect symbols matching `query` from a flat symbol list, including impl children.
///
/// Matches top-level symbols by name (excluding impl blocks), and also
/// matches methods/children inside impl blocks by name.
pub fn collect_matching_symbols(symbols: &[Symbol], query: &str, matches: &mut Vec<Symbol>) {
    for symbol in symbols {
        if symbol.kind != SymbolKind::Impl && symbol.name == query {
            matches.push(symbol.clone());
        }

        for child in &symbol.children {
            if child.name == query {
                matches.push(child.clone());
            }
        }
    }
}

/// Internal: iterate files, pre-filter, parse, extract, and collect all matches.
fn search_files_for_symbol(
    files: &[PathBuf],
    project_root: &Path,
    name: &str,
    lang_filter: Option<Language>,
) -> anyhow::Result<Vec<Symbol>> {
    let mut matches: Vec<Symbol> = Vec::new();
    let mut current_parser: Option<(Language, Parser)> = None;
    let mut runtime_parser: Option<(String, Parser)> = None;

    for relative_path in files {
        let absolute_path = project_root.join(relative_path);

        // Try builtin language (fast path)
        if let Some(language) = language_for_file(relative_path) {
            if let Some(filter) = lang_filter {
                if language != filter {
                    continue;
                }
            }

            let Ok(source) = std::fs::read_to_string(&absolute_path) else {
                continue;
            };

            if !source.contains(name) {
                continue;
            }

            let parser = get_or_create_parser(&mut current_parser, language)?;
            let Ok(tree) = parser.parse(source.as_bytes()) else {
                continue;
            };
            let symbols = extract_symbols(&source, &tree, relative_path, language);
            collect_matching_symbols(&symbols, name, &mut matches);
            continue;
        }

        // Fallback: runtime language via registry name
        let Some(lang_name) = language_name_for_file(relative_path) else {
            continue;
        };

        // Language enum filter can't match runtime languages — skip
        if lang_filter.is_some() {
            continue;
        }

        let Ok(source) = std::fs::read_to_string(&absolute_path) else {
            continue;
        };

        if !source.contains(name) {
            continue;
        }

        let Ok(parser) = get_or_create_runtime_parser(&mut runtime_parser, &lang_name) else {
            continue;
        };
        let Ok(tree) = parser.parse(source.as_bytes()) else {
            continue;
        };
        let symbols = extract_symbols_by_name(&source, &tree, relative_path, &lang_name);
        collect_matching_symbols(&symbols, name, &mut matches);
    }

    Ok(matches)
}

/// Reuse the current parser if it matches the language, otherwise create a new one.
fn get_or_create_parser(
    current: &mut Option<(Language, Parser)>,
    language: Language,
) -> anyhow::Result<&mut Parser> {
    match current {
        Some((lang, _)) if *lang == language => {}
        _ => {
            *current = Some((language, Parser::for_language(language)?));
        }
    }
    Ok(&mut current.as_mut().expect("just assigned or matched").1)
}

/// Reuse the current parser if it matches the runtime language name, otherwise create a new one.
fn get_or_create_runtime_parser<'a>(
    current: &'a mut Option<(String, Parser)>,
    name: &'a str,
) -> anyhow::Result<&'a mut Parser> {
    match current {
        Some((n, _)) if n == name => {}
        _ => {
            *current = Some((name.to_string(), Parser::for_name(name)?));
        }
    }
    Ok(&mut current.as_mut().expect("just assigned or matched").1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::Visibility;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        children: Vec<Symbol>,
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
            body: None,
            signature: None,
        }
    }

    #[test]
    fn test_collect_matching_symbols_finds_top_level() {
        let symbols = vec![
            make_symbol("greet", SymbolKind::Function, "src/lib.rs", 9, 0, vec![]),
            make_symbol("other", SymbolKind::Function, "src/lib.rs", 20, 0, vec![]),
        ];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "greet", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "greet");
    }

    #[test]
    fn test_collect_matching_symbols_finds_method_in_impl() {
        let method = make_symbol(
            "is_adult",
            SymbolKind::Method,
            "src/services.rs",
            16,
            4,
            vec![],
        );
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            vec![method],
        );
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block], "is_adult", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "is_adult");
    }

    #[test]
    fn test_collect_matching_symbols_skips_impl_block_name() {
        let method = make_symbol("new", SymbolKind::Method, "src/services.rs", 8, 4, vec![]);
        let impl_block = make_symbol(
            "User",
            SymbolKind::Impl,
            "src/services.rs",
            6,
            0,
            vec![method],
        );
        let struct_def = make_symbol("User", SymbolKind::Struct, "src/models.rs", 5, 0, vec![]);
        let mut matches = Vec::new();
        collect_matching_symbols(&[impl_block, struct_def], "User", &mut matches);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn test_collect_matching_symbols_not_found_returns_empty() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            vec![],
        )];
        let mut matches = Vec::new();
        collect_matching_symbols(&symbols, "nonexistent", &mut matches);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_symbols_by_name_against_fixture() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let matches = find_symbols_by_name("greet", Some(&fixture), None, None).unwrap();
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|s| s.name == "greet"));
    }

    #[test]
    fn test_find_symbols_by_name_not_found() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let matches =
            find_symbols_by_name("nonexistent_symbol_xyz", Some(&fixture), None, None).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_symbols_by_name_with_lang_filter() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        // Searching with Rust filter in a Rust project should still find results
        let matches =
            find_symbols_by_name("greet", Some(&fixture), None, Some(Language::Rust)).unwrap();
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_find_symbols_by_name_lang_filter_excludes_other_languages() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        // Searching with Python filter in a Rust-only project should find nothing
        let matches =
            find_symbols_by_name("greet", Some(&fixture), None, Some(Language::Python)).unwrap();
        assert!(matches.is_empty());
    }
}
