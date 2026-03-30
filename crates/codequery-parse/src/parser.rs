//! Language-aware tree-sitter parser.
//!
//! Wraps a `tree_sitter::Parser` with the appropriate grammar pre-loaded
//! for any Tier 1 language. The parser is reusable across multiple files
//! of the same language.

use std::path::Path;

use codequery_core::Language;

use crate::error::{ParseError, Result};
use crate::runtime_grammar;
use crate::wasm_loader;

/// A tree-sitter parser configured for a specific source language.
///
/// Created via the [`Parser::for_language`] factory, which loads the
/// correct grammar. The parser is reusable across multiple files — call
/// `parse()` or `parse_file()` repeatedly without recreating the parser.
pub struct Parser {
    parser: tree_sitter::Parser,
    language: Language,
}

impl Parser {
    /// Create a parser for the given language.
    ///
    /// Loads the tree-sitter grammar corresponding to `language`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the grammar fails to load.
    pub fn for_language(language: Language) -> Result<Self> {
        // Try compiled-in grammar first (fast path)
        if let Some(grammar) = compiled_grammar(language) {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&grammar)
                .map_err(|e| ParseError::LanguageError(e.to_string()))?;
            return Ok(Self { parser, language });
        }

        // For non-compiled-in grammars, try runtime/WASM fallback.
        // Cannot delegate to for_name here (would recurse).
        Self::from_runtime_or_wasm(language.name(), language)
    }

    /// Create a parser for a language identified by name.
    ///
    /// Resolution order:
    /// 1. Builtin language (Tier 1/2 compiled grammars)
    /// 2. Native runtime grammar (`.so`/`.dylib` from `~/.local/share/cq/grammars/`)
    /// 3. WASM grammar (`.wasm` from `~/.local/share/cq/languages/<name>/grammar.wasm`)
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the name does not match
    /// any builtin language and no runtime or WASM grammar is available.
    pub fn for_name(name: &str) -> Result<Self> {
        // Try builtin languages first
        if let Some(lang) = Language::from_name(name) {
            return Self::for_language(lang);
        }

        // Unknown language name — try runtime/WASM with a placeholder variant
        Self::from_runtime_or_wasm(name, Language::Rust)
    }

    /// Try loading a grammar from native runtime (.so/.dylib) or WASM plugin.
    ///
    /// This is the shared fallback for `for_language` (when the compiled-in
    /// grammar isn't available) and `for_name` (when the name isn't a known
    /// Language variant). The `language` parameter is used as the Language
    /// variant on the returned Parser; for WASM/runtime grammars it may be
    /// a placeholder.
    fn from_runtime_or_wasm(name: &str, language: Language) -> Result<Self> {
        // Try native runtime grammar loading (.so/.dylib)
        match runtime_grammar::load_runtime_grammar(name) {
            Ok(grammar) => {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&grammar)
                    .map_err(|e| ParseError::LanguageError(e.to_string()))?;

                return Ok(Self { parser, language });
            }
            Err(_native_err) => {
                // Native grammar not found, try WASM next
            }
        }

        // Fall back to WASM grammar loading (check if already installed)
        if let Some(info) = wasm_loader::find_wasm_grammar(name) {
            let mut parser = tree_sitter::Parser::new();
            wasm_loader::load_wasm_language_cached(&info.wasm_path, &mut parser)?;

            return Ok(Self { parser, language });
        }

        // Auto-install: if the language is in the registry, download it on first use.
        // This makes the "75 languages, zero setup" promise real — any language works
        // the first time you use it, with a ~1-2s download on first encounter.
        if auto_install_grammar(name) {
            // Retry WASM loading after install
            if let Some(info) = wasm_loader::find_wasm_grammar(name) {
                let mut parser = tree_sitter::Parser::new();
                wasm_loader::load_wasm_language_cached(&info.wasm_path, &mut parser)?;
                return Ok(Self { parser, language });
            }
        }

        Err(ParseError::LanguageError(format!(
            "no grammar available for language '{name}': \
             not a builtin language, no runtime grammar, \
             and auto-install failed. Try: cq grammar install {name}"
        )))
    }

    /// The language this parser is configured for.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }
}

// ── Auto-install ────────────────────────────────────────────────

use std::sync::Mutex;

/// Languages we've already attempted to auto-install this process.
/// Prevents repeated download attempts (and 30s timeouts) for the same language
/// when scanning a project with many files of that type.
static AUTO_INSTALL_ATTEMPTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Attempt to auto-install a grammar from the registry via `cq grammar install`.
///
/// Returns `true` if the install succeeded and the grammar should now be available.
/// Only attempts once per language per process — subsequent calls return immediately.
fn auto_install_grammar(name: &str) -> bool {
    // Check if we've already tried this language
    {
        let attempted = AUTO_INSTALL_ATTEMPTED.lock().unwrap_or_else(|e| e.into_inner());
        if attempted.iter().any(|n| n == name) {
            return false;
        }
    }

    // Mark as attempted before trying (even if it fails)
    {
        let mut attempted = AUTO_INSTALL_ATTEMPTED.lock().unwrap_or_else(|e| e.into_inner());
        attempted.push(name.to_string());
    }

    eprintln!("cq: auto-installing {name} language support...");

    let version = env!("CARGO_PKG_VERSION");
    let url = format!(
        "https://github.com/jmfirth/codequery/releases/download/v{version}/lang-{name}.tar.gz"
    );

    // Determine install directory
    let Some(languages_dir) = codequery_core::dirs::languages_dir() else {
        return false;
    };
    let pkg_dir = languages_dir.join(name);
    if std::fs::create_dir_all(&pkg_dir).is_err() {
        return false;
    }

    // Download and extract via curl + tar (available on macOS/Linux)
    let download = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "5", &url, "-o", "-"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let Ok(result) = download else {
        let _ = std::fs::remove_dir_all(&pkg_dir);
        return false;
    };

    if !result.status.success() {
        let _ = std::fs::remove_dir_all(&pkg_dir);
        return false;
    }

    // Extract tarball
    let extract = std::process::Command::new("tar")
        .args(["xzf", "-", "-C"])
        .arg(&pkg_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(&result.stdout);
            }
            child.wait()
        });

    match extract {
        Ok(status) if status.success() => {
            // Verify we got a real grammar
            let grammar_path = pkg_dir.join("grammar.wasm");
            if grammar_path.exists()
                && std::fs::metadata(&grammar_path)
                    .map(|m| m.len() > 100)
                    .unwrap_or(false)
            {
                eprintln!("cq: {name} installed successfully");
                true
            } else {
                let _ = std::fs::remove_dir_all(&pkg_dir);
                false
            }
        }
        _ => {
            let _ = std::fs::remove_dir_all(&pkg_dir);
            false
        }
    }
}

impl Parser {
    /// Parse source bytes into a tree-sitter tree.
    ///
    /// Tree-sitter always produces a tree, even for invalid syntax.
    /// Check `tree.root_node().has_error()` to detect parse errors
    /// in the resulting tree.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::ParseFailed` if tree-sitter returns `None`,
    /// which should only happen if the language is not set.
    pub fn parse(&mut self, source: &[u8]) -> Result<tree_sitter::Tree> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseFailed("unknown source".to_string()))
    }

    /// Read a file from disk and parse it.
    ///
    /// Returns the file contents as a `String` and the parsed tree.
    /// The caller needs the source string because tree-sitter nodes
    /// reference byte ranges in the original source.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Io` if the file cannot be read, or
    /// `ParseError::ParseFailed` if tree-sitter returns no tree.
    pub fn parse_file(&mut self, path: &Path) -> Result<(String, tree_sitter::Tree)> {
        let source = std::fs::read_to_string(path)?;
        let tree = self.parse(source.as_bytes())?;
        Ok((source, tree))
    }
}

/// Select the compiled-in tree-sitter grammar for a language, if available.
///
/// Maps a `codequery_core::Language` to the corresponding `tree_sitter::Language`
/// grammar compiled into the binary. Returns `None` if the grammar's feature flag
/// is not enabled, in which case callers should fall back to WASM or runtime
/// grammar loading.
#[must_use]
pub fn compiled_grammar(language: Language) -> Option<tree_sitter::Language> {
    match language {
        #[cfg(feature = "lang-rust")]
        Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),

        #[cfg(feature = "lang-typescript")]
        Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),

        #[cfg(feature = "lang-javascript")]
        Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),

        #[cfg(feature = "lang-python")]
        Language::Python => Some(tree_sitter_python::LANGUAGE.into()),

        #[cfg(feature = "lang-go")]
        Language::Go => Some(tree_sitter_go::LANGUAGE.into()),

        #[cfg(feature = "lang-c")]
        Language::C => Some(tree_sitter_c::LANGUAGE.into()),

        #[cfg(feature = "lang-cpp")]
        Language::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),

        #[cfg(feature = "lang-java")]
        Language::Java => Some(tree_sitter_java::LANGUAGE.into()),

        #[cfg(feature = "lang-ruby")]
        Language::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),

        #[cfg(feature = "lang-php")]
        Language::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),

        #[cfg(feature = "lang-csharp")]
        Language::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),

        #[cfg(feature = "lang-swift")]
        Language::Swift => Some(tree_sitter_swift::LANGUAGE.into()),

        #[cfg(feature = "lang-kotlin")]
        Language::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),

        #[cfg(feature = "lang-scala")]
        Language::Scala => Some(tree_sitter_scala::LANGUAGE.into()),

        #[cfg(feature = "lang-zig")]
        Language::Zig => Some(tree_sitter_zig::LANGUAGE.into()),

        #[cfg(feature = "lang-lua")]
        Language::Lua => Some(tree_sitter_lua::LANGUAGE.into()),

        #[cfg(feature = "lang-bash")]
        Language::Bash => Some(tree_sitter_bash::LANGUAGE.into()),

        #[cfg(feature = "lang-html")]
        Language::Html => Some(tree_sitter_html::LANGUAGE.into()),

        #[cfg(feature = "lang-css")]
        Language::Css => Some(tree_sitter_css::LANGUAGE.into()),

        #[cfg(feature = "lang-json")]
        Language::Json => Some(tree_sitter_json::LANGUAGE.into()),

        #[cfg(feature = "lang-yaml")]
        Language::Yaml => Some(tree_sitter_yaml::LANGUAGE.into()),

        #[cfg(feature = "lang-toml")]
        Language::Toml => Some(tree_sitter_toml_ng::LANGUAGE.into()),

        // Catch-all for languages whose features are not enabled
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Select the tree-sitter grammar for a language.
///
/// Tries the compiled-in grammar first, then falls back to runtime (native
/// `.so`/`.dylib`) and WASM grammar loading. This is the primary entry point
/// for obtaining a grammar.
///
/// # Errors
///
/// Returns `ParseError::LanguageError` if no grammar is available by any method.
pub fn grammar_for_language(language: Language) -> Result<tree_sitter::Language> {
    // 1. Try compiled-in grammar
    if let Some(grammar) = compiled_grammar(language) {
        return Ok(grammar);
    }

    // 2. Try native runtime grammar (.so/.dylib)
    let name = language.name();
    if let Ok(grammar) = runtime_grammar::load_runtime_grammar(name) {
        return Ok(grammar);
    }

    // 3. Try WASM grammar
    if let Some(info) = wasm_loader::find_wasm_grammar(name) {
        let mut parser = tree_sitter::Parser::new();
        let grammar = wasm_loader::load_wasm_language_cached(&info.wasm_path, &mut parser)?;
        return Ok(grammar);
    }

    Err(ParseError::LanguageError(format!(
        "no grammar available for '{name}': not compiled in (enable feature \
         'lang-{name}'), and no runtime or WASM grammar installed. \
         Try: cq grammar install {name}"
    )))
}

/// A Rust-specific parser — convenience alias for backward compatibility.
///
/// Equivalent to `Parser::for_language(Language::Rust)`.
pub struct RustParser;

impl RustParser {
    /// Create a new parser with the Rust grammar loaded.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::LanguageError` if the Rust grammar fails to load.
    #[allow(clippy::new_ret_no_self)]
    // Backward-compatibility wrapper — intentionally returns Parser, not Self
    pub fn new() -> Result<Parser> {
        Parser::for_language(Language::Rust)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Backward-compatible: RustParser still works
    // -----------------------------------------------------------------------
    #[test]
    fn test_rust_parser_new_succeeds() {
        let parser = RustParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_rust_parser_returns_rust_language() {
        let parser = RustParser::new().unwrap();
        assert_eq!(parser.language(), Language::Rust);
    }

    // -----------------------------------------------------------------------
    // Parser::for_language for all 8 variants
    // -----------------------------------------------------------------------
    #[test]
    fn test_for_language_rust_creates_parser() {
        let parser = Parser::for_language(Language::Rust);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_typescript_creates_parser() {
        let parser = Parser::for_language(Language::TypeScript);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_javascript_creates_parser() {
        let parser = Parser::for_language(Language::JavaScript);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_python_creates_parser() {
        let parser = Parser::for_language(Language::Python);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_go_creates_parser() {
        let parser = Parser::for_language(Language::Go);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_c_creates_parser() {
        let parser = Parser::for_language(Language::C);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_cpp_creates_parser() {
        let parser = Parser::for_language(Language::Cpp);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_java_creates_parser() {
        let parser = Parser::for_language(Language::Java);
        assert!(parser.is_ok());
    }

    // -----------------------------------------------------------------------
    // Parsing with non-Rust languages produces valid trees
    // -----------------------------------------------------------------------
    #[test]
    fn test_parse_typescript_source_produces_tree() {
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let tree = parser
            .parse(b"function greet(name: string): string { return name; }")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_python_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser
            .parse(b"def greet(name: str) -> str:\n    return name\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_go_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let tree = parser
            .parse(b"package main\n\nfunc greet(name string) string {\n\treturn name\n}\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_c_source_produces_tree() {
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(b"int main() { return 0; }\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_cpp_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Cpp).unwrap();
        let tree = parser
            .parse(b"class Foo { public: void bar(); };\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_java_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser
            .parse(b"public class Main { public static void main(String[] args) {} }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_javascript_source_produces_tree() {
        let mut parser = Parser::for_language(Language::JavaScript).unwrap();
        let tree = parser
            .parse(b"function greet(name) { return name; }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    // -----------------------------------------------------------------------
    // Existing parser behavior tests (migrated from RustParser)
    // -----------------------------------------------------------------------
    #[test]
    fn test_parse_valid_rust_returns_tree_without_errors() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main() {}").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_broken_rust_returns_tree_with_errors() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main( {}").unwrap();
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_parse_empty_source_returns_valid_tree() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parser_is_reusable_across_multiple_sources() {
        let mut parser = RustParser::new().unwrap();

        let tree1 = parser.parse(b"fn foo() {}").unwrap();
        assert!(!tree1.root_node().has_error());

        let tree2 = parser.parse(b"struct Bar { x: i32 }").unwrap();
        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn test_parse_file_reads_and_parses_fixture() {
        let mut parser = RustParser::new().unwrap();
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rust_project/src/lib.rs");
        let (source, tree) = parser.parse_file(&fixture_path).unwrap();

        assert!(!source.is_empty());
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_file_nonexistent_returns_io_error() {
        let mut parser = RustParser::new().unwrap();
        let result = parser.parse_file(Path::new("/nonexistent/path/file.rs"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::Io(_)));
    }

    #[test]
    fn test_parse_valid_rust_root_node_is_source_file() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main() {}").unwrap();
        assert_eq!(tree.root_node().kind(), "source_file");
    }

    // -----------------------------------------------------------------------
    // Tier 2: Parser::for_language creates parsers
    // -----------------------------------------------------------------------

    #[test]
    fn test_for_language_ruby_creates_parser() {
        let parser = Parser::for_language(Language::Ruby);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_php_creates_parser() {
        let parser = Parser::for_language(Language::Php);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_csharp_creates_parser() {
        let parser = Parser::for_language(Language::CSharp);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_swift_creates_parser() {
        let parser = Parser::for_language(Language::Swift);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_kotlin_creates_parser() {
        let parser = Parser::for_language(Language::Kotlin);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_scala_creates_parser() {
        let parser = Parser::for_language(Language::Scala);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_zig_creates_parser() {
        let parser = Parser::for_language(Language::Zig);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_lua_creates_parser() {
        let parser = Parser::for_language(Language::Lua);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_for_language_bash_creates_parser() {
        let parser = Parser::for_language(Language::Bash);
        assert!(parser.is_ok());
    }

    // -----------------------------------------------------------------------
    // Tier 2: Parsing produces valid trees
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_ruby_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Ruby).unwrap();
        let tree = parser.parse(b"def greet(name)\n  name\nend\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_php_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Php).unwrap();
        let tree = parser
            .parse(b"<?php\nfunction greet($name) { return $name; }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_csharp_source_produces_tree() {
        let mut parser = Parser::for_language(Language::CSharp).unwrap();
        let tree = parser.parse(b"class Foo { void Bar() {} }\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_swift_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Swift).unwrap();
        let tree = parser
            .parse(b"func greet(name: String) -> String { return name }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_kotlin_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Kotlin).unwrap();
        let tree = parser
            .parse(b"fun greet(name: String): String = name\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_scala_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Scala).unwrap();
        let tree = parser
            .parse(b"object Main { def greet(name: String): String = name }\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_zig_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Zig).unwrap();
        let tree = parser.parse(b"pub fn main() void {}\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_lua_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Lua).unwrap();
        let tree = parser
            .parse(b"function greet(name)\n  return name\nend\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_bash_source_produces_tree() {
        let mut parser = Parser::for_language(Language::Bash).unwrap();
        let tree = parser
            .parse(b"#!/bin/bash\ngreet() {\n  echo \"hello\"\n}\n")
            .unwrap();
        assert!(!tree.root_node().has_error());
    }

    // -----------------------------------------------------------------------
    // Parser::for_name — builtin resolution
    // -----------------------------------------------------------------------

    #[test]
    fn test_for_name_resolves_builtin_rust() {
        let parser = Parser::for_name("rust");
        assert!(parser.is_ok());
        assert_eq!(parser.unwrap().language(), Language::Rust);
    }

    #[test]
    fn test_for_name_resolves_builtin_python() {
        let parser = Parser::for_name("python");
        assert!(parser.is_ok());
        assert_eq!(parser.unwrap().language(), Language::Python);
    }

    #[test]
    fn test_for_name_resolves_builtin_alias() {
        let parser = Parser::for_name("ts");
        assert!(parser.is_ok());
        assert_eq!(parser.unwrap().language(), Language::TypeScript);
    }

    #[test]
    fn test_for_name_unknown_without_runtime_grammar_returns_error() {
        // This will fail because "haskell" is not a builtin and no
        // runtime grammar is installed in the test environment
        let result = Parser::for_name("haskell");
        assert!(result.is_err());
    }
}
