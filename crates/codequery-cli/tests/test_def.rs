mod common;

use common::{assert_exit_code, run_cq_fixture, stdout};

// Test 8: Find unique symbol — greet function
#[test]
fn test_def_find_unique_function_greet() {
    let output = run_cq_fixture(&["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "expected 'function greet' in output, got: {out}"
    );
}

// Test 9: Find struct — User
#[test]
fn test_def_find_struct_user() {
    let output = run_cq_fixture(&["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("struct User"),
        "expected 'struct User' in output, got: {out}"
    );
}

// Test 10: Find trait — Validate
#[test]
fn test_def_find_trait_validate() {
    let output = run_cq_fixture(&["def", "Validate"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("trait Validate"),
        "expected 'trait Validate' in output, got: {out}"
    );
}

// Test 11: Find method — is_adult
#[test]
fn test_def_find_method_is_adult() {
    let output = run_cq_fixture(&["def", "is_adult"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("method is_adult"),
        "expected 'method is_adult' in output, got: {out}"
    );
}

// Test 12: Multiple matches — helper exists in services.rs and utils/helpers.rs
#[test]
fn test_def_multiple_matches_helper() {
    let output = run_cq_fixture(&["def", "helper"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/services.rs"),
        "expected services.rs match in output, got: {out}"
    );
    assert!(
        out.contains("src/utils/helpers.rs"),
        "expected utils/helpers.rs match in output, got: {out}"
    );
    // Verify there are two result frame headers (plus 1 meta header)
    let frame_count = out.matches("@@ ").count();
    assert_eq!(
        frame_count, 3,
        "expected 3 frame headers (1 meta + 2 results) for 2 matches, got {frame_count} in: {out}"
    );
}

// Test 13: Symbol not found — exit code 1, empty stdout
#[test]
fn test_def_symbol_not_found_returns_no_results() {
    let output = run_cq_fixture(&["def", "nonexistent_xyz"]);
    assert_exit_code(&output, 1);
    let out = stdout(&output);
    assert!(
        out.is_empty(),
        "expected empty stdout for not-found symbol, got: {out}"
    );
}

// Test 14: Scoped search — `cq def helper --in src/utils` finds only utils result
#[test]
fn test_def_scoped_search_limits_to_subdirectory() {
    let output = run_cq_fixture(&["--in", "src/utils", "def", "helper"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/utils/helpers.rs"),
        "expected utils/helpers.rs match in scoped output, got: {out}"
    );
    assert!(
        !out.contains("src/services.rs"),
        "scoped search should NOT include services.rs, got: {out}"
    );
    // One result frame header (plus 1 meta header)
    let frame_count = out.matches("@@ ").count();
    assert_eq!(
        frame_count, 2,
        "expected 2 frame headers (1 meta + 1 result) for scoped search, got {frame_count} in: {out}"
    );
}

// Test 15: Find const — MAX_RETRIES
#[test]
fn test_def_find_const_max_retries() {
    let output = run_cq_fixture(&["def", "MAX_RETRIES"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("const MAX_RETRIES"),
        "expected 'const MAX_RETRIES' in output, got: {out}"
    );
}

// Test 16: Find enum — Role
#[test]
fn test_def_find_enum_role() {
    let output = run_cq_fixture(&["def", "Role"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("enum Role"),
        "expected 'enum Role' in output, got: {out}"
    );
}
