//! In-memory symbol index for project-wide lookups.
//!
//! Builds from scan results and provides fast lookup by name, kind, and file.
//! This is the backbone for wide commands (refs, callers, deps).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codequery_core::{Symbol, SymbolKind};

use crate::scanner::FileSymbols;

/// In-memory index of all symbols in a project.
///
/// Built from [`FileSymbols`] scan results, the index provides O(1) lookup
/// by name, kind, or file path. Symbols are stored in a flat vector with
/// secondary hash maps pointing into it by index.
pub struct SymbolIndex {
    symbols: Vec<Symbol>,
    by_name: HashMap<String, Vec<usize>>,
    by_file: HashMap<PathBuf, Vec<usize>>,
    by_kind: HashMap<SymbolKind, Vec<usize>>,
}

impl SymbolIndex {
    /// Build an index from scan results.
    ///
    /// Flattens all symbols (including children) from each file's scan results
    /// into a single searchable index.
    #[must_use]
    pub fn from_scan(scan: &[FileSymbols]) -> Self {
        let mut symbols = Vec::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_file: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        let mut by_kind: HashMap<SymbolKind, Vec<usize>> = HashMap::new();

        for file_symbols in scan {
            Self::collect_symbols(
                &file_symbols.symbols,
                &mut symbols,
                &mut by_name,
                &mut by_file,
                &mut by_kind,
            );
        }

        Self {
            symbols,
            by_name,
            by_file,
            by_kind,
        }
    }

    /// Recursively collect symbols and their children into the flat index.
    fn collect_symbols(
        source_symbols: &[Symbol],
        symbols: &mut Vec<Symbol>,
        by_name: &mut HashMap<String, Vec<usize>>,
        by_file: &mut HashMap<PathBuf, Vec<usize>>,
        by_kind: &mut HashMap<SymbolKind, Vec<usize>>,
    ) {
        for sym in source_symbols {
            let idx = symbols.len();
            symbols.push(sym.clone());
            by_name.entry(sym.name.clone()).or_default().push(idx);
            by_file.entry(sym.file.clone()).or_default().push(idx);
            by_kind.entry(sym.kind).or_default().push(idx);

            // Recursively index children (e.g., methods inside impl blocks)
            if !sym.children.is_empty() {
                Self::collect_symbols(&sym.children, symbols, by_name, by_file, by_kind);
            }
        }
    }

    /// Find symbols by name.
    ///
    /// Returns all symbols whose name exactly matches `name`.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Vec<&Symbol> {
        self.by_name.get(name).map_or_else(Vec::new, |indices| {
            indices.iter().map(|&i| &self.symbols[i]).collect()
        })
    }

    /// Find symbols by kind.
    ///
    /// Returns all symbols of the given kind.
    #[must_use]
    pub fn find_by_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.by_kind.get(&kind).map_or_else(Vec::new, |indices| {
            indices.iter().map(|&i| &self.symbols[i]).collect()
        })
    }

    /// Get all symbols in a file.
    ///
    /// Returns symbols whose `file` path matches `file`.
    #[must_use]
    pub fn symbols_in_file(&self, file: &Path) -> Vec<&Symbol> {
        self.by_file.get(file).map_or_else(Vec::new, |indices| {
            indices.iter().map(|&i| &self.symbols[i]).collect()
        })
    }

    /// Get all symbols in the index.
    #[must_use]
    pub fn all_symbols(&self) -> &[Symbol] {
        &self.symbols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::Visibility;
    use std::path::PathBuf;

    /// Create a minimal Symbol for testing.
    fn make_symbol(name: &str, kind: SymbolKind, file: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line: 1,
            column: 0,
            end_line: 5,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        }
    }

    fn make_file_symbols(file: &str, symbols: Vec<Symbol>) -> FileSymbols {
        FileSymbols {
            file: PathBuf::from(file),
            symbols,
            source: String::new(),
        }
    }

    // -----------------------------------------------------------------------
    // from_scan
    // -----------------------------------------------------------------------

    #[test]
    fn test_index_from_scan_builds_from_scan_results() {
        let scan = vec![
            make_file_symbols(
                "src/lib.rs",
                vec![
                    make_symbol("greet", SymbolKind::Function, "src/lib.rs"),
                    make_symbol("User", SymbolKind::Struct, "src/lib.rs"),
                ],
            ),
            make_file_symbols(
                "src/models.rs",
                vec![make_symbol("Role", SymbolKind::Enum, "src/models.rs")],
            ),
        ];

        let index = SymbolIndex::from_scan(&scan);
        assert_eq!(index.all_symbols().len(), 3);
    }

    #[test]
    fn test_index_from_scan_empty_returns_empty() {
        let scan: Vec<FileSymbols> = vec![];
        let index = SymbolIndex::from_scan(&scan);
        assert!(index.all_symbols().is_empty());
    }

    #[test]
    fn test_index_from_scan_includes_children() {
        let child = make_symbol("new", SymbolKind::Method, "src/lib.rs");
        let mut parent = make_symbol("User", SymbolKind::Impl, "src/lib.rs");
        parent.children.push(child);

        let scan = vec![make_file_symbols("src/lib.rs", vec![parent])];
        let index = SymbolIndex::from_scan(&scan);

        // Should have both the impl and the method
        assert_eq!(index.all_symbols().len(), 2);
        assert_eq!(index.find_by_name("new").len(), 1);
        assert_eq!(index.find_by_kind(SymbolKind::Method).len(), 1);
    }

    // -----------------------------------------------------------------------
    // find_by_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_index_find_by_name_returns_correct_symbols() {
        let scan = vec![make_file_symbols(
            "src/lib.rs",
            vec![
                make_symbol("greet", SymbolKind::Function, "src/lib.rs"),
                make_symbol("User", SymbolKind::Struct, "src/lib.rs"),
            ],
        )];

        let index = SymbolIndex::from_scan(&scan);

        let results = index.find_by_name("greet");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "greet");
        assert_eq!(results[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_index_find_by_name_returns_empty_for_missing() {
        let scan = vec![make_file_symbols(
            "src/lib.rs",
            vec![make_symbol("greet", SymbolKind::Function, "src/lib.rs")],
        )];

        let index = SymbolIndex::from_scan(&scan);
        assert!(index.find_by_name("nonexistent").is_empty());
    }

    #[test]
    fn test_index_find_by_name_returns_multiple_matches() {
        let scan = vec![
            make_file_symbols(
                "src/a.rs",
                vec![make_symbol("helper", SymbolKind::Function, "src/a.rs")],
            ),
            make_file_symbols(
                "src/b.rs",
                vec![make_symbol("helper", SymbolKind::Function, "src/b.rs")],
            ),
        ];

        let index = SymbolIndex::from_scan(&scan);
        let results = index.find_by_name("helper");
        assert_eq!(results.len(), 2);
    }

    // -----------------------------------------------------------------------
    // find_by_kind
    // -----------------------------------------------------------------------

    #[test]
    fn test_index_find_by_kind_filters_correctly() {
        let scan = vec![make_file_symbols(
            "src/lib.rs",
            vec![
                make_symbol("greet", SymbolKind::Function, "src/lib.rs"),
                make_symbol("User", SymbolKind::Struct, "src/lib.rs"),
                make_symbol("hello", SymbolKind::Function, "src/lib.rs"),
            ],
        )];

        let index = SymbolIndex::from_scan(&scan);

        let functions = index.find_by_kind(SymbolKind::Function);
        assert_eq!(functions.len(), 2);
        assert!(functions.iter().all(|s| s.kind == SymbolKind::Function));

        let structs = index.find_by_kind(SymbolKind::Struct);
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "User");
    }

    #[test]
    fn test_index_find_by_kind_returns_empty_for_no_matches() {
        let scan = vec![make_file_symbols(
            "src/lib.rs",
            vec![make_symbol("greet", SymbolKind::Function, "src/lib.rs")],
        )];

        let index = SymbolIndex::from_scan(&scan);
        assert!(index.find_by_kind(SymbolKind::Trait).is_empty());
    }

    // -----------------------------------------------------------------------
    // symbols_in_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_index_symbols_in_file_returns_file_symbols() {
        let scan = vec![
            make_file_symbols(
                "src/lib.rs",
                vec![
                    make_symbol("greet", SymbolKind::Function, "src/lib.rs"),
                    make_symbol("User", SymbolKind::Struct, "src/lib.rs"),
                ],
            ),
            make_file_symbols(
                "src/models.rs",
                vec![make_symbol("Role", SymbolKind::Enum, "src/models.rs")],
            ),
        ];

        let index = SymbolIndex::from_scan(&scan);

        let lib_symbols = index.symbols_in_file(Path::new("src/lib.rs"));
        assert_eq!(lib_symbols.len(), 2);

        let model_symbols = index.symbols_in_file(Path::new("src/models.rs"));
        assert_eq!(model_symbols.len(), 1);
        assert_eq!(model_symbols[0].name, "Role");
    }

    #[test]
    fn test_index_symbols_in_file_returns_empty_for_unknown_file() {
        let scan = vec![make_file_symbols(
            "src/lib.rs",
            vec![make_symbol("greet", SymbolKind::Function, "src/lib.rs")],
        )];

        let index = SymbolIndex::from_scan(&scan);
        assert!(index
            .symbols_in_file(Path::new("nonexistent.rs"))
            .is_empty());
    }

    // -----------------------------------------------------------------------
    // Integration: real fixture scan
    // -----------------------------------------------------------------------

    #[test]
    fn test_index_from_real_scan_finds_fixture_symbols() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project");
        let scan = crate::scan_project(&fixture, None).unwrap();
        let index = SymbolIndex::from_scan(&scan);

        // The fixture has "greet" in lib.rs
        let greets = index.find_by_name("greet");
        assert!(
            !greets.is_empty(),
            "expected 'greet' in index from fixture scan"
        );

        // Should have struct types
        let structs = index.find_by_kind(SymbolKind::Struct);
        assert!(
            !structs.is_empty(),
            "expected at least one struct in fixture"
        );

        // Should have symbols in multiple files
        assert!(
            index.all_symbols().len() > 5,
            "expected more than 5 symbols from fixture, got {}",
            index.all_symbols().len()
        );
    }
}
