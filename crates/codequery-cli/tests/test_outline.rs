mod common;

use common::{assert_exit_code, fixture_project, run_cq_fixture, stdout};

// Test 1: Basic outline of lib.rs produces exit code 0 and file header
#[test]
fn test_outline_basic_lib_rs_exit_code_and_header() {
    let file = fixture_project().join("src/lib.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("@@ src/lib.rs @@"),
        "expected file header in output, got: {out}"
    );
}

// Test 2: Outline contains expected symbols (greet function, MAX_RETRIES const)
#[test]
fn test_outline_contains_expected_symbols() {
    let file = fixture_project().join("src/lib.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "expected greet function in output, got: {out}"
    );
    assert!(
        out.contains("MAX_RETRIES (const, pub)"),
        "expected MAX_RETRIES const in output, got: {out}"
    );
}

// Test 3: Outline shows module declarations
#[test]
fn test_outline_contains_module_declarations() {
    let file = fixture_project().join("src/lib.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("models (module, pub)"),
        "expected models module in output, got: {out}"
    );
    assert!(
        out.contains("traits (module, pub)"),
        "expected traits module in output, got: {out}"
    );
    assert!(
        out.contains("services (module, pub)"),
        "expected services module in output, got: {out}"
    );
}

// Test 4: Nested symbols — impl block with indented methods
#[test]
fn test_outline_nested_symbols_impl_with_methods() {
    let file = fixture_project().join("src/services.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // The impl block should exist
    assert!(
        out.contains("User (impl,"),
        "expected User impl block in output, got: {out}"
    );
    // Methods should be indented more than the impl block
    assert!(
        out.contains("    new (method, pub)"),
        "expected indented new method in output, got: {out}"
    );
    assert!(
        out.contains("    is_adult (method, pub)"),
        "expected indented is_adult method in output, got: {out}"
    );
}

// Test 5: Visibility mix — pub, priv symbols coexist
#[test]
fn test_outline_visibility_mix_pub_and_priv() {
    let file = fixture_project().join("src/lib.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // greet is pub
    assert!(
        out.contains("greet (function, pub)"),
        "expected pub greet in output, got: {out}"
    );
    // utils module is private
    assert!(
        out.contains("utils (module, priv)"),
        "expected priv utils module in output, got: {out}"
    );
}

// Test 6: Nonexistent file returns exit code 3
#[test]
fn test_outline_nonexistent_file_returns_project_error() {
    let file = fixture_project().join("src/nonexistent.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 3);
}

// Test 7: Doc comments don't break extraction — symbols with docs are found
#[test]
fn test_outline_doc_comments_do_not_break_extraction() {
    let file = fixture_project().join("src/lib.rs");
    let output = run_cq_fixture(&["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // greet has a doc comment; it should still appear
    assert!(
        out.contains("greet (function, pub)"),
        "greet with doc comment should still be found, got: {out}"
    );
    // MAX_RETRIES has a doc comment; it should still appear
    assert!(
        out.contains("MAX_RETRIES (const, pub)"),
        "MAX_RETRIES with doc comment should still be found, got: {out}"
    );
    // Also check traits.rs — Validate trait has a doc comment
    let file2 = fixture_project().join("src/traits.rs");
    let output2 = run_cq_fixture(&["outline", file2.to_str().unwrap()]);
    assert_exit_code(&output2, 0);
    let out2 = stdout(&output2);
    assert!(
        out2.contains("Validate (trait, pub)"),
        "Validate trait with doc comment should be found, got: {out2}"
    );
}
