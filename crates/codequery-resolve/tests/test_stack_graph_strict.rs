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
