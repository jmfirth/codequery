//! End-to-end test for the WASM plugin system.
//!
//! Proves the full path: load WASM grammar → parse source → extract symbols
//! via extract.toml → verify correct results.

#[cfg(feature = "wasm")]
mod wasm_e2e {
    use std::path::{Path, PathBuf};

    use codequery_core::{load_extract_config, SymbolKind};
    use codequery_parse::extract_engine::extract_with_config_uncached;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/wasm_plugin_test")
    }

    /// End-to-end: WASM grammar + extract.toml → symbols from Python source.
    ///
    /// This is the critical test that proves the plugin system works:
    /// 1. Load a .wasm grammar file
    /// 2. Parse source code with it
    /// 3. Extract symbols using extract.toml rules
    /// 4. Verify correct symbol names and kinds
    #[test]
    fn wasm_grammar_plus_extract_toml_produces_correct_symbols() {
        let dir = fixture_dir();
        let wasm_path = dir.join("grammar.wasm");
        let extract_path = dir.join("extract.toml");
        let source_path = dir.join("sample.py");

        // Skip if WASM fixture doesn't exist (CI might not have it)
        if !wasm_path.exists() {
            eprintln!("Skipping WASM e2e test: grammar.wasm not found");
            return;
        }

        // 1. Load the WASM grammar
        let mut parser = tree_sitter::Parser::new();
        let engine = tree_sitter::wasmtime::Engine::default();
        let mut store = tree_sitter::WasmStore::new(&engine).expect("failed to create WasmStore");
        let wasm_bytes = std::fs::read(&wasm_path).expect("failed to read grammar.wasm");
        let language = store
            .load_language("python", &wasm_bytes)
            .expect("failed to load WASM language");
        parser
            .set_wasm_store(store)
            .expect("failed to set WasmStore");
        parser
            .set_language(&language)
            .expect("failed to set language");

        // 2. Parse the source file
        let source = std::fs::read_to_string(&source_path).expect("failed to read sample.py");
        let tree = parser
            .parse(source.as_bytes(), None)
            .expect("failed to parse source");

        // 3. Load extract.toml and extract symbols
        let config_text =
            std::fs::read_to_string(&extract_path).expect("failed to read extract.toml");
        let config = load_extract_config(&config_text).expect("failed to parse extract.toml");

        let symbols = extract_with_config_uncached(
            &config,
            &source,
            &tree,
            Path::new("sample.py"),
            &language,
        );

        // 4. Verify results
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<SymbolKind> = symbols.iter().map(|s| s.kind).collect();

        eprintln!("Extracted {} symbols:", symbols.len());
        for s in &symbols {
            eprintln!("  {} ({:?}) at line {}", s.name, s.kind, s.line);
        }

        // Must find all 3 functions
        assert!(
            names.contains(&"greet"),
            "should find 'greet' function, got: {names:?}"
        );
        assert!(
            names.contains(&"add"),
            "should find 'add' function, got: {names:?}"
        );

        // Must find the class
        assert!(
            names.contains(&"User"),
            "should find 'User' class, got: {names:?}"
        );

        // Verify kinds
        let greet_sym = symbols.iter().find(|s| s.name == "greet").unwrap();
        assert_eq!(greet_sym.kind, SymbolKind::Function);

        let user_sym = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_sym.kind, SymbolKind::Class);

        // Verify line numbers (1-based)
        assert_eq!(greet_sym.line, 1, "greet should be on line 1");
        assert_eq!(user_sym.line, 4, "User should be on line 4");

        // Verify body extraction
        assert!(
            greet_sym.body.as_ref().is_some_and(|b| b.contains("Hello")),
            "greet body should contain 'Hello'"
        );
    }

    /// Verify that the extract.toml config validates against the WASM grammar.
    #[test]
    fn extract_toml_queries_compile_against_wasm_grammar() {
        let dir = fixture_dir();
        let wasm_path = dir.join("grammar.wasm");
        let extract_path = dir.join("extract.toml");

        if !wasm_path.exists() {
            eprintln!("Skipping: grammar.wasm not found");
            return;
        }

        let engine = tree_sitter::wasmtime::Engine::default();
        let mut store = tree_sitter::WasmStore::new(&engine).unwrap();
        let wasm_bytes = std::fs::read(&wasm_path).unwrap();
        let language = store.load_language("python", &wasm_bytes).unwrap();

        let config_text = std::fs::read_to_string(&extract_path).unwrap();
        let config = load_extract_config(&config_text).unwrap();

        // Every query in the config should compile against the grammar
        for rule in &config.symbols {
            let result = tree_sitter::Query::new(&language, &rule.query);
            assert!(
                result.is_ok(),
                "Query for {:?} failed to compile: {:?}\nQuery: {}",
                rule.kind,
                result.err(),
                rule.query
            );
        }
    }
}
