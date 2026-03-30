//! Declarative symbol extraction engine.
//!
//! Takes an [`ExtractConfig`] (parsed from `extract.toml`), a tree-sitter
//! parse tree, and source text, and produces `Vec<Symbol>`. This is the
//! runtime engine that plugin languages use instead of compiled-in Rust
//! extraction code.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use codequery_core::extract_config::{parse_symbol_kind, ExtractConfig};
use codequery_core::{Symbol, SymbolKind, Visibility};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Query, QueryCursor};

/// A compiled set of extraction rules, ready for repeated use.
///
/// Pre-compiles tree-sitter queries from an [`ExtractConfig`] so they can
/// be reused across many files without recompilation overhead.
pub struct CompiledExtractor {
    rules: Vec<CompiledRule>,
}

/// A single compiled extraction rule.
struct CompiledRule {
    kind: SymbolKind,
    query: Query,
    name_capture: String,
    body_capture: Option<String>,
    doc_strategy: Option<String>,
    visibility_strategy: Option<String>,
}

/// Cache of compiled extractors keyed by language name.
///
/// Tree-sitter query compilation is expensive. This cache ensures each
/// unique `ExtractConfig` is compiled only once per process lifetime.
static COMPILED_CACHE: std::sync::LazyLock<Mutex<HashMap<String, Arc<CompiledExtractor>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

impl CompiledExtractor {
    /// Compile an extractor from a configuration and tree-sitter language.
    ///
    /// Rules whose queries fail to compile are logged (via eprintln) and
    /// skipped — a bad rule does not prevent other rules from working.
    pub fn compile(config: &ExtractConfig, ts_language: &Language) -> Self {
        let mut rules = Vec::new();

        for rule in &config.symbols {
            let Some(kind) = parse_symbol_kind(&rule.kind) else {
                eprintln!(
                    "cq: extract.toml warning: unknown symbol kind '{}', skipping rule",
                    rule.kind
                );
                continue;
            };

            match Query::new(ts_language, &rule.query) {
                Ok(query) => {
                    rules.push(CompiledRule {
                        kind,
                        query,
                        name_capture: strip_at(&rule.name),
                        body_capture: rule.body.as_deref().map(strip_at),
                        doc_strategy: rule.doc.clone(),
                        visibility_strategy: rule.visibility.clone(),
                    });
                }
                Err(e) => {
                    eprintln!(
                        "cq: extract.toml warning: query for '{}' failed to compile: {e}, skipping rule",
                        rule.kind
                    );
                }
            }
        }

        Self { rules }
    }

    /// Get or compile a cached extractor for the given config and language.
    ///
    /// Uses the language name from the config as the cache key.
    pub fn get_or_compile(config: &ExtractConfig, ts_language: &Language) -> Arc<Self> {
        let key = config.language.name.clone();
        let mut cache = COMPILED_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(cached) = cache.get(&key) {
            return Arc::clone(cached);
        }

        let compiled = Arc::new(Self::compile(config, ts_language));
        cache.insert(key, Arc::clone(&compiled));
        compiled
    }

    /// Extract symbols from a parsed source file.
    ///
    /// Runs all compiled rules against the tree and collects matching symbols.
    #[must_use]
    pub fn extract(&self, source: &str, tree: &tree_sitter::Tree, file: &Path) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        for rule in &self.rules {
            extract_with_rule(rule, source, tree, file, &mut symbols);
        }

        symbols
    }
}

/// Run a single compiled rule against the tree and push matching symbols.
fn extract_with_rule(
    rule: &CompiledRule,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&rule.query, tree.root_node(), source.as_bytes());

    while let Some(match_) = matches.next() {
        let Some(name) = capture_text(&rule.query, match_, &rule.name_capture, source) else {
            continue;
        };

        let name_node = capture_node(&rule.query, match_, &rule.name_capture);

        let (line, column, end_line) = match name_node {
            Some(node) => (
                node.start_position().row + 1,
                node.start_position().column,
                // Use the overall match extent for end_line when a body capture exists
                match &rule.body_capture {
                    Some(cap) => capture_node(&rule.query, match_, cap)
                        .map_or(node.end_position().row + 1, |body_node| {
                            body_node.end_position().row + 1
                        }),
                    None => node.end_position().row + 1,
                },
            ),
            None => (1, 0, 1),
        };

        let body = rule
            .body_capture
            .as_ref()
            .and_then(|cap| capture_text(&rule.query, match_, cap, source));

        let visibility = determine_visibility(rule, &name);

        let doc = extract_doc(rule, match_, &rule.query, source);

        symbols.push(Symbol {
            name,
            kind: rule.kind,
            file: file.to_path_buf(),
            line,
            column,
            end_line,
            visibility,
            children: vec![],
            doc,
            body,
            signature: None,
        });
    }
}

/// Extract text from a named capture in a query match.
fn capture_text(
    query: &Query,
    match_: &tree_sitter::QueryMatch<'_, '_>,
    capture_name: &str,
    source: &str,
) -> Option<String> {
    let idx = query
        .capture_names()
        .iter()
        .position(|n| *n == capture_name)?;

    match_
        .captures
        .iter()
        .find(|c| c.index as usize == idx)
        .and_then(|c| c.node.utf8_text(source.as_bytes()).ok())
        .map(String::from)
}

/// Get the node from a named capture in a query match.
fn capture_node<'a>(
    query: &Query,
    match_: &tree_sitter::QueryMatch<'a, '_>,
    capture_name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let idx = query
        .capture_names()
        .iter()
        .position(|n| *n == capture_name)?;

    match_
        .captures
        .iter()
        .find(|c| c.index as usize == idx)
        .map(|c| c.node)
}

/// Determine symbol visibility based on the rule's visibility strategy.
fn determine_visibility(rule: &CompiledRule, name: &str) -> Visibility {
    match rule.visibility_strategy.as_deref() {
        Some("underscore_prefix") => {
            if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            }
        }
        Some("pub_keyword") => {
            // For pub_keyword, we would need to check the source text.
            // Default to public for now — the query itself can filter.
            Visibility::Public
        }
        _ => Visibility::Public,
    }
}

/// Extract documentation based on the rule's doc strategy.
fn extract_doc(
    rule: &CompiledRule,
    match_: &tree_sitter::QueryMatch<'_, '_>,
    query: &Query,
    source: &str,
) -> Option<String> {
    match rule.doc_strategy.as_deref() {
        Some("preceding_comment") => {
            // Find the name node and look for preceding comment nodes
            let name_node = capture_node(query, match_, &rule.name_capture)?;
            extract_preceding_comments(name_node, source)
        }
        Some("docstring") => {
            // Look for a @doc capture if present, otherwise try preceding_comment
            capture_text(query, match_, "doc", source)
        }
        _ => None,
    }
}

/// Extract preceding comment lines before a node.
fn extract_preceding_comments(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    // Walk backward from the node to find preceding comment siblings
    let mut comments = Vec::new();
    let mut current = node;

    // Go up to the parent to find siblings
    let parent = current.parent()?;
    let mut cursor = parent.walk();

    let siblings: Vec<_> = parent.children(&mut cursor).collect();
    let our_idx = siblings.iter().position(|s| s.id() == current.id())?;

    // Gather preceding comment nodes
    for i in (0..our_idx).rev() {
        let sibling = siblings[i];
        if sibling.kind() == "comment" || sibling.kind() == "line_comment" {
            if let Ok(text) = sibling.utf8_text(source.as_bytes()) {
                comments.push(text.to_string());
            }
        } else {
            break;
        }
    }

    if comments.is_empty() {
        // Also check: maybe the node itself is inside a larger construct
        // and the comments are siblings of the construct
        current = node;
        while let Some(p) = current.parent() {
            if let Some(prev) = p.prev_sibling() {
                if prev.kind() == "comment" || prev.kind() == "line_comment" {
                    if let Ok(text) = prev.utf8_text(source.as_bytes()) {
                        return Some(text.to_string());
                    }
                }
            }
            current = p;
        }
        return None;
    }

    comments.reverse();
    Some(comments.join("\n"))
}

/// Strip the leading `@` from a capture name, if present.
fn strip_at(s: &str) -> String {
    s.strip_prefix('@').unwrap_or(s).to_string()
}

/// Extract symbols from source code using a declarative configuration.
///
/// This is the main entry point for config-driven extraction. It compiles
/// (or retrieves cached) queries and runs them against the tree.
///
/// # Arguments
/// * `config` — the parsed `extract.toml` configuration
/// * `source` — the source text
/// * `tree` — the parsed tree-sitter tree
/// * `file` — the file path (stored in each `Symbol`)
/// * `ts_language` — the tree-sitter language grammar
#[must_use]
pub fn extract_with_config(
    config: &ExtractConfig,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    ts_language: &Language,
) -> Vec<Symbol> {
    let compiled = CompiledExtractor::get_or_compile(config, ts_language);
    compiled.extract(source, tree, file)
}

/// Extract symbols using a config and tree, without caching.
///
/// Useful for one-off extraction or testing where caching is unnecessary.
#[must_use]
pub fn extract_with_config_uncached(
    config: &ExtractConfig,
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    ts_language: &Language,
) -> Vec<Symbol> {
    let compiled = CompiledExtractor::compile(config, ts_language);
    compiled.extract(source, tree, file)
}

/// Validate that all queries in a config compile against the given language.
///
/// Returns a list of `(rule_index, error_message)` for rules that failed.
#[must_use]
pub fn validate_config(config: &ExtractConfig, ts_language: &Language) -> Vec<(usize, String)> {
    let mut errors = Vec::new();

    for (i, rule) in config.symbols.iter().enumerate() {
        if parse_symbol_kind(&rule.kind).is_none() {
            errors.push((i, format!("unknown symbol kind: '{}'", rule.kind)));
            continue;
        }

        if let Err(e) = Query::new(ts_language, &rule.query) {
            errors.push((i, format!("query compilation failed: {e}")));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::load_extract_config;
    use std::path::PathBuf;

    fn python_config() -> &'static str {
        r#"
[language]
name = "python_test"
extensions = [".py"]

[[symbols]]
kind = "function"
query = '(function_definition name: (identifier) @name) @def'
name = "@name"
body = "@def"
visibility = "underscore_prefix"

[[symbols]]
kind = "class"
query = '(class_definition name: (identifier) @name body: (block) @body) @def'
name = "@name"
body = "@def"
"#
    }

    fn parse_python(source: &str) -> (tree_sitter::Tree, Language) {
        let lang: Language = tree_sitter_python::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        (tree, lang)
    }

    #[test]
    fn test_extract_with_config_python_function() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def greet(name):\n    return name\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].line, 1);
        assert_eq!(symbols[0].column, 4);
    }

    #[test]
    fn test_extract_with_config_python_class() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "class Foo:\n    pass\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_with_config_multiple_symbols() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def foo():\n    pass\n\ndef bar():\n    pass\n\nclass Baz:\n    pass\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        assert_eq!(symbols.len(), 3);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"Baz"));
    }

    #[test]
    fn test_extract_with_config_body_captured() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def greet(name):\n    return name\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        assert_eq!(symbols.len(), 1);
        let body = symbols[0].body.as_deref().expect("body should be Some");
        assert!(body.contains("def greet(name):"));
        assert!(body.contains("return name"));
    }

    #[test]
    fn test_extract_with_config_visibility_underscore_prefix() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def public_fn():\n    pass\n\ndef _private_fn():\n    pass\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        let public = symbols.iter().find(|s| s.name == "public_fn").unwrap();
        assert_eq!(public.visibility, Visibility::Public);

        let private = symbols.iter().find(|s| s.name == "_private_fn").unwrap();
        assert_eq!(private.visibility, Visibility::Private);
    }

    #[test]
    fn test_extract_with_config_empty_source() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("empty.py"), &lang);

        assert!(symbols.is_empty());
    }

    #[test]
    fn test_extract_with_config_broken_source_partial_results() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def good():\n    pass\n\ndef broken(\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("broken.py"), &lang);

        // Should still extract the good function
        assert!(
            symbols.iter().any(|s| s.name == "good"),
            "should find 'good' despite broken sibling"
        );
    }

    #[test]
    fn test_validate_config_valid() {
        let config = load_extract_config(python_config()).unwrap();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let errors = validate_config(&config, &lang);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn test_validate_config_invalid_query() {
        let toml = r#"
[language]
name = "bad"
extensions = [".bad"]

[[symbols]]
kind = "function"
query = '(this_node_does_not_exist) @cap'
name = "@cap"
"#;
        let config = load_extract_config(toml).unwrap();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let errors = validate_config(&config, &lang);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].1.contains("query compilation failed"));
    }

    #[test]
    fn test_validate_config_unknown_kind() {
        let toml = r#"
[language]
name = "bad"
extensions = [".bad"]

[[symbols]]
kind = "lambda"
query = '(function_definition name: (identifier) @name)'
name = "@name"
"#;
        let config = load_extract_config(toml).unwrap();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let errors = validate_config(&config, &lang);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].1.contains("unknown symbol kind"));
    }

    #[test]
    fn test_strip_at_with_prefix() {
        assert_eq!(strip_at("@name"), "name");
    }

    #[test]
    fn test_strip_at_without_prefix() {
        assert_eq!(strip_at("name"), "name");
    }

    #[test]
    fn test_compiled_extractor_skips_bad_query() {
        let toml = r#"
[language]
name = "skiptest"
extensions = [".skip"]

[[symbols]]
kind = "function"
query = '(function_definition name: (identifier) @name) @def'
name = "@name"
body = "@def"

[[symbols]]
kind = "class"
query = '(nonexistent_node_type) @bad'
name = "@bad"
"#;
        let config = load_extract_config(toml).unwrap();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let compiled = CompiledExtractor::compile(&config, &lang);
        // Only the valid function rule should have compiled
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(compiled.rules[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_file_path_stored_in_symbols() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "def foo():\n    pass\n";
        let (tree, lang) = parse_python(source);
        let path = PathBuf::from("/project/src/foo.py");

        let symbols = extract_with_config_uncached(&config, source, &tree, &path, &lang);

        assert_eq!(symbols[0].file, path);
    }

    #[test]
    fn test_extract_with_config_line_numbers_1_based() {
        let config = load_extract_config(python_config()).unwrap();
        let source = "\n\ndef third_line():\n    pass\n";
        let (tree, lang) = parse_python(source);

        let symbols =
            extract_with_config_uncached(&config, source, &tree, Path::new("test.py"), &lang);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "third_line");
        assert_eq!(symbols[0].line, 3);
    }

    #[test]
    fn test_cached_extractor_returns_same_instance() {
        let config = load_extract_config(python_config()).unwrap();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let a = CompiledExtractor::get_or_compile(&config, &lang);
        let b = CompiledExtractor::get_or_compile(&config, &lang);

        assert!(Arc::ptr_eq(&a, &b), "should return cached instance");
    }

    // =======================================================================
    // Fixture-based integration: extract.toml vs compiled-in extraction
    // =======================================================================

    /// Load the template extract.toml for Python from the languages/ directory.
    fn load_python_extract_toml() -> ExtractConfig {
        let toml_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../languages/python/extract.toml");
        let toml_str = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", toml_path.display()));
        load_extract_config(&toml_str).unwrap()
    }

    /// Load the template extract.toml for Rust from the languages/ directory.
    fn load_rust_extract_toml() -> ExtractConfig {
        let toml_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../languages/rust/extract.toml");
        let toml_str = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", toml_path.display()));
        load_extract_config(&toml_str).unwrap()
    }

    #[test]
    fn test_python_extract_toml_loads_successfully() {
        let config = load_python_extract_toml();
        assert_eq!(config.language.name, "python");
        assert_eq!(config.language.extensions, vec![".py"]);
        assert!(
            config.symbols.len() >= 2,
            "should have at least function and class rules"
        );
    }

    #[test]
    fn test_python_extract_toml_queries_compile() {
        let config = load_python_extract_toml();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let errors = validate_config(&config, &lang);
        assert!(
            errors.is_empty(),
            "Python extract.toml has invalid queries: {errors:?}"
        );
    }

    #[test]
    fn test_python_extract_toml_matches_compiled_in_names() {
        let config = load_python_extract_toml();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/python_project/src/main.py");
        let source = std::fs::read_to_string(&fixture_path).unwrap();
        let (tree, _) = parse_python(&source);

        // Config-based extraction
        let config_symbols =
            extract_with_config_uncached(&config, &source, &tree, &fixture_path, &lang);

        // Compiled-in extraction
        let compiled_symbols = crate::extract_symbols(
            &source,
            &tree,
            &fixture_path,
            codequery_core::Language::Python,
        );

        // Both should find functions: greet, add, _private_helper
        let config_names: Vec<&str> = config_symbols.iter().map(|s| s.name.as_str()).collect();
        let compiled_names: Vec<&str> = compiled_symbols.iter().map(|s| s.name.as_str()).collect();

        for name in &["greet", "add", "_private_helper"] {
            assert!(
                config_names.contains(name),
                "config extraction missing function '{name}'"
            );
            assert!(
                compiled_names.contains(name),
                "compiled extraction missing function '{name}'"
            );
        }
    }

    #[test]
    fn test_python_extract_toml_matches_compiled_in_kinds() {
        let config = load_python_extract_toml();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/python_project/src/models.py");
        let source = std::fs::read_to_string(&fixture_path).unwrap();
        let (tree, _) = parse_python(&source);

        let config_symbols =
            extract_with_config_uncached(&config, &source, &tree, &fixture_path, &lang);

        let compiled_symbols = crate::extract_symbols(
            &source,
            &tree,
            &fixture_path,
            codequery_core::Language::Python,
        );

        // Both should find User and Admin classes
        let config_classes: Vec<&str> = config_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .map(|s| s.name.as_str())
            .collect();

        let compiled_classes: Vec<&str> = compiled_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .map(|s| s.name.as_str())
            .collect();

        assert!(
            config_classes.contains(&"User"),
            "config missing User class"
        );
        assert!(
            config_classes.contains(&"Admin"),
            "config missing Admin class"
        );
        assert!(
            compiled_classes.contains(&"User"),
            "compiled missing User class"
        );
        assert!(
            compiled_classes.contains(&"Admin"),
            "compiled missing Admin class"
        );
    }

    #[test]
    fn test_python_extract_toml_function_kinds_match() {
        let config = load_python_extract_toml();
        let lang: Language = tree_sitter_python::LANGUAGE.into();

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/python_project/src/main.py");
        let source = std::fs::read_to_string(&fixture_path).unwrap();
        let (tree, _) = parse_python(&source);

        let config_symbols =
            extract_with_config_uncached(&config, &source, &tree, &fixture_path, &lang);

        // Every function found by config should have Function kind
        // (compiled-in extraction differentiates test functions, but config
        // extraction produces what the rules say — both should agree on
        // basic functions)
        let greet = config_symbols
            .iter()
            .find(|s| s.name == "greet")
            .expect("config extraction missing 'greet'");
        assert_eq!(greet.kind, SymbolKind::Function);

        let add = config_symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("config extraction missing 'add'");
        assert_eq!(add.kind, SymbolKind::Function);
    }

    #[test]
    fn test_rust_extract_toml_loads_successfully() {
        let config = load_rust_extract_toml();
        assert_eq!(config.language.name, "rust");
        assert_eq!(config.language.extensions, vec![".rs"]);
        assert!(
            config.symbols.len() >= 5,
            "should have rules for functions, structs, enums, traits, etc."
        );
    }

    #[test]
    fn test_rust_extract_toml_queries_compile() {
        let config = load_rust_extract_toml();
        let lang: Language = tree_sitter_rust::LANGUAGE.into();

        let errors = validate_config(&config, &lang);
        assert!(
            errors.is_empty(),
            "Rust extract.toml has invalid queries: {errors:?}"
        );
    }

    #[test]
    fn test_rust_extract_toml_extracts_from_fixture() {
        let config = load_rust_extract_toml();
        let lang: Language = tree_sitter_rust::LANGUAGE.into();

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/src/lib.rs");
        let source = std::fs::read_to_string(&fixture_path).unwrap();

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();

        let config_symbols =
            extract_with_config_uncached(&config, &source, &tree, &fixture_path, &lang);

        let compiled_symbols = crate::extract_symbols(
            &source,
            &tree,
            &fixture_path,
            codequery_core::Language::Rust,
        );

        // Compare function names found by both
        let config_fn_names: Vec<&str> = config_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| s.name.as_str())
            .collect();

        let compiled_fn_names: Vec<&str> = compiled_symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| s.name.as_str())
            .collect();

        // The compiled-in extractor finds the "greet" function
        assert!(
            compiled_fn_names.contains(&"greet"),
            "compiled extraction should find 'greet'"
        );

        // The config-based extractor should also find it
        assert!(
            config_fn_names.contains(&"greet"),
            "config extraction should find 'greet'"
        );
    }
}
