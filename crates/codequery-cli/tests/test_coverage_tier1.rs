//! E2E test coverage for Tier 1 languages: C++, JavaScript, C, Go, Java, TypeScript, Python.
//!
//! Fills gaps identified by the coverage matrix. Every test validates actual
//! content output (not just exit codes).

mod common;

use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn cpp_project() -> PathBuf {
    fixture_base().join("cpp_project")
}

fn typescript_project() -> PathBuf {
    fixture_base().join("typescript_project")
}

fn python_project() -> PathBuf {
    fixture_base().join("python_project")
}

fn c_project() -> PathBuf {
    fixture_base().join("c_project")
}

fn go_project() -> PathBuf {
    fixture_base().join("go_project")
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

/// Parse stdout as JSON.
fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let text = stdout(output);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nstdout was: {text}");
    })
}

// ===========================================================================
// C++ — def
// ===========================================================================

#[test]
fn test_def_cpp_finds_class() {
    let output = run_cq_project(&cpp_project(), &["def", "Dog"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class Dog"),
        "should find class Dog definition: {out}"
    );
    assert!(out.contains("models.hpp"), "should be in models.hpp: {out}");
}

#[test]
fn test_def_cpp_finds_free_function() {
    let output = run_cq_project(&cpp_project(), &["def", "free_function"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function free_function"),
        "should find function free_function: {out}"
    );
    assert!(out.contains("main.cpp"), "should be in main.cpp: {out}");
}

#[test]
fn test_def_cpp_finds_enum() {
    let output = run_cq_project(&cpp_project(), &["def", "Color"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("enum Color"),
        "should find enum Color definition: {out}"
    );
}

// ===========================================================================
// C++ — body
// ===========================================================================

#[test]
fn test_body_cpp_extracts_free_function_body() {
    let output = run_cq_project(&cpp_project(), &["body", "free_function"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("free function outside"),
        "body should contain comment text: {out}"
    );
}

#[test]
fn test_body_cpp_extracts_class_body() {
    let output = run_cq_project(&cpp_project(), &["body", "Dog"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("speak() const override"),
        "Dog body should contain speak override: {out}"
    );
    assert!(
        out.contains("tricks_count_"),
        "Dog body should contain tricks_count_ field: {out}"
    );
}

// ===========================================================================
// C++ — sig
// ===========================================================================

#[test]
fn test_sig_cpp_extracts_free_function_signature() {
    let output = run_cq_project(&cpp_project(), &["sig", "free_function"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("void free_function()"),
        "should extract void free_function(): {out}"
    );
}

#[test]
fn test_sig_cpp_extracts_main_signature() {
    let output = run_cq_project(&cpp_project(), &["sig", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("int main()"),
        "should extract int main() signature: {out}"
    );
}

// ===========================================================================
// C++ — refs
// ===========================================================================

#[test]
fn test_refs_cpp_finds_dog_references() {
    let output = run_cq_project(&cpp_project(), &["refs", "Dog"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("definition"),
        "should show Dog definition: {out}"
    );
    assert!(
        out.contains("models.hpp"),
        "should reference models.hpp: {out}"
    );
}

#[test]
fn test_refs_cpp_finds_speak_references() {
    let output = run_cq_project(&cpp_project(), &["refs", "speak"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("definition"),
        "should show speak definitions: {out}"
    );
    // speak is defined in models.hpp (pure virtual + override) and implemented in models.cpp
    assert!(
        out.contains("models.hpp"),
        "should reference models.hpp: {out}"
    );
}

// ===========================================================================
// C++ — callers
// ===========================================================================

#[test]
fn test_callers_cpp_finds_speak_callers() {
    let output = run_cq_project(&cpp_project(), &["callers", "speak"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("models.cpp"),
        "should find call in models.cpp: {out}"
    );
    assert!(
        out.contains("caller"),
        "output should mention callers: {out}"
    );
}

// ===========================================================================
// C++ — deps
// ===========================================================================

#[test]
fn test_deps_cpp_finds_main_dependencies() {
    let output = run_cq_project(&cpp_project(), &["deps", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("dog"), "main should depend on dog call: {out}");
}

// ===========================================================================
// C++ — context
// ===========================================================================

#[test]
fn test_context_cpp_finds_enclosing_namespace() {
    let project = cpp_project();
    let file = project.join("models.cpp");
    let location = format!("{}:6", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("module mylib"),
        "should find enclosing namespace mylib: {out}"
    );
}

#[test]
fn test_context_cpp_finds_enclosing_function() {
    let project = cpp_project();
    let file = project.join("main.cpp");
    let location = format!("{}:7", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function main"),
        "should find enclosing function main: {out}"
    );
}

// ===========================================================================
// C++ — tree
// ===========================================================================

#[test]
fn test_tree_cpp_shows_all_source_files() {
    let output = run_cq_project(&cpp_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("main.cpp"), "should show main.cpp: {out}");
    assert!(out.contains("models.hpp"), "should show models.hpp: {out}");
    assert!(out.contains("models.cpp"), "should show models.cpp: {out}");
}

#[test]
fn test_tree_cpp_shows_symbols() {
    let output = run_cq_project(&cpp_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Animal (class, pub)"),
        "tree should show Animal class: {out}"
    );
    assert!(
        out.contains("Dog (class, pub)"),
        "tree should show Dog class: {out}"
    );
    assert!(
        out.contains("free_function (function, pub)"),
        "tree should show free_function: {out}"
    );
}

// ===========================================================================
// C++ — symbols
// ===========================================================================

#[test]
fn test_symbols_cpp_finds_all_expected_symbols() {
    let output = run_cq_project(&cpp_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class Animal"),
        "should find Animal class: {out}"
    );
    assert!(out.contains("class Dog"), "should find Dog class: {out}");
    assert!(out.contains("enum Color"), "should find Color enum: {out}");
    assert!(
        out.contains("function free_function"),
        "should find free_function: {out}"
    );
    assert!(
        out.contains("method speak"),
        "should find speak method: {out}"
    );
    assert!(
        out.contains("method get_name"),
        "should find get_name method: {out}"
    );
}

// ===========================================================================
// C++ — search (raw S-expression)
// ===========================================================================

#[test]
fn test_search_cpp_finds_classes_by_sexpr() {
    let output = run_cq_project(
        &cpp_project(),
        &[
            "--raw",
            "search",
            "(class_specifier name: (type_identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Animal"), "should find Animal: {out}");
    assert!(out.contains("Dog"), "should find Dog: {out}");
}

// ===========================================================================
// JavaScript — def
// ===========================================================================

#[test]
fn test_def_js_finds_function() {
    let output = run_cq_project(&typescript_project(), &["def", "formatName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function formatName"),
        "should find function formatName: {out}"
    );
    assert!(out.contains("utils.js"), "should be in utils.js: {out}");
}

#[test]
fn test_def_js_finds_class() {
    let output = run_cq_project(&typescript_project(), &["def", "Logger"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class Logger"),
        "should find class Logger: {out}"
    );
}

// ===========================================================================
// JavaScript — body
// ===========================================================================

#[test]
fn test_body_js_extracts_function_body() {
    let output = run_cq_project(&typescript_project(), &["body", "formatName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("return first"),
        "body should contain return statement: {out}"
    );
}

// ===========================================================================
// JavaScript — sig
// ===========================================================================

#[test]
fn test_sig_js_extracts_function_signature() {
    let output = run_cq_project(&typescript_project(), &["sig", "formatName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function formatName(first, last)"),
        "should extract formatName signature: {out}"
    );
}

// ===========================================================================
// JavaScript — refs
// ===========================================================================

#[test]
fn test_refs_js_finds_formatname_references() {
    let output = run_cq_project(&typescript_project(), &["refs", "formatName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("definition"),
        "should show formatName definition: {out}"
    );
    // After enrichment, formatName is called at the bottom of utils.js
    assert!(
        out.contains("reference") || out.contains("call"),
        "should show call or reference for formatName: {out}"
    );
}

// ===========================================================================
// JavaScript — callers
// ===========================================================================

#[test]
fn test_callers_js_finds_formatname_callers() {
    let output = run_cq_project(&typescript_project(), &["callers", "formatName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("caller"),
        "should find callers of formatName: {out}"
    );
    assert!(
        out.contains("formatName"),
        "output should mention formatName: {out}"
    );
}

// ===========================================================================
// JavaScript — context
// ===========================================================================

#[test]
fn test_context_js_finds_enclosing_function() {
    let project = typescript_project();
    let file = project.join("src/utils.js");
    let location = format!("{}:2", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function formatName"),
        "should find enclosing formatName function: {out}"
    );
}

#[test]
fn test_context_js_finds_enclosing_class_method() {
    let project = typescript_project();
    let file = project.join("src/utils.js");
    // Line 11 is inside Logger.log method
    let location = format!("{}:11", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("log") || out.contains("Logger"),
        "should find enclosing log method or Logger class: {out}"
    );
}

// ===========================================================================
// JavaScript — tree
// ===========================================================================

#[test]
fn test_tree_js_shows_utils_file() {
    let output = run_cq_project(&typescript_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/utils.js"),
        "tree should show utils.js: {out}"
    );
    assert!(
        out.contains("formatName (function,"),
        "tree should show formatName function: {out}"
    );
    assert!(
        out.contains("Logger (class,"),
        "tree should show Logger class: {out}"
    );
}

// ===========================================================================
// JavaScript — symbols
// ===========================================================================

#[test]
fn test_symbols_js_finds_expected_symbols() {
    let output = run_cq_project(&typescript_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function formatName"),
        "should find formatName: {out}"
    );
    assert!(
        out.contains("class Logger"),
        "should find Logger class: {out}"
    );
    assert!(
        out.contains("function double"),
        "should find double arrow fn: {out}"
    );
    assert!(
        out.contains("function exported"),
        "should find exported fn: {out}"
    );
}

// ===========================================================================
// JavaScript — search (raw S-expression)
// ===========================================================================

#[test]
fn test_search_js_finds_functions_by_sexpr() {
    let output = run_cq_project(
        &typescript_project(),
        &[
            "--raw",
            "search",
            "(function_declaration name: (identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("formatName"), "should find formatName: {out}");
    assert!(
        out.contains("exported"),
        "should find exported function: {out}"
    );
}

// ===========================================================================
// JavaScript — imports (utils.js has no imports, so exit code 1)
// ===========================================================================

#[test]
fn test_imports_js_no_imports_in_utils() {
    let project = typescript_project();
    let file = project.join("src/utils.js");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    // utils.js has no import statements, expect exit code 1 (NoResults)
    assert_exit_code(&output, 1);
}

// ===========================================================================
// TypeScript — callers
// ===========================================================================

#[test]
fn test_callers_ts_finds_greet_callers() {
    let output = run_cq_project(&typescript_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("caller"),
        "should find callers of greet: {out}"
    );
    // greet is called in services.ts greetUser method and re-exported in reexport.ts
    assert!(
        out.contains("services.ts"),
        "should find call in services.ts: {out}"
    );
}

// ===========================================================================
// TypeScript — deps
// ===========================================================================

#[test]
fn test_deps_ts_finds_userservice_dependencies() {
    let output = run_cq_project(&typescript_project(), &["deps", "UserService"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "UserService should depend on greet: {out}"
    );
    assert!(
        out.contains("index.ts"),
        "greet dependency should resolve to index.ts: {out}"
    );
}

// ===========================================================================
// Python — imports
// ===========================================================================

#[test]
fn test_imports_python_extracts_from_imports() {
    let project = python_project();
    let file = project.join("src/services.py");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("models"),
        "should find import from models: {out}"
    );
    assert!(
        out.contains("utils"),
        "should find import from utils: {out}"
    );
}

// ===========================================================================
// C — callers
// ===========================================================================

#[test]
fn test_callers_c_finds_add_callers() {
    let output = run_cq_project(&c_project(), &["callers", "add"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("caller"), "should find callers of add: {out}");
    assert!(out.contains("main.c"), "should find call in main.c: {out}");
}

// ===========================================================================
// C — deps
// ===========================================================================

#[test]
fn test_deps_c_finds_main_dependencies() {
    let output = run_cq_project(&c_project(), &["deps", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("add"), "main should depend on add: {out}");
    assert!(
        out.contains("utils.c"),
        "add should resolve to utils.c: {out}"
    );
}

// ===========================================================================
// C — search (raw S-expression)
// ===========================================================================

#[test]
fn test_search_c_finds_function_definitions() {
    let output = run_cq_project(
        &c_project(),
        &[
            "--raw",
            "search",
            "(function_definition declarator: (function_declarator declarator: (identifier) @name))",
        ],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("main"), "should find main: {out}");
    assert!(out.contains("add"), "should find add: {out}");
    assert!(out.contains("multiply"), "should find multiply: {out}");
}

// ===========================================================================
// Go — search (raw S-expression)
// ===========================================================================

#[test]
fn test_search_go_finds_function_declarations() {
    let output = run_cq_project(
        &go_project(),
        &[
            "--raw",
            "search",
            "(function_declaration name: (identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Greet"), "should find Greet: {out}");
    assert!(out.contains("helper"), "should find helper: {out}");
    assert!(out.contains("FormatName"), "should find FormatName: {out}");
}

// ===========================================================================
// Java — refs
// ===========================================================================

#[test]
fn test_refs_java_finds_getname_references() {
    let output = run_cq_project(&java_project(), &["refs", "getName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("definition"),
        "should show getName definition: {out}"
    );
    assert!(
        out.contains("Main.java"),
        "should find reference in Main.java: {out}"
    );
}

// ===========================================================================
// Java — callers
// ===========================================================================

#[test]
fn test_callers_java_finds_getname_callers() {
    let output = run_cq_project(&java_project(), &["callers", "getName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("caller"),
        "should find callers of getName: {out}"
    );
    assert!(
        out.contains("Main.java"),
        "should find call in Main.java: {out}"
    );
}

// ===========================================================================
// Java — deps
// ===========================================================================

#[test]
fn test_deps_java_finds_main_method_dependencies() {
    let output = run_cq_project(&java_project(), &["deps", "main"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("getName"),
        "main should depend on getName: {out}"
    );
    assert!(
        out.contains("User.java"),
        "getName should resolve to User.java: {out}"
    );
}

// ===========================================================================
// Java — search (raw S-expression)
// ===========================================================================

#[test]
fn test_search_java_finds_class_declarations() {
    let output = run_cq_project(
        &java_project(),
        &[
            "--raw",
            "search",
            "(class_declaration name: (identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Main"), "should find Main class: {out}");
    assert!(out.contains("User"), "should find User class: {out}");
}

// ===========================================================================
// JSON output validation — cross-language spot checks
// ===========================================================================

#[test]
fn test_json_def_cpp_produces_valid_json() {
    let output = run_cq_project(&cpp_project(), &["--json", "--pretty", "def", "Dog"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["definitions"].is_array(),
        "JSON should have definitions array"
    );
    let defs = json["definitions"].as_array().unwrap();
    assert!(!defs.is_empty(), "definitions should not be empty for Dog");
    // Check the first result has expected fields
    let first = &defs[0];
    assert!(
        first["name"].as_str().unwrap_or("") == "Dog",
        "first definition name should be Dog: {first}"
    );
    assert!(
        first["file"].as_str().unwrap_or("").contains("models.hpp"),
        "first definition file should contain models.hpp: {first}"
    );
}

#[test]
fn test_json_symbols_java_produces_valid_json() {
    let output = run_cq_project(&java_project(), &["--json", "--pretty", "symbols"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let text = stdout(&output);
    // Should contain User and Main
    assert!(
        text.contains("User") && text.contains("Main"),
        "JSON symbols should contain User and Main: {text}"
    );
}

#[test]
fn test_json_refs_go_produces_valid_json() {
    let output = run_cq_project(&go_project(), &["--json", "--pretty", "refs", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["definitions"].is_array() || json["results"].is_array(),
        "JSON refs should have definitions or results array"
    );
}

#[test]
fn test_json_callers_c_produces_valid_json() {
    let output = run_cq_project(&c_project(), &["--json", "--pretty", "callers", "add"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let text = stdout(&output);
    // Should contain definition and caller information
    assert!(
        text.contains("add") && text.contains("main.c"),
        "JSON callers should contain add and main.c: {text}"
    );
    assert!(
        json["definitions"].is_array(),
        "JSON callers should have definitions array: {json}"
    );
}
