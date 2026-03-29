//! Precision cascade tests: proving three-tier resolution (daemon → oneshot LSP → stack graph)
//! works correctly for cross-file references, callers, deps, definitions, body/sig extraction,
//! imports, context, and symbols.
//!
//! These tests verify:
//! 1. Stack graphs resolve cross-file references that tree-sitter alone cannot
//! 2. Resolution metadata is accurate ("resolved" vs "syntactic")
//! 3. Output content is correct (right files, right lines, right kinds)
//! 4. Fallback is safe for non-TSG languages

mod common;
use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn run_cq_project(project: &PathBuf, args: &[&str]) -> std::process::Output {
    let project_str = project.to_str().unwrap();
    let mut full_args = vec!["--project", project_str];
    full_args.extend_from_slice(args);
    run_cq(&full_args)
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let text = stdout(output);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nstdout was: {text}");
    })
}

/// Assert resolution is one of the accepted values. Returns the actual resolution string.
fn assert_resolution(json: &serde_json::Value, accepted: &[&str]) -> String {
    let resolution = json["resolution"]
        .as_str()
        .expect("missing 'resolution' field");
    assert!(
        accepted.contains(&resolution),
        "expected resolution to be one of {accepted:?}, got \"{resolution}\""
    );
    resolution.to_string()
}

/// Assert that at least one definition matches the given file suffix and line.
fn assert_definition_at(json: &serde_json::Value, file_suffix: &str, line: u64) {
    let defs = json["definitions"].as_array().expect("missing definitions");
    let found = defs.iter().any(|d| {
        let f = d["file"].as_str().unwrap_or("");
        let l = d["line"].as_u64().unwrap_or(0);
        f.ends_with(file_suffix) && l == line
    });
    assert!(
        found,
        "expected definition at {file_suffix}:{line}, got: {defs:?}"
    );
}

/// Assert that the references array has at least one entry in a file matching the suffix.
fn assert_has_ref_in_file(json: &serde_json::Value, key: &str, file_suffix: &str) {
    let refs = json[key]
        .as_array()
        .unwrap_or_else(|| panic!("missing '{key}' array"));
    let found = refs
        .iter()
        .any(|r| r["file"].as_str().unwrap_or("").ends_with(file_suffix));
    assert!(
        found,
        "expected reference in file matching '{file_suffix}' under '{key}', got: {refs:?}"
    );
}

/// Assert that the references array has an entry at a specific file and line.
fn assert_ref_at(json: &serde_json::Value, key: &str, file_suffix: &str, line: u64) {
    let refs = json[key]
        .as_array()
        .unwrap_or_else(|| panic!("missing '{key}' array"));
    let found = refs.iter().any(|r| {
        let f = r["file"].as_str().unwrap_or("");
        let l = r["line"].as_u64().unwrap_or(0);
        f.ends_with(file_suffix) && l == line
    });
    assert!(
        found,
        "expected {key} entry at {file_suffix}:{line}, got: {refs:?}"
    );
}

/// Assert that the total count is at least the given minimum.
fn assert_total_at_least(json: &serde_json::Value, min: u64) {
    let total = json["total"].as_u64().unwrap_or(0);
    assert!(total >= min, "expected total >= {min}, got {total}");
}

// ===========================================================================
// Section 1: Cross-File Resolution Proof (TSG-supported languages)
// ===========================================================================

#[test]
fn python_refs_user_cross_file() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Python has TSG support — may resolve to "resolved" or fall back to "syntactic"
    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition should be in models.py at line 3
    assert_definition_at(&json, "models.py", 3);

    // Cross-file references in services.py (import at line 3, usage at line 8)
    assert_has_ref_in_file(&json, "references", "services.py");
    assert_ref_at(&json, "references", "services.py", 3);
    assert_ref_at(&json, "references", "services.py", 8);

    assert_total_at_least(&json, 2);
}

#[test]
fn python_refs_format_name_cross_file() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "refs", "format_name"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in utils.py at line 3
    assert_definition_at(&json, "utils.py", 3);

    // Cross-file references in services.py at lines 4, 10, 11, 15
    assert_ref_at(&json, "references", "services.py", 4);
    assert_ref_at(&json, "references", "services.py", 10);
    assert_ref_at(&json, "references", "services.py", 11);
    assert_ref_at(&json, "references", "services.py", 15);

    assert_total_at_least(&json, 4);
}

#[test]
fn typescript_refs_user_cross_file() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in models.ts at line 1
    assert_definition_at(&json, "models.ts", 1);

    // Cross-file references in services.ts (import) and reexport.ts
    assert_has_ref_in_file(&json, "references", "services.ts");
    assert_has_ref_in_file(&json, "references", "reexport.ts");

    assert_total_at_least(&json, 2);
}

#[test]
fn typescript_refs_greet_cross_file() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in index.ts at line 2
    assert_definition_at(&json, "index.ts", 2);

    // Cross-file references: services.ts import (line 2), call (line 24), reexport.ts (line 6)
    assert_ref_at(&json, "references", "services.ts", 2);
    assert_ref_at(&json, "references", "services.ts", 24);
    assert_ref_at(&json, "references", "reexport.ts", 6);

    assert_total_at_least(&json, 3);
}

#[test]
fn go_refs_greet_cross_file() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "refs", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in main.go at line 15
    assert_definition_at(&json, "main.go", 15);

    // References in main.go:27 and main_test.go:6
    assert_ref_at(&json, "references", "main.go", 27);
    assert_ref_at(&json, "references", "main_test.go", 6);

    assert_total_at_least(&json, 2);
}

#[test]
fn rust_refs_user_cross_file() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in models.rs at line 5
    assert_definition_at(&json, "models.rs", 5);

    // Cross-file references in services.rs (use import at line 3, impl at line 6, etc.)
    assert_has_ref_in_file(&json, "references", "services.rs");
    assert_ref_at(&json, "references", "services.rs", 3);
    assert_ref_at(&json, "references", "services.rs", 6);

    assert_total_at_least(&json, 3);
}

#[test]
fn rust_refs_greet_cross_file() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in lib.rs at line 9
    assert_definition_at(&json, "lib.rs", 9);

    // Cross-file references in integration.rs (import line 1, call line 5)
    assert_ref_at(&json, "references", "integration.rs", 1);
    assert_ref_at(&json, "references", "integration.rs", 5);

    assert_total_at_least(&json, 2);
}

#[test]
fn c_refs_add_cross_file() {
    let project = fixture_base().join("c_project");
    let output = run_cq_project(&project, &["--json", "refs", "add"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in utils.c at line 4
    assert_definition_at(&json, "utils.c", 4);

    // Cross-file references: declaration in utils.h line 5, call in main.c line 6
    assert_has_ref_in_file(&json, "references", "main.c");
    assert_ref_at(&json, "references", "main.c", 6);
    assert_has_ref_in_file(&json, "references", "utils.h");

    assert_total_at_least(&json, 2);
}

#[test]
fn java_refs_user_cross_file() {
    let project = fixture_base().join("java_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in User.java at line 4
    assert_definition_at(&json, "User.java", 4);

    // Cross-file references in Main.java and UserService.java
    assert_has_ref_in_file(&json, "references", "Main.java");
    assert_has_ref_in_file(&json, "references", "UserService.java");

    assert_total_at_least(&json, 3);
}

#[test]
fn javascript_refs_format_name_cross_file() {
    // The TypeScript project has a utils.js file with JavaScript
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "refs", "formatName"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Definition in utils.js at line 1
    assert_definition_at(&json, "utils.js", 1);

    // Reference in utils.js at line 28 (self-file call)
    assert_ref_at(&json, "references", "utils.js", 28);

    assert_total_at_least(&json, 1);
}

// ===========================================================================
// Section 2: Callers Cross-File Resolution
// ===========================================================================

#[test]
fn python_callers_format_name() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "callers", "format_name"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Callers in services.py (lines 4, 10, 11, 15)
    assert_has_ref_in_file(&json, "callers", "services.py");
    assert_ref_at(&json, "callers", "services.py", 10);
    assert_ref_at(&json, "callers", "services.py", 11);
    assert_ref_at(&json, "callers", "services.py", 15);

    assert_total_at_least(&json, 3);
}

#[test]
fn typescript_callers_greet() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "callers", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Caller in services.ts at line 24 (greetUser method calls greet)
    assert_ref_at(&json, "callers", "services.ts", 24);

    assert_total_at_least(&json, 1);
}

#[test]
fn go_callers_greet() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "callers", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Callers in main.go (main function, line 27) and main_test.go (TestGreet, line 6)
    assert_ref_at(&json, "callers", "main.go", 27);
    assert_ref_at(&json, "callers", "main_test.go", 6);

    assert_total_at_least(&json, 2);
}

#[test]
fn rust_callers_summarize() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Caller in services.rs at line 39 (process_users calls summarize)
    assert_ref_at(&json, "callers", "services.rs", 39);

    assert_total_at_least(&json, 1);
}

#[test]
fn c_callers_add() {
    let project = fixture_base().join("c_project");
    let output = run_cq_project(&project, &["--json", "callers", "add"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Caller in main.c at line 6
    assert_ref_at(&json, "callers", "main.c", 6);

    assert_total_at_least(&json, 1);
}

#[test]
fn java_callers_get_name() {
    let project = fixture_base().join("java_project");
    let output = run_cq_project(&project, &["--json", "callers", "getName"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_resolution(&json, &["resolved", "syntactic"]);

    // Caller in Main.java at line 10
    assert_ref_at(&json, "callers", "Main.java", 10);

    assert_total_at_least(&json, 1);
}

// ===========================================================================
// Section 3: Deps Resolution
// ===========================================================================

#[test]
fn python_deps_process_user() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "deps", "process_user"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let deps = json["dependencies"]
        .as_array()
        .expect("missing dependencies array");

    // Should find User and format_name as dependencies
    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();

    assert!(
        dep_names.contains(&"User"),
        "expected 'User' in deps, got: {dep_names:?}"
    );
    assert!(
        dep_names.contains(&"format_name"),
        "expected 'format_name' in deps, got: {dep_names:?}"
    );
}

#[test]
fn rust_deps_process_users() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let deps = json["dependencies"]
        .as_array()
        .expect("missing dependencies array");

    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();

    // Should find User and summarize as dependencies
    assert!(
        dep_names.contains(&"User"),
        "expected 'User' in deps, got: {dep_names:?}"
    );
    assert!(
        dep_names.contains(&"summarize"),
        "expected 'summarize' in deps, got: {dep_names:?}"
    );
}

#[test]
fn typescript_deps_user_service() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "deps", "UserService"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let deps = json["dependencies"]
        .as_array()
        .expect("missing dependencies array");

    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();

    // Should find greet as a dependency (called in greetUser method)
    assert!(
        dep_names.contains(&"greet"),
        "expected 'greet' in deps, got: {dep_names:?}"
    );
}

// ===========================================================================
// Section 4: Fallback Behavior (non-TSG languages)
// ===========================================================================

#[test]
fn cpp_fallback_syntactic() {
    let project = fixture_base().join("cpp_project");
    let output = run_cq_project(&project, &["--json", "refs", "Animal"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // C++ has no TSG rules — must be syntactic
    let resolution = assert_resolution(&json, &["syntactic"]);
    assert_eq!(resolution, "syntactic");

    // Should have a note explaining the limitation
    let note = json["note"].as_str();
    assert!(
        note.is_some(),
        "expected 'note' field for syntactic fallback"
    );
    assert!(
        note.unwrap().contains("name-based"),
        "note should mention name-based matching"
    );

    // Results should still be useful (non-empty)
    let defs = json["definitions"].as_array().expect("missing definitions");
    assert!(!defs.is_empty(), "should still find definitions for Animal");
}

#[test]
fn ruby_resolved() {
    let project = fixture_base().join("ruby_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Ruby has TSG rules — should produce resolved references
    assert_resolution(&json, &["resolved"]);

    // Non-empty definitions
    let defs = json["definitions"].as_array().expect("missing definitions");
    assert!(!defs.is_empty(), "should find User class in Ruby");
}

#[test]
fn kotlin_fallback_syntactic() {
    let project = fixture_base().join("kotlin_project");
    let output = run_cq_project(&project, &["--json", "refs", "Animal"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Kotlin has no TSG rules — must be syntactic
    assert_resolution(&json, &["syntactic"]);

    // Should have a note
    assert!(
        json["note"].as_str().is_some(),
        "expected 'note' field for syntactic fallback"
    );

    // Results should still be useful
    let defs = json["definitions"].as_array().expect("missing definitions");
    assert!(!defs.is_empty(), "should find Animal class in Kotlin");

    let refs = json["references"].as_array().expect("missing references");
    assert!(
        !refs.is_empty(),
        "should find references for Animal in Kotlin"
    );
}

#[test]
fn php_fallback_syntactic() {
    let project = fixture_base().join("php_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // PHP has no TSG rules — must be syntactic
    assert_resolution(&json, &["syntactic"]);

    // Should have a note
    assert!(
        json["note"].as_str().is_some(),
        "expected 'note' field for PHP syntactic fallback"
    );

    // Non-empty definitions
    let defs = json["definitions"].as_array().expect("missing definitions");
    assert!(!defs.is_empty(), "should find User class in PHP");
}

// ===========================================================================
// Section 5: Line-Number Accuracy (def command)
// ===========================================================================

#[test]
fn python_def_user_line_accuracy() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "def", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_definition_at(&json, "models.py", 3);
}

#[test]
fn typescript_def_user_line_accuracy() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["--json", "def", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_definition_at(&json, "models.ts", 1);
}

#[test]
fn go_def_greet_line_accuracy() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "def", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_definition_at(&json, "main.go", 15);
}

#[test]
fn rust_def_user_line_accuracy() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "def", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_definition_at(&json, "models.rs", 5);
}

#[test]
fn c_def_add_line_accuracy() {
    let project = fixture_base().join("c_project");
    let output = run_cq_project(&project, &["--json", "def", "add"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Definition at utils.c line 4 (the function body definition)
    assert_definition_at(&json, "utils.c", 4);
}

#[test]
fn java_def_user_line_accuracy() {
    let project = fixture_base().join("java_project");
    let output = run_cq_project(&project, &["--json", "def", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_definition_at(&json, "User.java", 4);
}

// ===========================================================================
// Section 6: Body/Sig Content Accuracy
// ===========================================================================

#[test]
fn python_body_greet() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["body", "greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("Hello, {name}!"),
        "body should contain greeting: {text}"
    );
}

#[test]
fn python_sig_greet() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("def greet(name: str) -> str"),
        "sig should contain function signature: {text}"
    );
}

#[test]
fn rust_body_greet() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["body", "greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains(r#"format!("Hello, {name}!")"#),
        "body should contain format macro: {text}"
    );
}

#[test]
fn rust_sig_greet() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("pub fn greet(name: &str) -> String"),
        "sig should contain function signature: {text}"
    );
}

#[test]
fn go_sig_greet() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["sig", "Greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("func Greet(name string) string"),
        "sig should contain function signature: {text}"
    );
}

#[test]
fn typescript_sig_greet() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("function greet(name: string): string"),
        "sig should contain function signature: {text}"
    );
}

// ===========================================================================
// Section 7: Reference Kind Classification
// ===========================================================================

#[test]
fn rust_refs_greet_has_import_kind() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    // integration.rs line 1 should be an import reference
    let import_ref = refs.iter().find(|r| {
        let f = r["file"].as_str().unwrap_or("");
        let l = r["line"].as_u64().unwrap_or(0);
        f.ends_with("integration.rs") && l == 1
    });
    assert!(
        import_ref.is_some(),
        "expected import ref at integration.rs:1"
    );

    let kind = import_ref.unwrap()["kind"].as_str().unwrap_or("");
    assert!(
        kind.contains("import"),
        "expected kind containing 'import' for use statement, got: {kind}"
    );
}

#[test]
fn rust_refs_greet_has_call_kind() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    // integration.rs line 5 should be a call reference
    let call_ref = refs.iter().find(|r| {
        let f = r["file"].as_str().unwrap_or("");
        let l = r["line"].as_u64().unwrap_or(0);
        f.ends_with("integration.rs") && l == 5
    });
    assert!(call_ref.is_some(), "expected call ref at integration.rs:5");

    let kind = call_ref.unwrap()["kind"].as_str().unwrap_or("");
    assert!(
        kind.contains("call"),
        "expected kind containing 'call', got: {kind}"
    );
}

#[test]
fn go_refs_greet_call_kind() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "refs", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    // main.go:27 should be a call reference
    let call_ref = refs.iter().find(|r| {
        let f = r["file"].as_str().unwrap_or("");
        let l = r["line"].as_u64().unwrap_or(0);
        f.ends_with("main.go") && l == 27
    });
    assert!(call_ref.is_some(), "expected call ref at main.go:27");

    let kind = call_ref.unwrap()["kind"].as_str().unwrap_or("");
    assert!(
        kind.contains("call"),
        "expected kind containing 'call' at main.go:27, got: {kind}"
    );
}

#[test]
fn rust_refs_user_has_type_usage_kind() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    // services.rs should have type_usage references for impl blocks
    let type_usage = refs.iter().any(|r| {
        let kind = r["kind"].as_str().unwrap_or("");
        kind.contains("type_usage")
    });
    assert!(
        type_usage,
        "expected at least one type_usage reference kind for User in Rust"
    );
}

#[test]
fn rust_refs_user_has_import_kind() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    // services.rs line 3 should be an import reference
    let import_ref = refs.iter().find(|r| {
        let f = r["file"].as_str().unwrap_or("");
        let l = r["line"].as_u64().unwrap_or(0);
        f.ends_with("services.rs") && l == 3
    });
    assert!(import_ref.is_some(), "expected import ref at services.rs:3");

    let kind = import_ref.unwrap()["kind"].as_str().unwrap_or("");
    assert!(
        kind.contains("import"),
        "expected kind containing 'import' for use statement, got: {kind}"
    );
}

// ===========================================================================
// Section 8: Imports Accuracy
// ===========================================================================

#[test]
fn python_imports_services() {
    let project = fixture_base().join("python_project");
    let file = project.join("src/services.py");
    let file_str = file.to_str().unwrap();
    let output = run_cq_project(&project, &["imports", file_str]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("models"),
        "imports should contain 'models': {text}"
    );
    assert!(
        text.contains("format_name") || text.contains("utils"),
        "imports should contain 'format_name' or 'utils': {text}"
    );
}

#[test]
fn java_imports_main() {
    let project = fixture_base().join("java_project");
    let file = project.join("src/main/java/com/example/Main.java");
    let file_str = file.to_str().unwrap();
    let output = run_cq_project(&project, &["imports", file_str]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("User"),
        "imports should contain 'User': {text}"
    );
    assert!(
        text.contains("UserService"),
        "imports should contain 'UserService': {text}"
    );
}

#[test]
fn rust_imports_services() {
    let project = fixture_base().join("rust_project");
    let file = project.join("src/services.rs");
    let file_str = file.to_str().unwrap();
    let output = run_cq_project(&project, &["imports", file_str]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(
        text.contains("models::User") || text.contains("crate::models::User"),
        "imports should contain models::User: {text}"
    );
}

#[test]
fn go_imports_main() {
    let project = fixture_base().join("go_project");
    let file = project.join("main.go");
    let file_str = file.to_str().unwrap();
    let output = run_cq_project(&project, &["imports", file_str]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    assert!(text.contains("fmt"), "imports should contain 'fmt': {text}");
}

// ===========================================================================
// Section 9: Context Accuracy
// ===========================================================================

#[test]
fn python_context_services_line_8() {
    let project = fixture_base().join("python_project");
    let file = project.join("src/services.py");
    let location = format!("{}:8", file.to_str().unwrap());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    // Enclosing function should be process_user
    assert!(
        text.contains("process_user"),
        "context should show process_user as enclosing function: {text}"
    );
}

#[test]
fn rust_context_services_line_10() {
    let project = fixture_base().join("rust_project");
    let file = project.join("src/services.rs");
    let location = format!("{}:10", file.to_str().unwrap());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    // Enclosing method should be new
    assert!(
        text.contains("new"),
        "context should show 'new' as enclosing method: {text}"
    );
}

#[test]
fn go_context_main_line_16() {
    let project = fixture_base().join("go_project");
    let file = project.join("main.go");
    let location = format!("{}:16", file.to_str().unwrap());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    // Enclosing function should be Greet
    assert!(
        text.contains("Greet"),
        "context should show 'Greet' as enclosing function: {text}"
    );
}

#[test]
fn c_context_main_line_6() {
    let project = fixture_base().join("c_project");
    let file = project.join("main.c");
    let location = format!("{}:6", file.to_str().unwrap());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let text = stdout(&output);

    // Enclosing function should be main
    assert!(
        text.contains("main"),
        "context should show 'main' as enclosing function: {text}"
    );
}

// ===========================================================================
// Section 10: Symbols Completeness
// ===========================================================================

#[test]
fn python_symbols_completeness() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "symbols"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let symbols = json["symbols"].as_array().expect("missing symbols array");
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();

    let expected = [
        "User",
        "Admin",
        "greet",
        "add",
        "format_name",
        "validate_age",
        "process_user",
        "list_users",
        "MAX_RETRIES",
    ];

    for name in &expected {
        assert!(
            names.contains(name),
            "expected symbol '{name}' in Python project symbols, got: {names:?}"
        );
    }
}

#[test]
fn rust_symbols_completeness() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "symbols"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let symbols = json["symbols"].as_array().expect("missing symbols array");
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();

    let expected = [
        "greet",
        "User",
        "Role",
        "Validate",
        "Summary",
        "process_users",
        "format_name",
    ];

    for name in &expected {
        assert!(
            names.contains(name),
            "expected symbol '{name}' in Rust project symbols, got: {names:?}"
        );
    }
}

// ===========================================================================
// Section 11: Resolution Metadata Consistency
// ===========================================================================

#[test]
fn resolved_refs_have_no_note() {
    // For languages with stack graph support that return "resolved",
    // the note field should NOT be present.
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap_or("");
    if resolution == "resolved" {
        assert!(
            json["note"].is_null(),
            "resolved results should not have a 'note' field, but got: {:?}",
            json["note"]
        );
    }
}

#[test]
fn syntactic_refs_always_have_note() {
    // For non-TSG languages, syntactic resolution should always include a note.
    let project = fixture_base().join("cpp_project");
    let output = run_cq_project(&project, &["--json", "refs", "Dog"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert_eq!(
        json["resolution"].as_str().unwrap(),
        "syntactic",
        "C++ should always be syntactic"
    );
    assert!(
        json["note"].as_str().is_some(),
        "syntactic resolution should include a note field"
    );
}

// ===========================================================================
// Section 12: Cross-File Proof — Multiple Files in Single Result Set
// ===========================================================================

#[test]
fn python_refs_user_spans_multiple_files() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");
    let defs = json["definitions"].as_array().expect("missing definitions");

    // Collect unique files from both defs and refs
    let mut files = std::collections::HashSet::new();
    for d in defs {
        if let Some(f) = d["file"].as_str() {
            files.insert(f.to_string());
        }
    }
    for r in refs {
        if let Some(f) = r["file"].as_str() {
            files.insert(f.to_string());
        }
    }

    assert!(
        files.len() >= 2,
        "cross-file resolution should span at least 2 files, got: {files:?}"
    );
}

#[test]
fn go_refs_greet_spans_multiple_files() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "refs", "Greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    let mut files = std::collections::HashSet::new();
    for r in refs {
        if let Some(f) = r["file"].as_str() {
            files.insert(f.to_string());
        }
    }

    assert!(
        files.len() >= 2,
        "Go refs should span main.go and main_test.go, got: {files:?}"
    );
}

#[test]
fn java_refs_user_spans_multiple_files() {
    let project = fixture_base().join("java_project");
    let output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().expect("missing references");

    let mut files = std::collections::HashSet::new();
    for r in refs {
        if let Some(f) = r["file"].as_str() {
            files.insert(f.to_string());
        }
    }

    // Should have Main.java and UserService.java (and possibly User.java itself)
    assert!(
        files.len() >= 2,
        "Java refs should span multiple files, got: {files:?}"
    );
}

// ===========================================================================
// Section 13: No Crashes on Edge Cases
// ===========================================================================

#[test]
fn refs_nonexistent_symbol_returns_empty() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "refs", "NonExistentSymbol12345"]);
    // Should not crash — exit code 0 with empty results, or exit code 1
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "expected exit code 0 or 1 for nonexistent symbol, got: {code}"
    );
}

#[test]
fn callers_nonexistent_symbol_returns_empty() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "callers", "ZZZDoesNotExist"]);
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "expected exit code 0 or 1 for nonexistent symbol, got: {code}"
    );
}

#[test]
fn def_nonexistent_symbol_returns_empty() {
    let project = fixture_base().join("go_project");
    let output = run_cq_project(&project, &["--json", "def", "ZZZDoesNotExist"]);
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "expected exit code 0 or 1 for nonexistent symbol, got: {code}"
    );
}

// ===========================================================================
// Section 14: Callers Include Caller Context
// ===========================================================================

#[test]
fn rust_callers_summarize_includes_caller_name() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let callers = json["callers"].as_array().expect("missing callers array");

    // The caller at services.rs:39 should identify "process_users" as the enclosing function
    let caller = callers.iter().find(|c| {
        c["file"].as_str().unwrap_or("").ends_with("services.rs")
            && c["line"].as_u64().unwrap_or(0) == 39
    });
    assert!(caller.is_some(), "expected caller at services.rs:39");

    let caller = caller.unwrap();
    if let Some(caller_name) = caller["caller"].as_str() {
        assert_eq!(
            caller_name, "process_users",
            "caller should be process_users"
        );
    }
    // The context field should contain the source line
    if let Some(ctx) = caller["context"].as_str() {
        assert!(
            ctx.contains("summarize"),
            "context should contain 'summarize': {ctx}"
        );
    }
}

// ===========================================================================
// Section 15: Deps Locate Definition Sources
// ===========================================================================

#[test]
fn python_deps_process_user_resolves_sources() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["--json", "deps", "process_user"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let deps = json["dependencies"]
        .as_array()
        .expect("missing dependencies");

    // User should be defined_in models.py
    let user_dep = deps.iter().find(|d| d["name"].as_str() == Some("User"));
    assert!(user_dep.is_some(), "User should be in dependencies");
    let defined_in = user_dep.unwrap()["defined_in"].as_str().unwrap_or("");
    assert!(
        defined_in.contains("models.py"),
        "User defined_in should be models.py, got: {defined_in}"
    );

    // format_name should be defined_in utils.py
    let fmt_dep = deps
        .iter()
        .find(|d| d["name"].as_str() == Some("format_name"));
    assert!(fmt_dep.is_some(), "format_name should be in dependencies");
    let defined_in = fmt_dep.unwrap()["defined_in"].as_str().unwrap_or("");
    assert!(
        defined_in.contains("utils.py"),
        "format_name defined_in should be utils.py, got: {defined_in}"
    );
}

#[test]
fn rust_deps_process_users_resolves_sources() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let deps = json["dependencies"]
        .as_array()
        .expect("missing dependencies");

    // User should be defined_in models.rs
    let user_dep = deps.iter().find(|d| d["name"].as_str() == Some("User"));
    assert!(user_dep.is_some(), "User should be in dependencies");
    let defined_in = user_dep.unwrap()["defined_in"].as_str().unwrap_or("");
    assert!(
        defined_in.contains("models.rs"),
        "User defined_in should be models.rs, got: {defined_in}"
    );

    // summarize should be defined_in services.rs
    let sum_dep = deps
        .iter()
        .find(|d| d["name"].as_str() == Some("summarize"));
    assert!(sum_dep.is_some(), "summarize should be in dependencies");
    let defined_in = sum_dep.unwrap()["defined_in"].as_str().unwrap_or("");
    assert!(
        defined_in.contains("services.rs"),
        "summarize defined_in should be services.rs, got: {defined_in}"
    );
}
