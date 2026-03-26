//! Cross-language integration tests for cq commands.
//!
//! Verifies that all commands work correctly across Tier 1 languages
//! (Rust, TypeScript, JavaScript, Python, Go, C, C++, Java) using
//! per-language fixture projects and the mixed-language fixture project.

mod common;

use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn mixed_project() -> PathBuf {
    fixture_base().join("mixed_project")
}

fn rust_project() -> PathBuf {
    fixture_base().join("rust_project")
}

fn typescript_project() -> PathBuf {
    fixture_base().join("typescript_project")
}

fn python_project() -> PathBuf {
    fixture_base().join("python_project")
}

fn go_project() -> PathBuf {
    fixture_base().join("go_project")
}

fn c_project() -> PathBuf {
    fixture_base().join("c_project")
}

fn cpp_project() -> PathBuf {
    fixture_base().join("cpp_project")
}

fn java_project() -> PathBuf {
    fixture_base().join("java_project")
}

/// Run cq against a specific fixture project.
fn run_cq_project(project: &PathBuf, args: &[&str]) -> std::process::Output {
    let project_str = project.to_str().unwrap();
    let mut full_args = vec!["--project", project_str];
    full_args.extend_from_slice(args);
    run_cq(&full_args)
}

// ===========================================================================
// outline command across all languages
// ===========================================================================

#[test]
fn test_outline_rust_extracts_symbols() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(
        out.contains("MAX_RETRIES (const, pub)"),
        "missing MAX_RETRIES: {out}"
    );
}

#[test]
fn test_outline_typescript_extracts_symbols() {
    let project = typescript_project();
    let file = project.join("src/index.ts");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(
        out.contains("MAX_RETRIES (const, pub)"),
        "missing MAX_RETRIES: {out}"
    );
}

#[test]
fn test_outline_javascript_extracts_symbols() {
    let project = typescript_project();
    let file = project.join("src/utils.js");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("formatName (function,"),
        "missing formatName: {out}"
    );
}

#[test]
fn test_outline_python_extracts_symbols() {
    let project = python_project();
    let file = project.join("src/main.py");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(
        out.contains("MAX_RETRIES (const, pub)"),
        "missing MAX_RETRIES: {out}"
    );
}

#[test]
fn test_outline_go_extracts_symbols() {
    let project = go_project();
    let file = project.join("main.go");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Greet (function, pub)"),
        "missing Greet: {out}"
    );
    assert!(
        out.contains("MaxRetries (const, pub)"),
        "missing MaxRetries: {out}"
    );
}

#[test]
fn test_outline_c_extracts_symbols() {
    let project = c_project();
    let file = project.join("main.c");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("main (function, pub)"), "missing main: {out}");
    assert!(
        out.contains("Config (struct, pub)"),
        "missing Config: {out}"
    );
    assert!(
        out.contains("LogLevel (enum, pub)"),
        "missing LogLevel: {out}"
    );
}

#[test]
fn test_outline_cpp_extracts_symbols() {
    let project = cpp_project();
    let file = project.join("models.hpp");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Animal (class, pub)"), "missing Animal: {out}");
    assert!(out.contains("Dog (class, pub)"), "missing Dog: {out}");
    assert!(out.contains("Color (enum, pub)"), "missing Color: {out}");
}

#[test]
fn test_outline_java_extracts_symbols() {
    let project = java_project();
    let file = project.join("src/main/java/com/example/Main.java");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Main (class, pub)"), "missing Main: {out}");
    assert!(
        out.contains("main (method, pub)"),
        "missing main method: {out}"
    );
}

// ===========================================================================
// def command across all languages
// ===========================================================================

#[test]
fn test_def_typescript_finds_function() {
    let output = run_cq_project(&typescript_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_python_finds_function() {
    let output = run_cq_project(&python_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_go_finds_function() {
    let output = run_cq_project(&go_project(), &["def", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function Greet"),
        "missing function Greet: {out}"
    );
}

#[test]
fn test_def_c_finds_function() {
    let output = run_cq_project(&c_project(), &["def", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function main"),
        "missing function main: {out}"
    );
}

#[test]
fn test_def_java_finds_class() {
    let output = run_cq_project(&java_project(), &["def", "Main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("class Main"), "missing class Main: {out}");
}

#[test]
fn test_def_python_finds_class() {
    let output = run_cq_project(&python_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("class User"), "missing class User: {out}");
}

#[test]
fn test_def_go_finds_struct() {
    let output = run_cq_project(&go_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("struct User"), "missing struct User: {out}");
}

// ===========================================================================
// body command across all languages
// ===========================================================================

#[test]
fn test_body_typescript_extracts_function_body() {
    let output = run_cq_project(&typescript_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Hello"), "body should contain Hello: {out}");
}

#[test]
fn test_body_python_extracts_function_body() {
    let output = run_cq_project(&python_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Hello"), "body should contain Hello: {out}");
}

#[test]
fn test_body_go_extracts_function_body() {
    let output = run_cq_project(&go_project(), &["body", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Hello"), "body should contain Hello: {out}");
}

#[test]
fn test_body_c_extracts_function_body() {
    let output = run_cq_project(&c_project(), &["body", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("return 0"),
        "body should contain return: {out}"
    );
}

#[test]
fn test_body_java_extracts_class_body() {
    let output = run_cq_project(&java_project(), &["body", "Main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("public class Main"),
        "body should contain class def: {out}"
    );
}

// ===========================================================================
// sig command across all languages
// ===========================================================================

#[test]
fn test_sig_typescript_extracts_function_signature() {
    let output = run_cq_project(&typescript_project(), &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet(name: string): string"),
        "missing sig: {out}"
    );
}

#[test]
fn test_sig_python_extracts_function_signature() {
    let output = run_cq_project(&python_project(), &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("def greet(name: str) -> str"),
        "missing sig: {out}"
    );
}

#[test]
fn test_sig_go_extracts_function_signature() {
    let output = run_cq_project(&go_project(), &["sig", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("func Greet(name string) string"),
        "missing sig: {out}"
    );
}

#[test]
fn test_sig_c_extracts_function_signature() {
    let output = run_cq_project(&c_project(), &["sig", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("int main(int argc, char* argv[])"),
        "missing sig: {out}"
    );
}

#[test]
fn test_sig_java_extracts_method_signature() {
    let output = run_cq_project(&java_project(), &["sig", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("public static void main(String[] args)"),
        "missing sig: {out}"
    );
}

// ===========================================================================
// imports command across all languages
// ===========================================================================

#[test]
fn test_imports_rust_extracts_use_statements() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_imports_typescript_extracts_imports() {
    let project = typescript_project();
    let file = project.join("src/services.ts");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("import"), "should contain import: {out}");
}

#[test]
fn test_imports_go_extracts_imports() {
    let project = go_project();
    let file = project.join("main.go");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("fmt"), "should contain fmt: {out}");
}

#[test]
fn test_imports_c_extracts_includes() {
    let project = c_project();
    let file = project.join("main.c");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("stdio.h"), "should contain stdio.h: {out}");
}

#[test]
fn test_imports_cpp_extracts_includes() {
    let project = cpp_project();
    let file = project.join("main.cpp");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("iostream"), "should contain iostream: {out}");
}

#[test]
fn test_imports_java_extracts_imports() {
    let project = java_project();
    let file = project.join("src/main/java/com/example/Main.java");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("com.example.models.User"),
        "should contain User import: {out}"
    );
}

// ===========================================================================
// context command across all languages
// ===========================================================================

#[test]
fn test_context_typescript_finds_enclosing_function() {
    let project = typescript_project();
    let file = project.join("src/index.ts");
    let location = format!("{}:3", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function greet"), "should find greet: {out}");
}

#[test]
fn test_context_python_finds_enclosing_function() {
    let project = python_project();
    let file = project.join("src/main.py");
    let location = format!("{}:8", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function greet"), "should find greet: {out}");
}

#[test]
fn test_context_go_finds_enclosing_function() {
    let project = go_project();
    let file = project.join("main.go");
    let location = format!("{}:16", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function Greet"), "should find Greet: {out}");
}

#[test]
fn test_context_c_finds_enclosing_function() {
    let project = c_project();
    let file = project.join("main.c");
    let location = format!("{}:6", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function main"), "should find main: {out}");
}

#[test]
fn test_context_java_finds_enclosing_method() {
    let project = java_project();
    let file = project.join("src/main/java/com/example/Main.java");
    let location = format!("{}:9", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("method main"), "should find main: {out}");
}

// ===========================================================================
// symbols command — mixed project shows all languages
// ===========================================================================

#[test]
fn test_symbols_mixed_project_finds_rust_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/main.rs"),
        "should contain Rust file: {out}"
    );
    assert!(out.contains("greet"), "should contain Rust greet: {out}");
}

#[test]
fn test_symbols_mixed_project_finds_typescript_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/app.ts"),
        "should contain TypeScript file: {out}"
    );
    assert!(
        out.contains("Application"),
        "should contain TS class: {out}"
    );
}

#[test]
fn test_symbols_mixed_project_finds_python_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/utils.py"),
        "should contain Python file: {out}"
    );
    assert!(
        out.contains("Connection"),
        "should contain Python class: {out}"
    );
}

#[test]
fn test_symbols_mixed_project_finds_go_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/handler.go"),
        "should contain Go file: {out}"
    );
    assert!(out.contains("Handler"), "should contain Go struct: {out}");
}

#[test]
fn test_symbols_mixed_project_finds_c_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("src/core.c"), "should contain C file: {out}");
    assert!(out.contains("CoreState"), "should contain C struct: {out}");
}

#[test]
fn test_symbols_mixed_project_finds_java_symbols() {
    let output = run_cq_project(&mixed_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/Models.java"),
        "should contain Java file: {out}"
    );
    assert!(out.contains("Models"), "should contain Java class: {out}");
}

// ===========================================================================
// tree command — mixed project shows all languages
// ===========================================================================

#[test]
fn test_tree_mixed_project_shows_all_language_files() {
    let output = run_cq_project(&mixed_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("src/main.rs"), "missing Rust file: {out}");
    assert!(out.contains("src/app.ts"), "missing TS file: {out}");
    assert!(out.contains("src/utils.py"), "missing Python file: {out}");
    assert!(out.contains("src/handler.go"), "missing Go file: {out}");
    assert!(out.contains("src/core.c"), "missing C file: {out}");
    assert!(out.contains("src/Models.java"), "missing Java file: {out}");
}

#[test]
fn test_tree_mixed_project_shows_symbols_from_each_language() {
    let output = run_cq_project(&mixed_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // Spot-check one symbol from each language
    assert!(
        out.contains("Config (struct, pub)"),
        "missing Rust struct: {out}"
    );
    assert!(
        out.contains("AppConfig (interface, pub)"),
        "missing TS interface: {out}"
    );
    assert!(
        out.contains("format_address (function, pub)"),
        "missing Python fn: {out}"
    );
    assert!(
        out.contains("NewHandler (function, pub)"),
        "missing Go fn: {out}"
    );
    assert!(
        out.contains("core_init (function, pub)"),
        "missing C fn: {out}"
    );
    assert!(
        out.contains("getId (method, pub)"),
        "missing Java method: {out}"
    );
}

// ===========================================================================
// refs command — generic fallback for non-Rust languages
// ===========================================================================

#[test]
fn test_refs_go_finds_function_references() {
    let output = run_cq_project(&go_project(), &["refs", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // Greet is called in main.go and referenced in test
    assert!(out.contains("definition"), "should show definition: {out}");
}

#[test]
fn test_refs_python_finds_function_references() {
    let output = run_cq_project(&python_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("definition"), "should show definition: {out}");
}

#[test]
fn test_refs_typescript_returns_success() {
    let output = run_cq_project(&typescript_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_refs_c_returns_success_for_main() {
    let output = run_cq_project(&c_project(), &["refs", "add"]);
    assert_exit_code(&output, 0);
}

// ===========================================================================
// callers command — generic fallback for non-Rust languages
// ===========================================================================

#[test]
fn test_callers_go_finds_call_sites() {
    let output = run_cq_project(&go_project(), &["callers", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("call"), "should show call refs: {out}");
}

#[test]
fn test_callers_python_finds_call_sites() {
    let output = run_cq_project(&python_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("call"), "should show call refs: {out}");
}

// ===========================================================================
// deps command — generic fallback for non-Rust languages
// ===========================================================================

#[test]
fn test_deps_go_finds_dependencies() {
    let output = run_cq_project(&go_project(), &["deps", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Greet"), "should list Greet as dep: {out}");
}

// ===========================================================================
// def across mixed_project — finds symbols from multiple languages
// ===========================================================================

#[test]
fn test_def_mixed_project_finds_rust_function() {
    let output = run_cq_project(&mixed_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/main.rs"),
        "should find in Rust file: {out}"
    );
}

#[test]
fn test_def_mixed_project_finds_go_function() {
    let output = run_cq_project(&mixed_project(), &["def", "NewHandler"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/handler.go"),
        "should find in Go file: {out}"
    );
}

#[test]
fn test_def_mixed_project_finds_python_class() {
    let output = run_cq_project(&mixed_project(), &["def", "Connection"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/utils.py"),
        "should find in Python file: {out}"
    );
}

#[test]
fn test_def_mixed_project_finds_c_struct() {
    let output = run_cq_project(&mixed_project(), &["def", "CoreState"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("src/core.c"), "should find in C file: {out}");
}

#[test]
fn test_def_mixed_project_finds_java_class() {
    let output = run_cq_project(&mixed_project(), &["def", "Models"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/Models.java"),
        "should find in Java file: {out}"
    );
}

#[test]
fn test_def_mixed_project_finds_ts_class() {
    let output = run_cq_project(&mixed_project(), &["def", "Application"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("src/app.ts"), "should find in TS file: {out}");
}

// ===========================================================================
// body/sig across mixed_project
// ===========================================================================

#[test]
fn test_body_mixed_project_go_function() {
    let output = run_cq_project(&mixed_project(), &["body", "NewHandler"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Handler"),
        "body should reference Handler: {out}"
    );
}

#[test]
fn test_sig_mixed_project_python_function() {
    let output = run_cq_project(&mixed_project(), &["sig", "format_address"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("def format_address"),
        "sig should contain def: {out}"
    );
}

#[test]
fn test_sig_mixed_project_c_function() {
    let output = run_cq_project(&mixed_project(), &["sig", "core_init"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("void core_init"),
        "sig should contain void core_init: {out}"
    );
}

// ===========================================================================
// C pointer-return-type function name extraction (regression)
// ===========================================================================

#[test]
fn test_outline_c_pointer_return_type_extracts_clean_name() {
    let project = mixed_project();
    let file = project.join("src/core.c");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // status_string returns const char* — name should be clean, not include params
    assert!(
        out.contains("status_string (function, pub)"),
        "should extract clean name for pointer-return function: {out}"
    );
}

#[test]
fn test_def_c_pointer_return_function() {
    let output = run_cq_project(&mixed_project(), &["def", "status_string"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function status_string"),
        "should find status_string: {out}"
    );
}
