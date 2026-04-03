//! Language-aware symbol extraction dispatch.
//!
//! Routes extraction requests to the appropriate per-language extractor
//! based on the `Language` parameter. Languages without extractors return
//! empty results until their modules are implemented.

use std::path::Path;

use codequery_core::{Language, Symbol};

use crate::extract_engine::{extract_with_config, CompiledExtractor};
use crate::languages::bash::BashExtractor;
use crate::languages::c::CExtractor;
use crate::languages::cpp::CppExtractor;
use crate::languages::csharp::CSharpExtractor;
use crate::languages::css::CssExtractor;
use crate::languages::go::GoExtractor;
use crate::languages::html::HtmlExtractor;
use crate::languages::java::JavaExtractor;
use crate::languages::json::JsonExtractor;
use crate::languages::kotlin::KotlinExtractor;
use crate::languages::lua::LuaExtractor;
use crate::languages::php::PhpExtractor;
use crate::languages::python::PythonExtractor;
use crate::languages::ruby::RubyExtractor;
use crate::languages::rust::RustExtractor;
use crate::languages::scala::ScalaExtractor;
use crate::languages::swift::SwiftExtractor;
use crate::languages::toml::TomlExtractor;
use crate::languages::typescript::TypeScriptExtractor;
use crate::languages::yaml::YamlExtractor;
use crate::languages::zig::ZigExtractor;
use crate::languages::LanguageExtractor;

/// Extract symbols using the appropriate language extractor.
///
/// Dispatches to the per-language extractor based on `language`. For languages
/// whose extractors have not yet been implemented, returns an empty `Vec`.
///
/// # Arguments
/// * `source` — the source text (needed to extract node text via byte ranges)
/// * `tree` — the parsed tree-sitter tree
/// * `file` — the file path (stored in each `Symbol` for output)
/// * `language` — the source language, used to select the extractor
#[must_use]
pub fn extract_symbols(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &Path,
    language: Language,
) -> Vec<Symbol> {
    match language {
        Language::Rust => RustExtractor::extract_symbols(source, tree, file),
        Language::Python => PythonExtractor::extract_symbols(source, tree, file),
        Language::Go => GoExtractor::extract_symbols(source, tree, file),
        Language::Java => JavaExtractor::extract_symbols(source, tree, file),
        Language::TypeScript | Language::JavaScript => {
            TypeScriptExtractor::extract_symbols(source, tree, file)
        }
        Language::C => CExtractor::extract_symbols(source, tree, file),
        Language::Cpp => CppExtractor::extract_symbols(source, tree, file),
        Language::Ruby => RubyExtractor::extract_symbols(source, tree, file),
        Language::Php => PhpExtractor::extract_symbols(source, tree, file),
        Language::CSharp => CSharpExtractor::extract_symbols(source, tree, file),
        Language::Swift => SwiftExtractor::extract_symbols(source, tree, file),
        Language::Kotlin => KotlinExtractor::extract_symbols(source, tree, file),
        Language::Scala => ScalaExtractor::extract_symbols(source, tree, file),
        Language::Zig => ZigExtractor::extract_symbols(source, tree, file),
        Language::Lua => LuaExtractor::extract_symbols(source, tree, file),
        Language::Bash => BashExtractor::extract_symbols(source, tree, file),
        Language::Html => HtmlExtractor::extract_symbols(source, tree, file),
        Language::Css => CssExtractor::extract_symbols(source, tree, file),
        Language::Json => JsonExtractor::extract_symbols(source, tree, file),
        Language::Yaml => YamlExtractor::extract_symbols(source, tree, file),
        Language::Toml => TomlExtractor::extract_symbols(source, tree, file),
    }
}

/// Extract symbols from a file identified by language name string.
///
/// For builtin languages (those with a `Language` enum variant), delegates to
/// [`extract_symbols`] with the compiled-in extractor. For runtime languages
/// (installed via `cq grammar install`), loads the `extract.toml` config and
/// uses the declarative extraction engine.
///
/// Returns an empty `Vec` if the language has no extraction rules available.
#[must_use]
pub fn extract_symbols_by_name(
    source: &str,
    tree: &tree_sitter::Tree,
    file: &std::path::Path,
    lang_name: &str,
) -> Vec<Symbol> {
    // Fast path: builtin language with compiled-in extractor
    if let Some(lang) = Language::from_name(lang_name) {
        return extract_symbols(source, tree, file, lang);
    }

    // Check for cached compiled extractor (avoids re-loading grammar)
    if let Some(compiled) = CompiledExtractor::get_cached(lang_name) {
        return compiled.extract(source, tree, file);
    }

    // Cache miss: load extract.toml + grammar, compile rules, cache
    let Some(config) = load_runtime_extract_config(lang_name) else {
        return Vec::new();
    };

    let Ok(ts_lang) = crate::parser::grammar_for_name(lang_name) else {
        return Vec::new();
    };

    extract_with_config(&config, source, tree, file, &ts_lang)
}

/// Load `extract.toml` from an installed grammar package.
///
/// Looks in `~/.local/share/cq/languages/<name>/extract.toml`. Returns `None`
/// if the package is not installed or has no extraction config.
fn load_runtime_extract_config(
    lang_name: &str,
) -> Option<codequery_core::extract_config::ExtractConfig> {
    let dir = codequery_core::dirs::languages_dir()?;
    let config_path = dir.join(lang_name).join("extract.toml");
    let config_str = std::fs::read_to_string(&config_path).ok()?;
    codequery_core::load_extract_config(&config_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RustParser;

    #[test]
    fn test_extract_symbols_rust_still_works() {
        let mut parser = RustParser::new().unwrap();
        let tree = parser.parse(b"fn main() {}").unwrap();
        let symbols = extract_symbols("fn main() {}", &tree, Path::new("main.rs"), Language::Rust);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
    }

    #[test]
    fn test_extract_symbols_python_extracts_function() {
        let mut parser = crate::Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(b"def foo():\n    pass\n").unwrap();
        let symbols = extract_symbols(
            "def foo():\n    pass\n",
            &tree,
            Path::new("foo.py"),
            Language::Python,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
    }

    #[test]
    fn test_extract_symbols_typescript_dispatches_correctly() {
        let mut parser = crate::Parser::for_language(Language::TypeScript).unwrap();
        let tree = parser.parse(b"function foo(): void {}").unwrap();
        let symbols = extract_symbols(
            "function foo(): void {}",
            &tree,
            Path::new("foo.ts"),
            Language::TypeScript,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
    }

    #[test]
    fn test_extract_symbols_javascript_dispatches_correctly() {
        let mut parser = crate::Parser::for_language(Language::JavaScript).unwrap();
        let tree = parser.parse(b"function foo() {}").unwrap();
        let symbols = extract_symbols(
            "function foo() {}",
            &tree,
            Path::new("foo.js"),
            Language::JavaScript,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
    }

    #[test]
    fn test_extract_symbols_go_dispatches_correctly() {
        let mut parser = crate::Parser::for_language(Language::Go).unwrap();
        let tree = parser.parse(b"package main\nfunc foo() {}\n").unwrap();
        let symbols = extract_symbols(
            "package main\nfunc foo() {}\n",
            &tree,
            Path::new("foo.go"),
            Language::Go,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
    }

    #[test]
    fn test_extract_symbols_c_dispatches_correctly() {
        let mut parser = crate::Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(b"int main() { return 0; }").unwrap();
        let symbols = extract_symbols(
            "int main() { return 0; }",
            &tree,
            Path::new("main.c"),
            Language::C,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
    }

    #[test]
    fn test_extract_symbols_cpp_dispatches_correctly() {
        let mut parser = crate::Parser::for_language(Language::Cpp).unwrap();
        let tree = parser.parse(b"class Foo {};").unwrap();
        let symbols = extract_symbols("class Foo {};", &tree, Path::new("foo.cpp"), Language::Cpp);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
    }

    #[test]
    fn test_extract_symbols_java_extracts_class() {
        let mut parser = crate::Parser::for_language(Language::Java).unwrap();
        let tree = parser.parse(b"public class Foo {}").unwrap();
        let symbols = extract_symbols(
            "public class Foo {}",
            &tree,
            Path::new("Foo.java"),
            Language::Java,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Foo");
    }

    // Tier-2 language tests skip gracefully when the grammar is not installed.
    // These languages are not compiled into the binary with the `common` feature.

    macro_rules! tier2_extract_test {
        ($name:ident, $lang:expr, $source:expr, $file:expr, $expected_name:expr) => {
            #[test]
            fn $name() {
                let Ok(mut parser) = crate::Parser::for_language($lang) else {
                    eprintln!("skipping: {:?} grammar not installed", $lang);
                    return;
                };
                let tree = parser.parse($source.as_bytes()).unwrap();
                let symbols = extract_symbols($source, &tree, Path::new($file), $lang);
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, $expected_name);
            }
        };
    }

    // Swift WASM grammar causes SIGBUS — ignore until grammar is fixed.
    #[test]
    #[ignore]
    fn test_extract_symbols_swift_dispatches_correctly() {
        let Ok(mut parser) = crate::Parser::for_language(Language::Swift) else {
            eprintln!("skipping: Swift grammar not installed");
            return;
        };
        let tree = parser.parse("func greet() {}".as_bytes()).unwrap();
        let symbols = extract_symbols(
            "func greet() {}",
            &tree,
            Path::new("test.swift"),
            Language::Swift,
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "greet");
    }
    tier2_extract_test!(
        test_extract_symbols_kotlin_dispatches_correctly,
        Language::Kotlin,
        "fun greet() {}",
        "test.kt",
        "greet"
    );
    tier2_extract_test!(
        test_extract_symbols_scala_dispatches_correctly,
        Language::Scala,
        "class Foo {}",
        "test.scala",
        "Foo"
    );
    tier2_extract_test!(
        test_extract_symbols_zig_dispatches_correctly,
        Language::Zig,
        "pub fn add() void {}",
        "add.zig",
        "add"
    );
    tier2_extract_test!(
        test_extract_symbols_lua_dispatches_correctly,
        Language::Lua,
        "function greet()\n  return 1\nend\n",
        "greet.lua",
        "greet"
    );
    tier2_extract_test!(
        test_extract_symbols_bash_dispatches_correctly,
        Language::Bash,
        "greet() {\n  echo hello\n}\n",
        "greet.sh",
        "greet"
    );
}
