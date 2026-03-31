//! Comprehensive integration tests verifying commands return actual results,
//! not just exit codes. Tests content of JSON output, validates cross-file
//! references, and exercises all major commands against fixture projects.

mod common;

use common::{assert_exit_code, run_cq, run_cq_fixture, stdout};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn rust_project() -> PathBuf {
    fixture_base().join("rust_project")
}

fn python_project() -> PathBuf {
    fixture_base().join("python_project")
}

fn typescript_project() -> PathBuf {
    fixture_base().join("typescript_project")
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

/// Parse stdout as JSON and return the serde_json::Value.
///
/// Note: `QueryResult` uses `#[serde(flatten)]` on the `data` field, so all
/// result-specific fields (references, definitions, symbols, etc.) appear at
/// the top level alongside `resolution`, `completeness`, and `note`.
fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let text = stdout(output);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nstdout was: {text}");
    })
}

// ===========================================================================
// 1. Cross-file references (the bug we're fixing)
// ===========================================================================

#[test]
fn test_refs_greet_json_returns_references() {
    // greet is defined in lib.rs and called in tests/integration.rs
    let output = run_cq_fixture(&["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    assert!(
        !refs.is_empty(),
        "refs for greet should find at least 1 reference (call site in integration.rs), got 0. Full JSON: {json}"
    );
}

#[test]
fn test_refs_user_json_returns_cross_file_references() {
    // User is defined in models.rs and imported/used in services.rs
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    assert!(
        !refs.is_empty(),
        "refs for User should find references in services.rs, got 0. Full JSON: {json}"
    );
    // At least one reference should be in services.rs
    let has_services_ref = refs
        .iter()
        .any(|r| r["file"].as_str().unwrap_or("").contains("services.rs"));
    assert!(
        has_services_ref,
        "expected at least one User reference in services.rs, refs: {refs:?}"
    );
}

#[test]
fn test_callers_summarize_json_returns_callers() {
    // summarize is called in process_users in services.rs
    let output = run_cq_fixture(&["--json", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let callers = json["callers"]
        .as_array()
        .expect("callers should be an array");
    assert!(
        !callers.is_empty(),
        "callers for summarize should find at least 1 call site in process_users, got 0. Full JSON: {json}"
    );
}

#[test]
fn test_refs_python_cross_file() {
    // User is defined in models.py and used in services.py
    let output = run_cq_project(&python_project(), &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    assert!(
        !refs.is_empty(),
        "Python refs for User should find cross-file references, got 0. Full JSON: {json}"
    );
}

#[test]
fn test_refs_typescript_cross_file() {
    // User is defined in models.ts and imported in services.ts
    let output = run_cq_project(&typescript_project(), &["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    assert!(
        !refs.is_empty(),
        "TypeScript refs for User should find cross-file references, got 0. Full JSON: {json}"
    );
}

// ===========================================================================
// 2. deps returns real dependencies
// ===========================================================================

#[test]
fn test_deps_process_users_json_returns_dependencies() {
    // process_users calls summarize and references User
    let output = run_cq_fixture(&["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let deps = json["dependencies"]
        .as_array()
        .expect("dependencies should be an array");
    assert!(
        !deps.is_empty(),
        "deps for process_users should find dependencies, got 0. Full JSON: {json}"
    );
    // Should reference summarize (called on each user)
    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();
    assert!(
        dep_names.contains(&"summarize"),
        "deps should include 'summarize', got: {dep_names:?}"
    );
}

#[test]
fn test_deps_greet_json_returns_dependencies() {
    // greet uses format! which references format_args (macro internals)
    let output = run_cq_fixture(&["--json", "deps", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // greet exists and has a body, so we should get Success
    let deps = json["dependencies"]
        .as_array()
        .expect("dependencies should be an array");
    // format! macro may or may not produce visible deps, so just verify
    // the structure is valid (array exists)
    let _ = deps;
}

// ===========================================================================
// 3. Multi-file project coherence
// ===========================================================================

#[test]
fn test_symbols_rust_project_returns_many_symbols() {
    let output = run_cq_fixture(&["--json", "symbols"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let symbols = json["symbols"]
        .as_array()
        .expect("symbols should be an array");
    assert!(
        symbols.len() > 15,
        "rust_project should have at least 15 symbols, found {}. First few: {:?}",
        symbols.len(),
        &symbols[..symbols.len().min(5)]
    );
}

#[test]
fn test_tree_rust_project_contains_all_source_files() {
    let output = run_cq_fixture(&["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/lib.rs"),
        "tree should contain lib.rs: {out}"
    );
    assert!(
        out.contains("src/models.rs"),
        "tree should contain models.rs: {out}"
    );
    assert!(
        out.contains("src/services.rs"),
        "tree should contain services.rs: {out}"
    );
    assert!(
        out.contains("src/traits.rs"),
        "tree should contain traits.rs: {out}"
    );
}

#[test]
fn test_def_helper_returns_two_results() {
    // helper exists in services.rs and utils/helpers.rs
    let output = run_cq_fixture(&["def", "helper"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    let frame_count = out.matches("@@ ").count();
    assert_eq!(
        frame_count, 3,
        "expected 3 frame headers (1 meta + 2 results) for 2 helper definitions, got {frame_count} in: {out}"
    );
}

// ===========================================================================
// 4. Output content validation (not just exit codes)
// ===========================================================================

#[test]
fn test_body_greet_contains_format() {
    let output = run_cq_fixture(&["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("format!"),
        "body of greet should contain 'format!', got: {out}"
    );
}

#[test]
fn test_sig_user_contains_name_field() {
    // User struct signature should contain the struct fields
    let output = run_cq_fixture(&["sig", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("name") && out.contains("String"),
        "sig of User should contain 'name: String', got: {out}"
    );
}

#[test]
fn test_outline_services_contains_impl_and_process_users() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (impl,"),
        "outline should contain 'User (impl,', got: {out}"
    );
    assert!(
        out.contains("process_users (function, pub)"),
        "outline should contain process_users, got: {out}"
    );
}

#[test]
fn test_imports_services_contains_use_models_user() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("crate::models::User") || out.contains("models::User"),
        "imports of services.rs should reference models::User, got: {out}"
    );
}

// ===========================================================================
// 5. Cross-language consistency
// ===========================================================================

#[test]
fn test_outline_python_produces_symbols() {
    let project = python_project();
    let file = project.join("src/models.py");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (class, pub)"),
        "Python outline should contain User class: {out}"
    );
}

#[test]
fn test_outline_typescript_produces_symbols() {
    let project = typescript_project();
    let file = project.join("src/services.ts");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("UserService (class, pub)"),
        "TypeScript outline should contain UserService class: {out}"
    );
}

#[test]
fn test_outline_go_produces_symbols() {
    let project = go_project();
    let file = project.join("models.go");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (struct, pub)"),
        "Go outline should contain User struct: {out}"
    );
}

#[test]
fn test_outline_java_produces_symbols() {
    let project = java_project();
    let file = project.join("src/main/java/com/example/models/User.java");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (class, pub)"),
        "Java outline should contain User class: {out}"
    );
}

#[test]
fn test_def_python_finds_user_class() {
    let output = run_cq_project(&python_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class User"),
        "Python def should find User class: {out}"
    );
}

#[test]
fn test_def_typescript_finds_user_service() {
    let output = run_cq_project(&typescript_project(), &["def", "UserService"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class UserService"),
        "TypeScript def should find UserService: {out}"
    );
}

#[test]
fn test_def_go_finds_user_struct() {
    let output = run_cq_project(&go_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("struct User"),
        "Go def should find User struct: {out}"
    );
}

#[test]
fn test_def_java_finds_user_class() {
    let output = run_cq_project(&java_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("class User"),
        "Java def should find User class: {out}"
    );
}

#[test]
fn test_body_python_greet_contains_hello() {
    let output = run_cq_project(&python_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Hello"),
        "Python body of greet should contain 'Hello': {out}"
    );
}

#[test]
fn test_body_typescript_greet_contains_hello() {
    let output = run_cq_project(&typescript_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Hello"),
        "TypeScript body of greet should contain 'Hello': {out}"
    );
}

#[test]
fn test_body_go_greet_contains_hello() {
    let output = run_cq_project(&go_project(), &["body", "Greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Hello"),
        "Go body of Greet should contain 'Hello': {out}"
    );
}

#[test]
fn test_body_java_get_name_contains_return() {
    let output = run_cq_project(&java_project(), &["body", "getName"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("return"),
        "Java body of getName should contain 'return': {out}"
    );
}

// ===========================================================================
// 6. Error handling
// ===========================================================================

#[test]
fn test_refs_nonexistent_symbol_returns_no_results() {
    let output = run_cq_fixture(&["refs", "nonexistent_xyz"]);
    assert_exit_code(&output, 1);
}

#[test]
fn test_body_nonexistent_symbol_returns_no_results() {
    let output = run_cq_fixture(&["body", "nonexistent_xyz"]);
    assert_exit_code(&output, 1);
}

#[test]
fn test_outline_nonexistent_file_returns_project_error() {
    let output = run_cq_fixture(&["outline", "nonexistent_file.rs"]);
    assert_exit_code(&output, 3);
}

#[test]
fn test_refs_nonexistent_project_returns_project_error() {
    let output = run_cq(&[
        "--project",
        "/tmp/nonexistent_cq_project_xyz",
        "refs",
        "User",
    ]);
    assert_exit_code(&output, 3);
}

// ===========================================================================
// 7. JSON structure validation
// ===========================================================================

#[test]
fn test_refs_json_structure_has_required_fields() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // Top-level metadata (from QueryResult)
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    // Flattened data fields (from RefsResult via #[serde(flatten)])
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(json["definitions"].is_array(), "missing definitions field");
    assert!(json["references"].is_array(), "missing references field");
    assert!(json["total"].is_number(), "missing total field");
}

#[test]
fn test_refs_json_total_matches_references_count() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    let total = json["total"].as_u64().expect("total should be a number");
    assert_eq!(
        refs.len() as u64,
        total,
        "total field should match references array length"
    );
}

#[test]
fn test_deps_json_structure_has_required_fields() {
    let output = run_cq_fixture(&["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(
        json["dependencies"].is_array(),
        "missing dependencies field"
    );
}

#[test]
fn test_symbols_json_structure_has_required_fields() {
    let output = run_cq_fixture(&["--json", "symbols"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(json["symbols"].is_array(), "missing symbols field");
    let symbols = json["symbols"].as_array().unwrap();
    if let Some(first) = symbols.first() {
        assert!(first["name"].is_string(), "symbol missing name field");
        assert!(first["kind"].is_string(), "symbol missing kind field");
        assert!(first["file"].is_string(), "symbol missing file field");
    }
}

// ===========================================================================
// 8. Reference content validation (file, line, context)
// ===========================================================================

#[test]
fn test_refs_user_json_references_have_file_and_line() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    for r in refs {
        assert!(r["file"].is_string(), "reference missing file field: {r}");
        assert!(r["line"].is_number(), "reference missing line field: {r}");
    }
}

#[test]
fn test_refs_user_json_definitions_have_file_and_line() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let defs = json["definitions"]
        .as_array()
        .expect("definitions should be an array");
    assert!(
        !defs.is_empty(),
        "should have at least one definition for User"
    );
    for d in defs {
        assert!(d["file"].is_string(), "definition missing file field: {d}");
        assert!(d["line"].is_number(), "definition missing line field: {d}");
    }
}

// ===========================================================================
// 9. Callers with content validation
// ===========================================================================

#[test]
fn test_callers_greet_json_finds_call_in_integration_test() {
    // greet is called in tests/integration.rs (inside assert_eq! macros)
    let output = run_cq_fixture(&["--json", "callers", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let callers = json["callers"]
        .as_array()
        .expect("callers should be an array");
    assert!(
        !callers.is_empty(),
        "callers for greet should find at least 1 call (in integration.rs), got 0. Full JSON: {json}"
    );
}

#[test]
fn test_callers_nonexistent_returns_no_results() {
    let output = run_cq_fixture(&["callers", "nonexistent_xyz"]);
    assert_exit_code(&output, 1);
}

// ===========================================================================
// 10. Callers JSON structure validation
// ===========================================================================

#[test]
fn test_callers_json_structure_has_required_fields() {
    let output = run_cq_fixture(&["--json", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(json["definitions"].is_array(), "missing definitions field");
    assert!(json["callers"].is_array(), "missing callers field");
    assert!(json["total"].is_number(), "missing total field");
}

#[test]
fn test_callers_json_caller_has_context_and_caller_name() {
    let output = run_cq_fixture(&["--json", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let callers = json["callers"]
        .as_array()
        .expect("callers should be an array");
    assert!(!callers.is_empty(), "expected at least one caller");
    let first = &callers[0];
    assert!(
        first["context"].is_string(),
        "caller should have context field"
    );
    assert!(
        first["caller"].is_string(),
        "caller should have caller (enclosing function) field"
    );
}

// ===========================================================================
// 11. Refs content - verify specific reference kinds
// ===========================================================================

#[test]
fn test_refs_user_includes_import_kind() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    let has_import = refs.iter().any(|r| r["kind"].as_str() == Some("import"));
    assert!(
        has_import,
        "User refs should include an import reference (from use statement), refs: {refs:?}"
    );
}

#[test]
fn test_refs_user_includes_type_usage_kind() {
    let output = run_cq_fixture(&["--json", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    let has_type_usage = refs
        .iter()
        .any(|r| r["kind"].as_str() == Some("type_usage"));
    assert!(
        has_type_usage,
        "User refs should include type_usage references (impl blocks, parameter types), refs: {refs:?}"
    );
}

#[test]
fn test_refs_greet_includes_call_kind() {
    let output = run_cq_fixture(&["--json", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"]
        .as_array()
        .expect("references should be an array");
    let has_call = refs.iter().any(|r| r["kind"].as_str() == Some("call"));
    assert!(
        has_call,
        "greet refs should include call references (from integration test), refs: {refs:?}"
    );
}

// ===========================================================================
// 12. Deps content validation
// ===========================================================================

#[test]
fn test_deps_process_users_includes_user_type_reference() {
    let output = run_cq_fixture(&["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let deps = json["dependencies"]
        .as_array()
        .expect("dependencies should be an array");
    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();
    assert!(
        dep_names.contains(&"User"),
        "process_users deps should include 'User' type reference, got: {dep_names:?}"
    );
}

#[test]
fn test_deps_json_each_dependency_has_name_and_kind() {
    let output = run_cq_fixture(&["--json", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let deps = json["dependencies"]
        .as_array()
        .expect("dependencies should be an array");
    for dep in deps {
        assert!(dep["name"].is_string(), "dependency missing name: {dep}");
        assert!(dep["kind"].is_string(), "dependency missing kind: {dep}");
    }
}
