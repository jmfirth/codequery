//! Strict stack graph resolution tests.
//!
//! These tests enforce exact expectations per language:
//! - Python, TypeScript, JavaScript, Java, Rust, Go, C: MUST produce `Resolution::Resolved`
//!   for same-file references (TSG scope wiring enables path stitching)
//! - Exact reference counts where deterministic
//! - Cross-file resolution with specific file paths

use std::path::PathBuf;

use codequery_core::Language;
use codequery_index::FileSymbols;
use codequery_parse::Parser;
use codequery_resolve::{Resolution, StackGraphResolver};

/// Create a `FileSymbols` from source text, path, and language.
fn make_file_symbols(path: &str, source: &str, lang: Language) -> FileSymbols {
    let mut parser = Parser::for_language(lang).unwrap();
    let tree = parser.parse(source.as_bytes()).unwrap();
    let file = PathBuf::from(path);
    let symbols = codequery_parse::extract_symbols(source, &tree, &file, lang);
    FileSymbols {
        file,
        symbols,
        source: source.to_string(),
        tree,
    }
}

// ===========================================================================
// Python: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn python_same_file_resolved() {
    let source = "def greet(name):\n    return f'Hello, {name}!'\n\ngreet('world')\n";
    let fs = make_file_symbols("app.py", source, Language::Python);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Python same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Python same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn python_cross_file_resolved() {
    let src_utils = "def format_name(first, last):\n    return f'{first} {last}'\n";
    let src_services = "from utils import format_name\nresult = format_name('A', 'B')\n";

    let fs_utils = make_file_symbols("utils.py", src_utils, Language::Python);
    let fs_services = make_file_symbols("services.py", src_services, Language::Python);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_utils, fs_services], "format_name");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Python cross-file: expected >= 1 reference for 'format_name', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved (not Syntactic).
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Python cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // At least one reference must be in services.py pointing to utils.py definition.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("services"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("utils"))
    });
    assert!(
        cross_file_ref.is_some(),
        "Python cross-file: expected at least one reference from services.py -> utils.py, \
         got refs: {:?}",
        refs
    );
}

// ===========================================================================
// TypeScript: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn typescript_same_file_resolved() {
    let source =
        "function greet(name: string): string { return `Hello ${name}`; }\nconst r = greet('world');\n";
    let fs = make_file_symbols("app.ts", source, Language::TypeScript);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "TypeScript same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "TypeScript same-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn typescript_cross_file_resolved() {
    let src_models = "export interface User { name: string; }\n";
    let src_services = "import { User } from './models';\nconst u: User = { name: 'test' };\n";

    let fs_models = make_file_symbols("models.ts", src_models, Language::TypeScript);
    let fs_services = make_file_symbols("services.ts", src_services, Language::TypeScript);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_models, fs_services], "User");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "TypeScript cross-file: expected >= 1 reference for 'User', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "TypeScript cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // At least one ref should be in services.ts.
    let in_services = refs
        .iter()
        .any(|r| r.ref_file.to_str().is_some_and(|s| s.contains("services")));
    assert!(
        in_services,
        "TypeScript cross-file: expected reference in services.ts, got refs: {:?}",
        refs
    );
}

// ===========================================================================
// JavaScript: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn javascript_same_file_resolved() {
    let source = "function greet(name) { return 'Hello ' + name; }\nconst r = greet('world');\n";
    let fs = make_file_symbols("app.js", source, Language::JavaScript);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "JavaScript same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "JavaScript same-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// Java: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn java_same_file_resolved() {
    let source = "public class Main {\n  static void greet() {}\n  public static void main(String[] a) { greet(); }\n}\n";
    let fs = make_file_symbols("Main.java", source, Language::Java);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Java same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Java same-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// Rust: MUST produce Resolution::Resolved (TSG scope wiring fixed)
// ===========================================================================

#[test]
fn rust_same_file_resolved() {
    let source = "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n";
    let fs = make_file_symbols("main.rs", source, Language::Rust);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Rust same-file: expected >= 1 resolved reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Rust same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// Rust: cross-file use crate:: resolution
// ===========================================================================

#[test]
fn rust_cross_file_use_crate_resolved() {
    // services.rs uses `crate::models::User` from models.rs.
    // The stack graph must resolve `User` in services.rs to the definition
    // in models.rs (not fall back to syntactic).
    //
    // lib.rs declares `pub mod models;` and `pub mod services;`, matching
    // the fixture project layout.

    let src_lib = concat!("pub mod models;\n", "pub mod services;\n",);
    let src_models = concat!("pub struct User {\n", "    pub name: String,\n", "}\n",);
    let src_services = concat!(
        "use crate::models::User;\n",
        "\n",
        "pub fn create_user(name: &str) -> User {\n",
        "    User { name: name.to_string() }\n",
        "}\n",
    );

    let fs_lib = make_file_symbols("src/lib.rs", src_lib, Language::Rust);
    let fs_models = make_file_symbols("src/models.rs", src_models, Language::Rust);
    let fs_services = make_file_symbols("src/services.rs", src_services, Language::Rust);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_lib, fs_models, fs_services], "User");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Rust cross-file: expected >= 1 reference for 'User', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved (not Syntactic).
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Rust cross-file: reference MUST be Resolved, got {:?}. Warnings: {:?}",
            r.resolution,
            result.warnings
        );
    }

    // At least one reference must be in services.rs pointing to models.rs definition.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("services"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("models"))
    });
    assert!(
        cross_file_ref.is_some(),
        "Rust cross-file: expected at least one reference from services.rs -> models.rs, \
         got refs: {:?}",
        refs
    );
}

// ===========================================================================
// Go: MUST produce Resolution::Resolved (TSG scope wiring fixed)
// ===========================================================================

#[test]
fn go_same_file_resolved() {
    let source =
        "package main\n\nfunc greet() string { return \"hello\" }\n\nfunc main() { greet() }\n";
    let fs = make_file_symbols("main.go", source, Language::Go);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Go same-file: expected >= 1 resolved reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Go same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// Go: Cross-file same-package resolution
// ===========================================================================

#[test]
fn go_cross_file_same_package_resolved() {
    // Two files in package main: main.go defines Greet, test.go calls it.
    // In Go, same-package functions are called without qualification,
    // so cross-file resolution requires a direct ROOT_NODE -> file.defs edge.
    let src_main = concat!(
        "package main\n",
        "\n",
        "// Greet returns a greeting.\n",
        "func Greet(name string) string {\n",
        "\treturn \"Hello, \" + name\n",
        "}\n",
    );
    let src_test = concat!(
        "package main\n",
        "\n",
        "func TestGreet() {\n",
        "\tgot := Greet(\"World\")\n",
        "\t_ = got\n",
        "}\n",
    );

    let fs_main = make_file_symbols("main.go", src_main, Language::Go);
    let fs_test = make_file_symbols("main_test.go", src_test, Language::Go);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_main, fs_test], "Greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Go cross-file: expected >= 1 reference for 'Greet', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved (not Syntactic).
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Go cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // At least one reference must be in main_test.go pointing to main.go definition.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("test"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("main.go"))
    });
    assert!(
        cross_file_ref.is_some(),
        "Go cross-file: expected at least one reference from main_test.go -> main.go, \
         got refs: {:?}",
        refs
    );
}

#[test]
fn go_cross_file_multiple_files_same_package_resolved() {
    // Three files in package main: main.go defines Greet, utils.go defines FormatName,
    // consumer.go calls both. All cross-file references should resolve.
    let src_main = "package main\n\nfunc Greet() string { return \"hello\" }\n";
    let src_utils = "package main\n\nfunc FormatName(s string) string { return s }\n";
    let src_consumer = concat!(
        "package main\n",
        "\n",
        "func run() {\n",
        "\tGreet()\n",
        "\tFormatName(\"test\")\n",
        "}\n",
    );

    // Check Greet resolution (build fresh FileSymbols per resolve call)
    {
        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(
            &[
                make_file_symbols("main.go", src_main, Language::Go),
                make_file_symbols("utils.go", src_utils, Language::Go),
                make_file_symbols("consumer.go", src_consumer, Language::Go),
            ],
            "Greet",
        );
        let refs = &result.references;
        assert!(
            !refs.is_empty(),
            "Go multi-file: expected >= 1 reference for 'Greet', got 0. Warnings: {:?}",
            result.warnings
        );
        for r in refs {
            assert_eq!(
                r.resolution,
                Resolution::Resolved,
                "Go multi-file: Greet reference MUST be Resolved, got {:?}",
                r.resolution
            );
        }
    }

    // Check FormatName resolution
    {
        let mut resolver = StackGraphResolver::new();
        let result = resolver.resolve_refs(
            &[
                make_file_symbols("main.go", src_main, Language::Go),
                make_file_symbols("utils.go", src_utils, Language::Go),
                make_file_symbols("consumer.go", src_consumer, Language::Go),
            ],
            "FormatName",
        );
        let refs = &result.references;
        assert!(
            !refs.is_empty(),
            "Go multi-file: expected >= 1 reference for 'FormatName', got 0. Warnings: {:?}",
            result.warnings
        );
        for r in refs {
            assert_eq!(
                r.resolution,
                Resolution::Resolved,
                "Go multi-file: FormatName reference MUST be Resolved, got {:?}",
                r.resolution
            );
        }
    }
}

// ===========================================================================
// C: MUST produce Resolution::Resolved (TSG scope wiring fixed)
// ===========================================================================

#[test]
fn c_same_file_resolved() {
    let source = "void greet() {}\nint main() { greet(); return 0; }\n";
    let fs = make_file_symbols("main.c", source, Language::C);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C same-file: expected >= 1 resolved reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// C: regression -- files with comments, include guards, literals must not fail
// ===========================================================================

#[test]
fn c_file_with_comments_and_ifdef_resolved() {
    // Real-world C files have comments, include guards, and preprocessor
    // directives. Previously, a wildcard `(_)` in child-wiring stanzas
    // matched comment extras and identifier nodes inside preproc_ifdef,
    // causing "Undefined scoped variable" errors that prevented resolution.
    let source = concat!(
        "/* Multi-line\n",
        "   comment */\n",
        "#ifndef MAIN_H\n",
        "#define MAIN_H\n",
        "\n",
        "// Single-line comment\n",
        "#include <stdio.h>\n",
        "\n",
        "void greet() {\n",
        "    // comment inside function body\n",
        "    int x = 42;\n",
        "}\n",
        "\n",
        "int main() {\n",
        "    greet(); /* inline comment */\n",
        "    return 0;\n",
        "}\n",
        "\n",
        "#endif\n",
    );
    let fs = make_file_symbols("main.c", source, Language::C);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C with comments/ifdef: expected >= 1 reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C with comments/ifdef: references MUST be Resolved, got {:?}. \
             This likely means the wildcard fix regressed.",
            r.resolution
        );
    }
}

// ===========================================================================
// Rust: regression -- files with comments, attributes, macros must not fail
// ===========================================================================

#[test]
fn rust_file_with_comments_and_attributes_resolved() {
    // Real-world Rust files start with doc comments, attributes, and macro
    // invocations. Previously, a wildcard `(_)` in the source_file children
    // stanza matched these unhandled node types and caused "Undefined scoped
    // variable" errors, preventing ALL resolution in that file.
    let source = concat!(
        "//! Module-level doc comment\n",
        "//! Another doc line\n",
        "\n",
        "// Regular line comment\n",
        "#![allow(unused)]\n",
        "#[derive(Debug)]\n",
        "struct Config { value: i32 }\n",
        "\n",
        "fn greet() -> String { String::from(\"hello\") }\n",
        "fn main() { greet(); }\n",
    );
    let fs = make_file_symbols("main.rs", source, Language::Rust);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Rust with comments/attrs: expected >= 1 reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Rust with comments/attrs: references MUST be Resolved, got {:?}. \
             This likely means the wildcard fix regressed.",
            r.resolution
        );
    }
}

#[test]
fn rust_impl_block_with_generic_type_resolved() {
    // impl blocks with generic types (e.g. `impl<T> Foo<T>`) should not fail
    // even though generic_type doesn't have .ref
    let source = concat!(
        "struct Wrapper<T> { inner: T }\n",
        "impl<T> Wrapper<T> {\n",
        "    fn get(&self) -> &T { &self.inner }\n",
        "}\n",
        "fn main() {\n",
        "    let w = Wrapper { inner: 42 };\n",
        "}\n",
    );
    let fs = make_file_symbols("main.rs", source, Language::Rust);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "Wrapper");

    // We just need it to not error out. Any references found are a bonus.
    // The key assertion is that warnings don't contain "Undefined scoped variable".
    for w in &result.warnings {
        let msg = format!("{w:?}");
        assert!(
            !msg.contains("Undefined scoped variable"),
            "Rust generic impl: should not produce 'Undefined scoped variable' errors, \
             got warning: {msg}"
        );
    }
}

// ===========================================================================
// Completeness: exact reference count
// ===========================================================================

#[test]
fn python_exact_three_call_sites() {
    let source = "def foo():\n    pass\n\nfoo()\nfoo()\nfoo()\n";
    let fs = make_file_symbols("app.py", source, Language::Python);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "foo");

    let refs = &result.references;
    assert_eq!(
        refs.len(),
        3,
        "Python completeness: expected EXACTLY 3 resolved references for 'foo', got {}. \
         Refs: {:?}. Warnings: {:?}",
        refs.len(),
        refs,
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "foo");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Python completeness: all 3 references MUST be Resolved"
        );
    }
}

// ===========================================================================
// A/B test: resolved refs have def_file, syntactic do not
// ===========================================================================

#[test]
fn python_resolved_refs_have_def_file() {
    let src_utils = "def format_name(first, last):\n    return f'{first} {last}'\n";
    let src_services = "from utils import format_name\nresult = format_name('A', 'B')\n";

    let fs_utils = make_file_symbols("utils.py", src_utils, Language::Python);
    let fs_services = make_file_symbols("services.py", src_services, Language::Python);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_utils, fs_services], "format_name");

    // All resolved references should have def_file set.
    let resolved_with_def: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.resolution == Resolution::Resolved && r.def_file.is_some())
        .collect();

    assert!(
        !resolved_with_def.is_empty(),
        "Python A/B: resolved references MUST have def_file set (proving stack graphs \
         provide more info than tree-sitter alone). Got refs: {:?}",
        result.references
    );
}

#[test]
fn rust_resolved_refs_have_def_file() {
    let source = "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n";
    let fs = make_file_symbols("main.rs", source, Language::Rust);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    // Resolved references should have def_file set.
    assert!(
        !result.references.is_empty(),
        "Rust A/B: expected >= 1 resolved reference for 'greet', got 0"
    );
    for r in &result.references {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Rust A/B: expected Resolved, got {:?}",
            r.resolution
        );
        assert!(
            r.def_file.is_some(),
            "Rust A/B: resolved references MUST have def_file set, but got None"
        );
    }
}

// ===========================================================================
// resolve_callers: Rust resolved refs are returned by callers
// ===========================================================================

#[test]
fn rust_callers_returns_resolved() {
    let source = "fn greet() -> String { String::from(\"hello\") }\nfn main() { greet(); }\n";

    let mut resolver = StackGraphResolver::new();

    // resolve_refs should find resolved refs (TSG scope wiring works).
    let fs_for_refs = make_file_symbols("main.rs", source, Language::Rust);
    let refs_result = resolver.resolve_refs(&[fs_for_refs], "greet");
    assert!(
        !refs_result.references.is_empty(),
        "resolve_refs should find resolved refs for Rust"
    );
    for r in &refs_result.references {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "resolve_refs should return Resolved, got {:?}",
            r.resolution
        );
    }

    // resolve_callers filters to Resolved only — with working TSG rules,
    // Rust should now return callers.
    let fs_for_callers = make_file_symbols("main.rs", source, Language::Rust);
    let callers_result = resolver.resolve_callers(&[fs_for_callers], "greet");
    assert!(
        !callers_result.references.is_empty(),
        "resolve_callers should return resolved refs for Rust, \
         but got empty. Warnings: {:?}",
        callers_result.warnings
    );
    for r in &callers_result.references {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "resolve_callers should only contain Resolved refs, got {:?}",
            r.resolution
        );
    }
}

// ===========================================================================
// C: cross-file resolution via #include
// ===========================================================================

#[test]
fn c_cross_file_include_resolved() {
    // main.c includes utils.h, calls add() defined in utils.c.
    // The reference must resolve as Resolution::Resolved across files.
    let src_main = concat!(
        "#include \"utils.h\"\n",
        "#include <stdio.h>\n",
        "\n",
        "int main(int argc, char* argv[]) {\n",
        "    int result = add(2, 3);\n",
        "    printf(\"Result: %d\\n\", result);\n",
        "    return 0;\n",
        "}\n",
    );
    let src_utils_h = concat!(
        "#ifndef UTILS_H\n",
        "#define UTILS_H\n",
        "\n",
        "int add(int a, int b);\n",
        "int multiply(int a, int b);\n",
        "\n",
        "#endif\n",
    );
    let src_utils_c = concat!(
        "#include \"utils.h\"\n",
        "\n",
        "int add(int a, int b) {\n",
        "    return a + b;\n",
        "}\n",
        "\n",
        "int multiply(int a, int b) {\n",
        "    return a * b;\n",
        "}\n",
    );

    let fs_main = make_file_symbols("main.c", src_main, Language::C);
    let fs_utils_h = make_file_symbols("utils.h", src_utils_h, Language::C);
    let fs_utils_c = make_file_symbols("utils.c", src_utils_c, Language::C);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_main, fs_utils_h, fs_utils_c], "add");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C cross-file: expected >= 1 reference for 'add', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved (not Syntactic).
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // At least one reference must be in main.c pointing to utils.c definition.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("main"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("utils"))
    });
    assert!(
        cross_file_ref.is_some(),
        "C cross-file: expected at least one reference from main.c -> utils.c, \
         got refs: {:?}",
        refs
    );
}

#[test]
fn c_cross_file_fixture_project_resolved() {
    // Test against the actual C fixture project files.
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/c_project");
    let fixture_files = [
        ("main.c", Language::C),
        ("utils.h", Language::C),
        ("utils.c", Language::C),
    ];

    let scan_results: Vec<codequery_index::FileSymbols> = fixture_files
        .iter()
        .filter_map(|(rel, lang)| {
            let abs = fixture_root.join(rel);
            let source = std::fs::read_to_string(&abs).ok()?;
            Some(make_file_symbols(rel, &source, *lang))
        })
        .collect();

    assert_eq!(
        scan_results.len(),
        3,
        "all 3 C fixture files should be readable"
    );

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&scan_results, "add");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C fixture cross-file: expected >= 1 reference for 'add', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved.
    for r in refs {
        assert_eq!(r.symbol, "add");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C fixture cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // The reference from main.c should resolve to utils.c.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("main"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("utils"))
    });
    assert!(
        cross_file_ref.is_some(),
        "C fixture cross-file: expected main.c -> utils.c reference, got refs: {:?}",
        refs
    );
}

#[test]
fn c_cross_file_multiply_resolved() {
    // Verify cross-file works for multiply too (same pattern, different symbol).
    let src_main = concat!(
        "#include \"utils.h\"\n",
        "\n",
        "int main() {\n",
        "    int r = multiply(3, 4);\n",
        "    return r;\n",
        "}\n",
    );
    let src_utils_h = concat!(
        "#ifndef UTILS_H\n",
        "#define UTILS_H\n",
        "int multiply(int a, int b);\n",
        "#endif\n",
    );
    let src_utils_c = concat!(
        "#include \"utils.h\"\n",
        "int multiply(int a, int b) {\n",
        "    return a * b;\n",
        "}\n",
    );

    let fs_main = make_file_symbols("main.c", src_main, Language::C);
    let fs_utils_h = make_file_symbols("utils.h", src_utils_h, Language::C);
    let fs_utils_c = make_file_symbols("utils.c", src_utils_c, Language::C);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_main, fs_utils_h, fs_utils_c], "multiply");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C cross-file multiply: expected >= 1 reference, got 0. Warnings: {:?}",
        result.warnings
    );

    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C cross-file multiply: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("main"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("utils"))
    });
    assert!(
        cross_file_ref.is_some(),
        "C cross-file multiply: expected main.c -> utils.c reference, got refs: {:?}",
        refs
    );
}

// ===========================================================================
// C++: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn cpp_same_file_resolved() {
    // Free function defined and called in same file.
    let source = "void greet() {}\nint main() { greet(); return 0; }\n";
    let fs = make_file_symbols("main.cpp", source, Language::Cpp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C++ same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C++ same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn cpp_namespace_function_same_file_resolved() {
    // Function defined inside a namespace and called from same file.
    let source = concat!(
        "namespace mylib {\n",
        "    void greet() {}\n",
        "}\n",
        "void caller() { mylib::greet(); }\n",
    );
    let fs = make_file_symbols("app.cpp", source, Language::Cpp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    // At minimum, the definition should be found. Reference resolution through
    // qualified_identifier is a stretch goal; we check for no errors.
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "C++ namespace: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }
}

#[test]
fn cpp_class_method_same_file_resolved() {
    // Class with inline method, called in same file.
    let source = concat!(
        "class Dog {\n",
        "public:\n",
        "    void speak() {}\n",
        "};\n",
        "void caller() {\n",
        "    Dog d;\n",
        "    d.speak();\n",
        "}\n",
    );
    let fs = make_file_symbols("app.cpp", source, Language::Cpp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "speak");

    // The key assertion is that the file doesn't cause TSG errors.
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "C++ class: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }
}

#[test]
fn cpp_cross_file_include_resolved() {
    // main.cpp includes models.hpp, calls a function defined in models.hpp.
    let src_header = concat!(
        "#ifndef MODELS_HPP\n",
        "#define MODELS_HPP\n",
        "void greet() {}\n",
        "#endif\n",
    );
    let src_main = concat!(
        "#include \"models.hpp\"\n",
        "int main() { greet(); return 0; }\n",
    );

    let fs_header = make_file_symbols("models.hpp", src_header, Language::Cpp);
    let fs_main = make_file_symbols("main.cpp", src_main, Language::Cpp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs_header, fs_main], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C++ cross-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );

    // All references must be Resolved.
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C++ cross-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }

    // At least one reference must be from main.cpp -> models.hpp.
    let cross_file_ref = refs.iter().find(|r| {
        r.ref_file.to_str().is_some_and(|s| s.contains("main"))
            && r.def_file
                .as_ref()
                .and_then(|p| p.to_str())
                .is_some_and(|s| s.contains("models"))
    });
    assert!(
        cross_file_ref.is_some(),
        "C++ cross-file: expected main.cpp -> models.hpp reference, got refs: {:?}",
        refs
    );
}

#[test]
fn cpp_file_with_comments_and_ifdef_resolved() {
    // Real-world C++ files have comments, include guards, and preprocessor directives.
    let source = concat!(
        "/* Multi-line\n",
        "   comment */\n",
        "#ifndef MAIN_HPP\n",
        "#define MAIN_HPP\n",
        "\n",
        "// Single-line comment\n",
        "#include <iostream>\n",
        "\n",
        "void greet() {\n",
        "    // comment inside function body\n",
        "    int x = 42;\n",
        "}\n",
        "\n",
        "int main() {\n",
        "    greet(); /* inline comment */\n",
        "    return 0;\n",
        "}\n",
        "\n",
        "#endif\n",
    );
    let fs = make_file_symbols("main.cpp", source, Language::Cpp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C++ with comments/ifdef: expected >= 1 reference for 'greet', got 0. \
         Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C++ with comments/ifdef: references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn cpp_fixture_project_no_tsg_errors() {
    // Test against the actual C++ fixture project files.
    // The key assertion: no "Undefined scoped variable" errors on real code.
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cpp_project");
    let fixture_files = [
        ("main.cpp", Language::Cpp),
        ("models.hpp", Language::Cpp),
        ("models.cpp", Language::Cpp),
    ];

    let scan_results: Vec<codequery_index::FileSymbols> = fixture_files
        .iter()
        .filter_map(|(rel, lang)| {
            let abs = fixture_root.join(rel);
            let source = std::fs::read_to_string(&abs).ok()?;
            Some(make_file_symbols(rel, &source, *lang))
        })
        .collect();

    assert_eq!(
        scan_results.len(),
        3,
        "all 3 C++ fixture files should be readable"
    );

    let mut resolver = StackGraphResolver::new();

    // Try resolving a symbol from the fixture project.
    // We pick "speak" which is defined in multiple places.
    let result = resolver.resolve_refs(&scan_results, "speak");

    // No "Undefined scoped variable" errors.
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "C++ fixture: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }
}

// ===========================================================================
// Ruby: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn ruby_same_file_resolved() {
    let source = "def greet(name)\n  \"Hello, #{name}!\"\nend\n\ngreet('world')\n";
    let fs = make_file_symbols("app.rb", source, Language::Ruby);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Ruby same-file: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Ruby same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn ruby_same_file_class_method_resolved() {
    // Class with a method defined and called within the same file.
    let source = concat!(
        "class User\n",
        "  def greet\n",
        "    \"Hello\"\n",
        "  end\n",
        "end\n",
        "\n",
        "u = User.new\n",
    );
    let fs = make_file_symbols("models.rb", source, Language::Ruby);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "User");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Ruby class same-file: expected >= 1 reference for 'User', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Ruby class same-file: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn ruby_file_with_comments_resolved() {
    // Real-world Ruby files have comments. Ensure no TSG errors.
    let source = concat!(
        "# Main entry point for the Ruby project.\n",
        "# This is a multi-line comment block.\n",
        "\n",
        "def greet(name)\n",
        "  # Greet the user\n",
        "  \"Hello, #{name}!\"\n",
        "end\n",
        "\n",
        "# Call the function\n",
        "result = greet('world')\n",
    );
    let fs = make_file_symbols("app.rb", source, Language::Ruby);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Ruby with comments: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "Ruby with comments: references MUST be Resolved, got {:?}. \
             This likely means the wildcard fix regressed.",
            r.resolution
        );
    }
}

#[test]
fn ruby_module_method_no_tsg_errors() {
    // Module with singleton methods should not cause TSG errors.
    let source = concat!(
        "module Utils\n",
        "  def self.format_name(first, last)\n",
        "    \"#{first} #{last}\"\n",
        "  end\n",
        "\n",
        "  def self.validate(value)\n",
        "    !value.nil?\n",
        "  end\n",
        "end\n",
    );
    let fs = make_file_symbols("utils.rb", source, Language::Ruby);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "format_name");

    // The key assertion is that warnings don't contain "Undefined scoped variable".
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "Ruby module: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }
}

#[test]
fn ruby_fixture_project_no_tsg_errors() {
    // Test against the actual Ruby fixture project files.
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ruby_project");
    let fixture_files = [
        ("lib/main.rb", Language::Ruby),
        ("lib/models.rb", Language::Ruby),
        ("lib/utils.rb", Language::Ruby),
    ];

    let scan_results: Vec<codequery_index::FileSymbols> = fixture_files
        .iter()
        .filter_map(|(rel, lang)| {
            let abs = fixture_root.join(rel);
            let source = std::fs::read_to_string(&abs).ok()?;
            Some(make_file_symbols(rel, &source, *lang))
        })
        .collect();

    assert_eq!(
        scan_results.len(),
        3,
        "all 3 Ruby fixture files should be readable"
    );

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&scan_results, "greet");

    // No "Undefined scoped variable" errors.
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "Ruby fixture: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }

    // Should find at least one reference.
    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "Ruby fixture: expected >= 1 reference for 'greet', got 0. Warnings: {:?}",
        result.warnings
    );
}

// ===========================================================================
// C#: MUST produce Resolution::Resolved
// ===========================================================================

#[test]
fn csharp_same_file_resolved() {
    let source =
        "class Program {\n  static void Greet() { }\n  static void Main() { Greet(); }\n}\n";
    let fs = make_file_symbols("Program.cs", source, Language::CSharp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "Greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C# same-file: expected >= 1 reference for 'Greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(r.symbol, "Greet");
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C# same-file: all references MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn csharp_class_with_members_resolved() {
    let source = concat!(
        "class User {\n",
        "  private int _age;\n",
        "  public void Greet() { }\n",
        "  public void Run() { Greet(); }\n",
        "}\n",
    );
    let fs = make_file_symbols("User.cs", source, Language::CSharp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "Greet");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C# class members: expected >= 1 reference for 'Greet', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C# class members: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn csharp_namespace_class_resolved() {
    let source = concat!(
        "namespace MyApp {\n",
        "  class Helper {\n",
        "    static void DoWork() { }\n",
        "    static void Main() { DoWork(); }\n",
        "  }\n",
        "}\n",
    );
    let fs = make_file_symbols("Helper.cs", source, Language::CSharp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "DoWork");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C# namespace: expected >= 1 reference for 'DoWork', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C# namespace: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn csharp_local_variable_resolved() {
    let source = concat!(
        "class Program {\n",
        "  static void Main() {\n",
        "    int x = 42;\n",
        "    int y = x;\n",
        "  }\n",
        "}\n",
    );
    let fs = make_file_symbols("Locals.cs", source, Language::CSharp);

    let mut resolver = StackGraphResolver::new();
    let result = resolver.resolve_refs(&[fs], "x");

    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C# locals: expected >= 1 reference for 'x', got 0. Warnings: {:?}",
        result.warnings
    );
    for r in refs {
        assert_eq!(
            r.resolution,
            Resolution::Resolved,
            "C# locals: reference MUST be Resolved, got {:?}",
            r.resolution
        );
    }
}

#[test]
fn csharp_fixture_project_no_tsg_errors() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/csharp_project");
    let fixture_files = vec![("src/Models.cs", Language::CSharp)];

    let scan_results: Vec<codequery_index::FileSymbols> = fixture_files
        .iter()
        .filter_map(|(rel, lang)| {
            let abs = fixture_root.join(rel);
            let source = std::fs::read_to_string(&abs).ok()?;
            Some(make_file_symbols(rel, &source, *lang))
        })
        .collect();

    assert_eq!(scan_results.len(), 1, "C# fixture file should be readable");

    let mut resolver = StackGraphResolver::new();

    // Resolve ValidateAge — it is defined and called within the same file.
    let result = resolver.resolve_refs(&scan_results, "ValidateAge");

    // No "Undefined scoped variable" errors.
    for w in &result.warnings {
        assert!(
            !w.contains("Undefined scoped variable"),
            "C# fixture: should not produce 'Undefined scoped variable' errors, \
             got warning: {w}"
        );
    }

    // Should find at least one reference (ValidateAge() is called on line 18).
    let refs = &result.references;
    assert!(
        !refs.is_empty(),
        "C# fixture: expected >= 1 reference for 'ValidateAge', got 0. Warnings: {:?}",
        result.warnings
    );
}
