//! Language-aware symbol extraction dispatch.
//!
//! Routes extraction requests to the appropriate per-language extractor
//! based on the `Language` parameter. Languages without extractors return
//! empty results until their modules are implemented.

use std::path::Path;

use codequery_core::{Language, Symbol};

use crate::languages::c::CExtractor;
use crate::languages::cpp::CppExtractor;
use crate::languages::csharp::CSharpExtractor;
use crate::languages::go::GoExtractor;
use crate::languages::java::JavaExtractor;
use crate::languages::php::PhpExtractor;
use crate::languages::python::PythonExtractor;
use crate::languages::ruby::RubyExtractor;
use crate::languages::rust::RustExtractor;
use crate::languages::typescript::TypeScriptExtractor;
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
        // Tier 2 languages — stub extractors returning empty results
        Language::Swift
        | Language::Kotlin
        | Language::Scala
        | Language::Zig
        | Language::Lua
        | Language::Bash => Vec::new(),
    }
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
}
