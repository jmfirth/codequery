//! Per-language import extraction from tree-sitter ASTs.
//!
//! Extracts import/dependency declarations from parsed source files.
//! Each language has its own extraction logic that maps language-specific
//! AST node types (e.g., `use_declaration`, `import_statement`) to a
//! unified `ImportInfo` representation.

use codequery_core::Language;
use serde::Serialize;

/// A single import/dependency declaration extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportInfo {
    /// The import source path (e.g., `crate::auth::authenticate`, `./models`).
    pub source: String,
    /// The kind of import: `"use"`, `"import"`, `"from"`, `"include"`, `"wildcard"`.
    pub kind: String,
    /// The 1-based line number of the import statement.
    pub line: usize,
    /// Whether this is an external (third-party) import.
    pub external: bool,
}

/// Extract all imports from a parsed source file.
///
/// Dispatches to the appropriate per-language import extractor based on
/// `language`. Returns an empty `Vec` for languages with no imports or
/// if no import nodes are found.
#[must_use]
pub fn extract_imports(
    source: &str,
    tree: &tree_sitter::Tree,
    language: Language,
) -> Vec<ImportInfo> {
    match language {
        Language::Rust => extract_rust_imports(source, tree),
        Language::TypeScript | Language::JavaScript => extract_ts_imports(source, tree),
        Language::Python => extract_python_imports(source, tree),
        Language::Go => extract_go_imports(source, tree),
        Language::C | Language::Cpp => extract_c_imports(source, tree),
        Language::Java => extract_java_imports(source, tree),
        Language::Ruby => extract_ruby_imports(source, tree),
        Language::Php => extract_php_imports(source, tree),
        Language::CSharp => extract_csharp_imports(source, tree),
        Language::Swift => extract_swift_imports(source, tree),
        Language::Kotlin => extract_kotlin_imports(source, tree),
        Language::Scala => extract_scala_imports(source, tree),
        Language::Zig => extract_zig_imports(source, tree),
        Language::Lua => extract_lua_imports(source, tree),
        Language::Bash => extract_bash_imports(source, tree),
        // Structured data formats have no import system
        Language::Html | Language::Css | Language::Json | Language::Yaml | Language::Toml => {
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Rust imports
// ---------------------------------------------------------------------------

/// Extract `use` declarations from Rust source.
///
/// Rust `use` statements produce `use_declaration` nodes. External imports
/// start with a crate name (not `crate`, `self`, or `super`).
fn extract_rust_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "use_declaration" {
            let text = node_text(source, &child);
            let path = text
                .trim_start_matches("pub ")
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub(super) ")
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .trim();

            let external = !path.starts_with("crate::")
                && !path.starts_with("self::")
                && !path.starts_with("super::");

            let kind = if path.ends_with("::*") {
                "wildcard"
            } else {
                "use"
            };

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: kind.to_string(),
                line: child.start_position().row + 1,
                external,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript imports
// ---------------------------------------------------------------------------

/// Extract `import` statements from TypeScript/JavaScript source.
///
/// TS/JS `import` statements produce `import_statement` nodes. Imports
/// starting with `.` or `..` are internal; all others are external.
fn extract_ts_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            let import_source = extract_ts_import_source(source, &child);
            let external = !import_source.starts_with('.') && !import_source.starts_with('/');

            imports.push(ImportInfo {
                source: import_source,
                kind: "import".to_string(),
                line: child.start_position().row + 1,
                external,
            });
        }
    }

    imports
}

/// Extract the source/path string from a TS/JS import statement node.
fn extract_ts_import_source(source: &str, node: &tree_sitter::Node<'_>) -> String {
    // Look for the `string` child which holds the module path
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            let text = node_text(source, &child);
            // Strip quotes
            return text
                .trim_start_matches(['"', '\''])
                .trim_end_matches(['"', '\''])
                .to_string();
        }
    }
    // Fallback: extract full text minus `import` keyword
    let text = node_text(source, node);
    text.trim_start_matches("import ")
        .trim_end_matches(';')
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Python imports
// ---------------------------------------------------------------------------

/// Extract `import` and `from ... import` statements from Python source.
///
/// Python has two import forms:
/// - `import_statement` for `import x` / `import x.y`
/// - `import_from_statement` for `from x import y`
///
/// Relative imports (starting with `.`) are internal; others are external.
fn extract_python_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                let text = node_text(source, &child);
                let path = text
                    .trim_start_matches("import ")
                    .trim_end_matches(';')
                    .trim();

                imports.push(ImportInfo {
                    source: path.to_string(),
                    kind: "import".to_string(),
                    line: child.start_position().row + 1,
                    external: !path.starts_with('.'),
                });
            }
            "import_from_statement" => {
                let module = extract_python_from_module(source, &child);
                let external = !module.starts_with('.');

                imports.push(ImportInfo {
                    source: module,
                    kind: "from".to_string(),
                    line: child.start_position().row + 1,
                    external,
                });
            }
            _ => {}
        }
    }

    imports
}

/// Extract the module path from a Python `from ... import` statement.
fn extract_python_from_module(source: &str, node: &tree_sitter::Node<'_>) -> String {
    // Look for `dotted_name` or `relative_import` child after `from`
    let mut cursor = node.walk();
    let mut found_from = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "from" {
            found_from = true;
            continue;
        }
        if found_from && (child.kind() == "dotted_name" || child.kind() == "relative_import") {
            return node_text(source, &child).to_string();
        }
    }
    // Fallback: parse from text
    let text = node_text(source, node);
    let stripped = text.trim_start_matches("from ");
    if let Some(import_pos) = stripped.find(" import") {
        return stripped[..import_pos].trim().to_string();
    }
    stripped.to_string()
}

// ---------------------------------------------------------------------------
// Go imports
// ---------------------------------------------------------------------------

/// Extract `import` declarations from Go source.
///
/// Go has single imports (`import "fmt"`) and grouped imports
/// (`import ( "fmt"; "os" )`). Both produce `import_declaration` nodes.
/// Standard library imports (no `.` or `/` at start) are external.
fn extract_go_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            extract_go_import_specs(source, &child, &mut imports);
        }
    }

    imports
}

/// Extract individual import specs from an import declaration.
fn extract_go_import_specs(
    source: &str,
    node: &tree_sitter::Node<'_>,
    imports: &mut Vec<ImportInfo>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                if let Some(info) = extract_go_single_import(source, &child) {
                    imports.push(info);
                }
            }
            "import_spec_list" => {
                let mut inner_cursor = child.walk();
                for spec in child.children(&mut inner_cursor) {
                    if spec.kind() == "import_spec" {
                        if let Some(info) = extract_go_single_import(source, &spec) {
                            imports.push(info);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract a single Go import spec.
fn extract_go_single_import(source: &str, node: &tree_sitter::Node<'_>) -> Option<ImportInfo> {
    // Find the `interpreted_string_literal` child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "interpreted_string_literal" {
            let text = node_text(source, &child);
            let path = text.trim_matches('"');
            // External: everything is external in Go (all packages are imported)
            // But we classify stdlib (no dots/slashes) vs third-party (has dots)
            let external = true;
            return Some(ImportInfo {
                source: path.to_string(),
                kind: "import".to_string(),
                line: node.start_position().row + 1,
                external,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// C / C++ imports
// ---------------------------------------------------------------------------

/// Extract `#include` directives from C/C++ source.
///
/// C/C++ `#include` directives produce `preproc_include` nodes.
/// System includes (`<...>`) are external; local includes (`"..."`) are internal.
fn extract_c_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "preproc_include" {
            if let Some(info) = extract_c_single_include(source, &child) {
                imports.push(info);
            }
        }
    }

    imports
}

/// Extract a single C/C++ `#include` directive.
fn extract_c_single_include(source: &str, node: &tree_sitter::Node<'_>) -> Option<ImportInfo> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "system_lib_string" => {
                let text = node_text(source, &child);
                let path = text.trim_start_matches('<').trim_end_matches('>');
                return Some(ImportInfo {
                    source: path.to_string(),
                    kind: "include".to_string(),
                    line: node.start_position().row + 1,
                    external: true,
                });
            }
            "string_literal" => {
                let text = node_text(source, &child);
                let path = text.trim_matches('"');
                return Some(ImportInfo {
                    source: path.to_string(),
                    kind: "include".to_string(),
                    line: node.start_position().row + 1,
                    external: false,
                });
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Java imports
// ---------------------------------------------------------------------------

/// Extract `import` declarations from Java source.
///
/// Java `import` statements produce `import_declaration` nodes.
/// All Java imports are external (package-based).
fn extract_java_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            let text = node_text(source, &child);
            let path = text
                .trim_start_matches("import ")
                .trim_start_matches("static ")
                .trim_end_matches(';')
                .trim();

            let kind = if path.ends_with(".*") || text.contains(".*") {
                "wildcard"
            } else if text.trim_start_matches("import ").starts_with("static ") {
                "static"
            } else {
                "import"
            };

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: kind.to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// Ruby imports
// ---------------------------------------------------------------------------

/// Extract `require` and `require_relative` calls from Ruby source.
///
/// Ruby uses `require` for external gems and `require_relative` for local files.
fn extract_ruby_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    collect_ruby_requires(root, source, &mut imports);
    imports
}

/// Recursively collect Ruby require calls.
fn collect_ruby_requires(node: tree_sitter::Node<'_>, source: &str, imports: &mut Vec<ImportInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call" || child.kind() == "command" {
            let text = node_text(source, &child);
            if text.starts_with("require_relative ") || text.starts_with("require_relative(") {
                let path = extract_ruby_string_arg(text);
                imports.push(ImportInfo {
                    source: path,
                    kind: "require_relative".to_string(),
                    line: child.start_position().row + 1,
                    external: false,
                });
            } else if text.starts_with("require ") || text.starts_with("require(") {
                let path = extract_ruby_string_arg(text);
                imports.push(ImportInfo {
                    source: path,
                    kind: "require".to_string(),
                    line: child.start_position().row + 1,
                    external: true,
                });
            }
        }
    }
}

/// Extract the string argument from a Ruby require call.
fn extract_ruby_string_arg(text: &str) -> String {
    // Try to extract quoted string
    if let Some(start) = text.find(['"', '\'']) {
        let quote = text.as_bytes()[start] as char;
        if let Some(end) = text[start + 1..].find(quote) {
            return text[start + 1..start + 1 + end].to_string();
        }
    }
    // Fallback: strip keyword
    text.trim_start_matches("require_relative")
        .trim_start_matches("require")
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// PHP imports
// ---------------------------------------------------------------------------

/// Extract `use` statements and `require`/`include` calls from PHP source.
///
/// PHP `namespace_use_declaration` nodes represent `use` statements.
fn extract_php_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "namespace_use_declaration" {
            let text = node_text(source, &child);
            let path = text.trim_start_matches("use ").trim_end_matches(';').trim();

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: "use".to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// C# imports
// ---------------------------------------------------------------------------

/// Extract `using` directives from C# source.
///
/// C# `using_directive` nodes represent `using` statements.
fn extract_csharp_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "using_directive" {
            let text = node_text(source, &child);
            let path = text
                .trim_start_matches("using ")
                .trim_start_matches("static ")
                .trim_end_matches(';')
                .trim();

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: "using".to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// Swift imports
// ---------------------------------------------------------------------------

/// Extract `import` declarations from Swift source.
///
/// Swift `import` statements produce `import_declaration` nodes.
/// All Swift imports are external (framework/module based).
fn extract_swift_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            let text = node_text(source, &child);
            let path = text.trim_start_matches("import ").trim();

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: "import".to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// Kotlin imports
// ---------------------------------------------------------------------------

/// Extract `import` declarations from Kotlin source.
///
/// Kotlin `import_header` / `import_list` nodes contain `import` children.
/// All Kotlin imports are external (package based).
fn extract_kotlin_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_list" {
            let mut inner_cursor = child.walk();
            for import_child in child.children(&mut inner_cursor) {
                if import_child.kind() == "import_header" {
                    let text = node_text(source, &import_child);
                    let path = text.trim_start_matches("import ").trim_end().trim();

                    imports.push(ImportInfo {
                        source: path.to_string(),
                        kind: "import".to_string(),
                        line: import_child.start_position().row + 1,
                        external: true,
                    });
                }
            }
        }
        // Some grammars also produce standalone import_header nodes
        if child.kind() == "import_header" {
            let text = node_text(source, &child);
            let path = text.trim_start_matches("import ").trim_end().trim();

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: "import".to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// Scala imports
// ---------------------------------------------------------------------------

/// Extract `import` declarations from Scala source.
///
/// Scala `import_declaration` nodes contain import paths.
/// All Scala imports are external (package based).
fn extract_scala_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            let text = node_text(source, &child);
            let path = text.trim_start_matches("import ").trim();

            let kind = if path.ends_with("._") {
                "wildcard"
            } else {
                "import"
            };

            imports.push(ImportInfo {
                source: path.to_string(),
                kind: kind.to_string(),
                line: child.start_position().row + 1,
                external: true,
            });
        }
    }

    imports
}

// ---------------------------------------------------------------------------
// Zig imports
// ---------------------------------------------------------------------------

/// Extract `@import` calls from Zig source.
///
/// Zig imports use `@import("module")` inside `const` declarations:
/// `const std = @import("std");`
/// External imports are string literals that don't start with `.` or `./`.
fn extract_zig_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "variable_declaration" {
            if let Some(info) = extract_zig_single_import(source, &child) {
                imports.push(info);
            }
        }
    }

    imports
}

/// Extract a single Zig `@import` from a variable declaration node.
fn extract_zig_single_import(source: &str, node: &tree_sitter::Node<'_>) -> Option<ImportInfo> {
    // Look for builtin_function child with @import
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "builtin_function" {
            let text = node_text(source, &child);
            if text.starts_with("@import") {
                // Extract the string argument
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "arguments" {
                        let mut arg_cursor = inner.walk();
                        for arg in inner.children(&mut arg_cursor) {
                            if arg.kind() == "string" {
                                let path = node_text(source, &arg).trim_matches('"').to_string();
                                let external = !path.starts_with('.')
                                    && !path.starts_with("./")
                                    && !path.starts_with("../");
                                return Some(ImportInfo {
                                    source: path,
                                    kind: "import".to_string(),
                                    line: node.start_position().row + 1,
                                    external,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Lua imports
// ---------------------------------------------------------------------------

/// Extract `require` calls from Lua source.
///
/// Lua imports use `require("module")` typically in local variable declarations:
/// `local M = require("module")`
/// All Lua requires are treated as external.
fn extract_lua_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        extract_lua_require_from_node(source, &child, &mut imports);
    }

    imports
}

/// Recursively look for `require` function calls in a node.
fn extract_lua_require_from_node(
    source: &str,
    node: &tree_sitter::Node<'_>,
    imports: &mut Vec<ImportInfo>,
) {
    if node.kind() == "function_call" {
        let text = node_text(source, node);
        if text.starts_with("require") {
            // Extract the string argument
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "arguments" {
                    let mut arg_cursor = child.walk();
                    for arg in child.children(&mut arg_cursor) {
                        if arg.kind() == "string" {
                            let path = node_text(source, &arg);
                            let path = path
                                .trim_start_matches(['"', '\''])
                                .trim_end_matches(['"', '\''])
                                .to_string();
                            imports.push(ImportInfo {
                                source: path,
                                kind: "require".to_string(),
                                line: node.start_position().row + 1,
                                external: true,
                            });
                            return;
                        }
                    }
                }
            }
        }
    }

    // Recurse into children to find nested require calls
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_lua_require_from_node(source, &child, imports);
    }
}

// ---------------------------------------------------------------------------
// Bash imports
// ---------------------------------------------------------------------------

/// Extract `source` and `.` commands from Bash source.
///
/// Bash sources other scripts via `source path` or `. path`.
/// All sourced files are treated as internal (local).
fn extract_bash_imports(source: &str, tree: &tree_sitter::Tree) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "command" {
            if let Some(info) = extract_bash_source_command(source, &child) {
                imports.push(info);
            }
        }
    }

    imports
}

/// Extract a single Bash `source` or `.` command.
fn extract_bash_source_command(source: &str, node: &tree_sitter::Node<'_>) -> Option<ImportInfo> {
    let mut cursor = node.walk();
    let mut is_source = false;
    let mut path = None;

    for child in node.children(&mut cursor) {
        if child.kind() == "command_name" {
            let text = node_text(source, &child).trim().to_string();
            if text == "source" || text == "." {
                is_source = true;
            }
        } else if is_source && child.kind() == "word" {
            path = Some(node_text(source, &child).to_string());
        }
    }

    if is_source {
        if let Some(p) = path {
            return Some(ImportInfo {
                source: p,
                kind: "source".to_string(),
                line: node.start_position().row + 1,
                external: false,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the text of a tree-sitter node from source.
fn node_text<'a>(source: &'a str, node: &tree_sitter::Node<'_>) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    // -----------------------------------------------------------------------
    // Rust imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_rust_use_declaration_extracted() {
        let source = "use std::collections::HashMap;\nuse crate::models::User;\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "std::collections::HashMap");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "use");
        assert_eq!(imports[0].line, 1);

        assert_eq!(imports[1].source, "crate::models::User");
        assert!(!imports[1].external);
        assert_eq!(imports[1].kind, "use");
        assert_eq!(imports[1].line, 2);
    }

    #[test]
    fn test_imports_rust_wildcard_use() {
        let source = "use std::io::*;\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].kind, "wildcard");
        assert_eq!(imports[0].source, "std::io::*");
        assert!(imports[0].external);
    }

    #[test]
    fn test_imports_rust_pub_use() {
        let source = "pub use crate::models::User;\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "crate::models::User");
        assert!(!imports[0].external);
    }

    #[test]
    fn test_imports_rust_self_and_super_are_internal() {
        let source = "use self::helper;\nuse super::parent_fn;\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert_eq!(imports.len(), 2);
        assert!(!imports[0].external);
        assert!(!imports[1].external);
    }

    #[test]
    fn test_imports_rust_empty_file() {
        let source = "fn main() {}\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert!(imports.is_empty());
    }

    // -----------------------------------------------------------------------
    // TypeScript imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_typescript_import_statement_extracted() {
        let source = "import { User } from \"./models\";\nimport express from \"express\";\n";
        let mut parser = Parser::for_language(Language::TypeScript).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::TypeScript);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "./models");
        assert!(!imports[0].external);
        assert_eq!(imports[0].kind, "import");
        assert_eq!(imports[0].line, 1);

        assert_eq!(imports[1].source, "express");
        assert!(imports[1].external);
        assert_eq!(imports[1].line, 2);
    }

    #[test]
    fn test_imports_javascript_import_extracted() {
        let source = "import { readFile } from \"fs\";\n";
        let mut parser = Parser::for_language(Language::JavaScript).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::JavaScript);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "fs");
        assert!(imports[0].external);
    }

    // -----------------------------------------------------------------------
    // Python imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_python_import_statement_extracted() {
        let source = "import os\nimport sys\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Python);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "os");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "import");
        assert_eq!(imports[0].line, 1);

        assert_eq!(imports[1].source, "sys");
        assert!(imports[1].external);
    }

    #[test]
    fn test_imports_python_from_import_extracted() {
        let source = "from os.path import join\nfrom .models import User\n";
        let mut parser = Parser::for_language(Language::Python).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Python);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "os.path");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "from");

        assert_eq!(imports[1].source, ".models");
        assert!(!imports[1].external);
        assert_eq!(imports[1].kind, "from");
    }

    // -----------------------------------------------------------------------
    // Go imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_go_single_import_extracted() {
        let source = "package main\n\nimport \"fmt\"\n";
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Go);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "fmt");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "import");
    }

    #[test]
    fn test_imports_go_grouped_imports_extracted() {
        let source = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n";
        let mut parser = Parser::for_language(Language::Go).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Go);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "fmt");
        assert_eq!(imports[1].source, "os");
    }

    // -----------------------------------------------------------------------
    // C / C++ imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_c_system_include_extracted() {
        let source = "#include <stdio.h>\n";
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::C);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "stdio.h");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "include");
    }

    #[test]
    fn test_imports_c_local_include_extracted() {
        let source = "#include \"utils.h\"\n";
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::C);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "utils.h");
        assert!(!imports[0].external);
        assert_eq!(imports[0].kind, "include");
    }

    #[test]
    fn test_imports_cpp_includes_extracted() {
        let source = "#include <iostream>\n#include \"models.hpp\"\n";
        let mut parser = Parser::for_language(Language::Cpp).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Cpp);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "iostream");
        assert!(imports[0].external);
        assert_eq!(imports[1].source, "models.hpp");
        assert!(!imports[1].external);
    }

    // -----------------------------------------------------------------------
    // Java imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_java_import_declaration_extracted() {
        let source = "import com.example.models.User;\nimport java.util.List;\n";
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Java);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "com.example.models.User");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "import");
        assert_eq!(imports[0].line, 1);

        assert_eq!(imports[1].source, "java.util.List");
        assert!(imports[1].external);
    }

    #[test]
    fn test_imports_java_wildcard_import() {
        let source = "import java.util.*;\n";
        let mut parser = Parser::for_language(Language::Java).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Java);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].kind, "wildcard");
        assert_eq!(imports[0].source, "java.util.*");
    }

    // -----------------------------------------------------------------------
    // Cross-cutting: external vs internal classification
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_external_vs_internal_rust() {
        let source = "use serde::Serialize;\nuse crate::models::User;\nuse self::helper;\nuse super::parent;\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);

        assert_eq!(imports.len(), 4);
        assert!(imports[0].external); // serde
        assert!(!imports[1].external); // crate::
        assert!(!imports[2].external); // self::
        assert!(!imports[3].external); // super::
    }

    #[test]
    fn test_imports_external_vs_internal_c() {
        let source = "#include <stdio.h>\n#include \"mylib.h\"\n";
        let mut parser = Parser::for_language(Language::C).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::C);

        assert_eq!(imports.len(), 2);
        assert!(imports[0].external); // <stdio.h> system
        assert!(!imports[1].external); // "mylib.h" local
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_empty_source_returns_empty() {
        let source = "";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_imports_no_imports_returns_empty() {
        let source = "fn main() { println!(\"hello\"); }\n";
        let mut parser = Parser::for_language(Language::Rust).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Rust);
        assert!(imports.is_empty());
    }

    // -----------------------------------------------------------------------
    // Zig imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_zig_import_extracted() {
        let source = "const std = @import(\"std\");\nconst utils = @import(\"./utils.zig\");\n";
        let mut parser = Parser::for_language(Language::Zig).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Zig);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "std");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "import");
        assert_eq!(imports[0].line, 1);

        assert_eq!(imports[1].source, "./utils.zig");
        assert!(!imports[1].external);
    }

    #[test]
    fn test_imports_zig_no_imports_returns_empty() {
        let source = "pub fn main() void {}\n";
        let mut parser = Parser::for_language(Language::Zig).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Zig);
        assert!(imports.is_empty());
    }

    // -----------------------------------------------------------------------
    // Lua imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_lua_require_extracted() {
        let source = "local M = require(\"main\")\n";
        let mut parser = Parser::for_language(Language::Lua).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Lua);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "main");
        assert!(imports[0].external);
        assert_eq!(imports[0].kind, "require");
    }

    #[test]
    fn test_imports_lua_no_requires_returns_empty() {
        let source = "function foo()\n  return 1\nend\n";
        let mut parser = Parser::for_language(Language::Lua).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Lua);
        assert!(imports.is_empty());
    }

    // -----------------------------------------------------------------------
    // Bash imports
    // -----------------------------------------------------------------------

    #[test]
    fn test_imports_bash_source_command_extracted() {
        let source = "#!/bin/bash\nsource ./utils.sh\n. ./helpers.sh\n";
        let mut parser = Parser::for_language(Language::Bash).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Bash);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "./utils.sh");
        assert!(!imports[0].external);
        assert_eq!(imports[0].kind, "source");
        assert_eq!(imports[0].line, 2);

        assert_eq!(imports[1].source, "./helpers.sh");
        assert!(!imports[1].external);
        assert_eq!(imports[1].kind, "source");
        assert_eq!(imports[1].line, 3);
    }

    #[test]
    fn test_imports_bash_no_sources_returns_empty() {
        let source = "#!/bin/bash\necho hello\n";
        let mut parser = Parser::for_language(Language::Bash).unwrap();
        let tree = parser.parse(source.as_bytes()).unwrap();
        let imports = extract_imports(source, &tree, Language::Bash);
        assert!(imports.is_empty());
    }
}
