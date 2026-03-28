//! Resolution precision integration tests for Phase 2.
//!
//! These tests verify that stack-graph-resolved results carry correct resolution
//! metadata in JSON output, that cross-language resolution works for supported
//! languages, that unsupported languages fall back gracefully to syntactic, and
//! that existing Phase 1 commands remain unaffected.

mod common;

use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn python_project() -> PathBuf {
    fixture_base().join("python_project")
}

fn rust_project() -> PathBuf {
    fixture_base().join("rust_project")
}

fn typescript_project() -> PathBuf {
    fixture_base().join("typescript_project")
}

fn cpp_project() -> PathBuf {
    fixture_base().join("cpp_project")
}

fn mixed_project() -> PathBuf {
    fixture_base().join("mixed_project")
}

fn go_project() -> PathBuf {
    fixture_base().join("go_project")
}

fn c_project() -> PathBuf {
    fixture_base().join("c_project")
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
fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let text = stdout(output);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nstdout was: {text}");
    })
}

/// Assert that a JSON value has a specific string field value.
fn assert_json_field(json: &serde_json::Value, field: &str, expected: &str) {
    assert_eq!(
        json[field].as_str(),
        Some(expected),
        "expected {field}={expected:?}, got: {:?}\nfull JSON: {}",
        json[field],
        serde_json::to_string_pretty(json).unwrap_or_default()
    );
}

/// Assert that a JSON value has a field that is not present or is null.
fn assert_json_field_absent_or_null(json: &serde_json::Value, field: &str) {
    assert!(
        json.get(field).is_none() || json[field].is_null(),
        "expected {field} to be absent or null, got: {:?}",
        json[field]
    );
}

// ===========================================================================
// 1. Resolution metadata verification
// ===========================================================================

#[test]
fn test_resolution_refs_python_user_shows_resolved() {
    // Python has TSG rules; refs for User (with cross-file references) should
    // produce "resolution": "resolved" in JSON output.
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "resolved");
    assert_json_field(&json, "completeness", "best_effort");
    // When resolved, there should be no note about syntactic matching.
    assert_json_field_absent_or_null(&json, "note");
}

#[test]
fn test_resolution_refs_python_format_name_shows_resolved() {
    // format_name is imported cross-file in services.py — resolved path.
    let output = run_cq_project(
        &python_project(),
        &["refs", "format_name", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "resolved");
}

#[test]
fn test_resolution_refs_rust_greet_shows_resolution_metadata() {
    // Rust has TSG rules but resolution quality depends on the rule coverage.
    // The important thing is that the JSON contains a "resolution" field.
    let output = run_cq_project(&rust_project(), &["refs", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // Must have a resolution field (either "resolved" or "syntactic").
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution to be 'resolved' or 'syntactic', got: {resolution}"
    );
}

#[test]
fn test_resolution_callers_python_format_name_shows_resolved() {
    // callers for format_name (called cross-file from services.py).
    let output = run_cq_project(
        &python_project(),
        &["callers", "format_name", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "resolved");
    // Should have actual callers from services.py.
    let callers = json["callers"].as_array().unwrap();
    assert!(
        !callers.is_empty(),
        "expected callers from services.py, got none"
    );
    // All caller files should reference services.py (the file that imports and calls).
    for caller in callers {
        let file = caller["file"].as_str().unwrap();
        assert!(
            file.contains("services.py"),
            "expected caller in services.py, got: {file}"
        );
    }
}

#[test]
fn test_resolution_deps_python_process_user_shows_resolution_metadata() {
    // deps command with stack graph resolution.
    let output = run_cq_project(
        &python_project(),
        &["deps", "process_user", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // Must have a resolution field.
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata, got: {resolution}"
    );
    // Should have dependencies.
    let deps = json["dependencies"].as_array().unwrap();
    assert!(
        !deps.is_empty(),
        "expected dependencies for process_user, got none"
    );
}

#[test]
fn test_resolution_deps_rust_process_users_shows_resolution_metadata() {
    // deps command against Rust fixture.
    let output = run_cq_project(
        &rust_project(),
        &["deps", "process_users", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata, got: {resolution}"
    );
}

#[test]
fn test_resolution_deps_per_dependency_resolution_field() {
    // Each dependency in the deps output should carry its own resolution field.
    let output = run_cq_project(
        &python_project(),
        &["deps", "process_user", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let deps = json["dependencies"].as_array().unwrap();
    for dep in deps {
        let dep_resolution = dep["resolution"].as_str().unwrap();
        assert!(
            dep_resolution == "resolved" || dep_resolution == "syntactic",
            "expected per-dep resolution field, got: {dep_resolution} for dep: {}",
            dep["name"]
        );
    }
}

// ===========================================================================
// 2. Cross-language resolution
// ===========================================================================

#[test]
fn test_resolution_python_cross_file_import_resolved() {
    // services.py imports User from models.py. Refs for User should find
    // cross-file references and resolve them.
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "resolved");
    let refs = json["references"].as_array().unwrap();
    // Should have references from services.py (the cross-file import user).
    let has_services_ref = refs
        .iter()
        .any(|r| r["file"].as_str().unwrap().contains("services.py"));
    assert!(
        has_services_ref,
        "expected cross-file reference from services.py, got refs: {refs:?}"
    );
}

#[test]
fn test_resolution_typescript_refs_user_has_metadata() {
    // TypeScript has TSG rules. Refs for User should carry resolution metadata.
    let output = run_cq_project(
        &typescript_project(),
        &["refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata for TS, got: {resolution}"
    );
}

#[test]
fn test_resolution_go_refs_has_metadata() {
    // Go has TSG rules.
    let output = run_cq_project(&go_project(), &["refs", "Greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata for Go, got: {resolution}"
    );
}

#[test]
fn test_resolution_c_refs_has_metadata() {
    // C has TSG rules.
    let output = run_cq_project(&c_project(), &["refs", "add", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata for C, got: {resolution}"
    );
}

#[test]
fn test_resolution_java_refs_has_metadata() {
    // Java has TSG rules.
    let output = run_cq_project(&java_project(), &["refs", "Main", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "expected resolution metadata for Java, got: {resolution}"
    );
}

// ===========================================================================
// 3. Regression tests — Phase 1 commands unaffected
// ===========================================================================

#[test]
fn test_resolution_outline_unaffected_by_phase2() {
    // outline command should still work and always report "syntactic" resolution.
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(
        &project,
        &["outline", file.to_str().unwrap(), "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
    assert_json_field(&json, "completeness", "exhaustive");
    // Symbols should still be present.
    let symbols = json["symbols"].as_array().unwrap();
    assert!(!symbols.is_empty(), "outline should still produce symbols");
}

#[test]
fn test_resolution_def_unaffected_by_phase2() {
    // def command should still work and report "syntactic" resolution.
    let output = run_cq_project(&rust_project(), &["def", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
    assert_json_field(&json, "completeness", "exhaustive");
}

#[test]
fn test_resolution_body_unaffected_by_phase2() {
    // body command should still work with syntactic resolution.
    let output = run_cq_project(&rust_project(), &["body", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
}

#[test]
fn test_resolution_sig_unaffected_by_phase2() {
    // sig command should still work with syntactic resolution.
    let output = run_cq_project(&rust_project(), &["sig", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
}

#[test]
fn test_resolution_framed_output_unchanged_for_non_resolution_commands() {
    // Framed output for outline/def should be unchanged from Phase 1.
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // Should have standard @@ file @@ framed format.
    assert!(
        out.contains("@@ src/lib.rs @@"),
        "framed output should be unchanged: {out}"
    );
    assert!(
        out.contains("greet (function, pub)"),
        "framed outline should still list symbols: {out}"
    );
}

#[test]
fn test_resolution_def_framed_output_unchanged() {
    // Framed def output should be unchanged.
    let output = run_cq_project(&rust_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("@@ src/lib.rs:"),
        "framed def output should be unchanged: {out}"
    );
    assert!(
        out.contains("function greet"),
        "framed def output should contain kind and name: {out}"
    );
}

#[test]
fn test_resolution_outline_python_unaffected() {
    // Python outline should still work identically to Phase 1.
    let project = python_project();
    let file = project.join("src/main.py");
    let output = run_cq_project(
        &project,
        &["outline", file.to_str().unwrap(), "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
    let symbols = json["symbols"].as_array().unwrap();
    let has_greet = symbols.iter().any(|s| s["name"].as_str() == Some("greet"));
    assert!(has_greet, "Python outline should still find greet");
}

// ===========================================================================
// 4. Performance sanity
// ===========================================================================

#[test]
fn test_resolution_refs_python_completes_within_5s() {
    // Performance sanity: refs with resolution should complete reasonably fast.
    let start = Instant::now();
    let output = run_cq_project(&python_project(), &["refs", "User"]);
    let elapsed = start.elapsed();
    assert_exit_code(&output, 0);
    assert!(
        elapsed.as_secs() < 5,
        "refs command took too long: {elapsed:?} (expected < 5s)"
    );
}

#[test]
fn test_resolution_callers_python_completes_within_5s() {
    let start = Instant::now();
    let output = run_cq_project(&python_project(), &["callers", "format_name"]);
    let elapsed = start.elapsed();
    assert_exit_code(&output, 0);
    assert!(
        elapsed.as_secs() < 5,
        "callers command took too long: {elapsed:?} (expected < 5s)"
    );
}

#[test]
fn test_resolution_deps_python_completes_within_5s() {
    let start = Instant::now();
    let output = run_cq_project(&python_project(), &["deps", "process_user"]);
    let elapsed = start.elapsed();
    assert_exit_code(&output, 0);
    assert!(
        elapsed.as_secs() < 5,
        "deps command took too long: {elapsed:?} (expected < 5s)"
    );
}

// ===========================================================================
// 5. Fallback behavior
// ===========================================================================

#[test]
fn test_resolution_cpp_falls_back_to_syntactic() {
    // C++ has no TSG rules — should fall back to syntactic gracefully.
    let output = run_cq_project(&cpp_project(), &["refs", "Dog", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
    // Should have the note about name-based matching.
    let note = json["note"].as_str();
    assert!(note.is_some(), "syntactic fallback should include a note");
    assert!(
        note.unwrap().contains("name-based"),
        "note should mention name-based matching: {:?}",
        note
    );
}

#[test]
fn test_resolution_cpp_callers_falls_back_gracefully() {
    // C++ callers should not crash; should fall back to syntactic.
    let output = run_cq_project(&cpp_project(), &["callers", "speak", "--json", "--pretty"]);
    // May or may not find results, but should not crash.
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "C++ callers should exit 0 or 1, got {code}"
    );
}

#[test]
fn test_resolution_cpp_refs_does_not_crash() {
    // C++ refs should work without crashing even though no TSG rules exist.
    let output = run_cq_project(
        &cpp_project(),
        &["refs", "free_function", "--json", "--pretty"],
    );
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "C++ refs should not crash, got exit code {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_resolution_cpp_deps_does_not_crash() {
    // C++ deps should work without crashing.
    let output = run_cq_project(&cpp_project(), &["deps", "main", "--json", "--pretty"]);
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "C++ deps should not crash, got exit code {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_resolution_mixed_project_refs_graceful_with_multiple_languages() {
    // Mixed project has Python (TSG rules), Rust (TSG rules), C (TSG rules),
    // and Go (TSG rules). The resolution metadata should reflect the overall
    // resolution quality (syntactic if any language falls back).
    let output = run_cq_project(&mixed_project(), &["refs", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "mixed project refs should have valid resolution metadata: {resolution}"
    );
}

// ===========================================================================
// 6. Cross-file import resolution quality (Python)
// ===========================================================================

#[test]
fn test_resolution_python_cross_file_callers_find_actual_call_sites() {
    // format_name is defined in utils.py, called from services.py.
    // Stack graph resolution should find the actual call sites.
    let output = run_cq_project(
        &python_project(),
        &["callers", "format_name", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let callers = json["callers"].as_array().unwrap();
    // With the services.py fixture, there are 3 call sites for format_name.
    assert!(
        callers.len() >= 2,
        "expected at least 2 callers for format_name, got {}: {callers:?}",
        callers.len()
    );
}

#[test]
fn test_resolution_python_refs_user_finds_cross_module_usage() {
    // User is defined in models.py, used in services.py (via import).
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let refs = json["references"].as_array().unwrap();
    // Should have references from multiple files.
    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    let has_models = files.iter().any(|f| f.contains("models.py"));
    let has_services = files.iter().any(|f| f.contains("services.py"));
    assert!(
        has_models || has_services,
        "expected refs from models.py or services.py, got files: {files:?}"
    );
}

// ===========================================================================
// 7. Resolution metadata in framed output for cross-ref commands
// ===========================================================================

#[test]
fn test_resolution_refs_framed_output_includes_summary() {
    // Framed refs output should include a summary line indicating resolution quality.
    let output = run_cq_project(&python_project(), &["refs", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    // The framed output should contain either "resolved" or "syntactic" in the summary.
    let has_resolution_summary = out.contains("resolved") || out.contains("syntactic");
    assert!(
        has_resolution_summary,
        "framed refs output should mention resolution quality: {out}"
    );
}

#[test]
fn test_resolution_callers_framed_output_includes_summary() {
    let output = run_cq_project(&python_project(), &["callers", "format_name"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    let has_resolution_summary = out.contains("resolved") || out.contains("syntactic");
    assert!(
        has_resolution_summary,
        "framed callers output should mention resolution quality: {out}"
    );
}

// ===========================================================================
// 8. Resolution for TypeScript re-exports
// ===========================================================================

#[test]
fn test_resolution_typescript_reexport_file_parseable() {
    // The re-export fixture should be parseable and accessible via outline.
    let project = typescript_project();
    let file = project.join("src/reexport.ts");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_resolution_typescript_refs_with_reexport_has_metadata() {
    // TypeScript refs should carry resolution metadata even with re-exports.
    let output = run_cq_project(
        &typescript_project(),
        &["refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "TS refs with re-exports should have resolution metadata: {resolution}"
    );
}

// ===========================================================================
// 9. JSON output structure validation
// ===========================================================================

#[test]
fn test_resolution_json_refs_has_required_fields() {
    // Verify the JSON structure for refs command with resolution.
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // Required top-level fields.
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(json["definitions"].is_array(), "missing definitions array");
    assert!(json["references"].is_array(), "missing references array");
    assert!(json["total"].is_number(), "missing total field");
}

#[test]
fn test_resolution_json_callers_has_required_fields() {
    let output = run_cq_project(
        &python_project(),
        &["callers", "format_name", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(json["definitions"].is_array(), "missing definitions array");
    assert!(json["callers"].is_array(), "missing callers array");
    assert!(json["total"].is_number(), "missing total field");
}

#[test]
fn test_resolution_json_deps_has_required_fields() {
    let output = run_cq_project(
        &python_project(),
        &["deps", "process_user", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(
        json["dependencies"].is_array(),
        "missing dependencies array"
    );
    assert!(json["total"].is_number(), "missing total field");
}

#[test]
fn test_resolution_json_no_results_still_has_metadata() {
    // Even when no results are found, JSON output should have resolution metadata.
    let output = run_cq_project(
        &python_project(),
        &["refs", "nonexistent_symbol_xyz_42", "--json", "--pretty"],
    );
    // Exit code 1 for no results is fine.
    let json = parse_json(&output);
    assert!(
        json["resolution"].is_string(),
        "no-results JSON should still have resolution field"
    );
}

// ===========================================================================
// 10. Resolved vs. syntactic precision comparison
// ===========================================================================

#[test]
fn test_resolution_resolved_note_absent_for_python() {
    // When resolution is "resolved", the note field should be absent (no
    // disclaimer about false positives needed).
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    if json["resolution"].as_str() == Some("resolved") {
        assert_json_field_absent_or_null(&json, "note");
    }
}

#[test]
fn test_resolution_syntactic_note_present_for_cpp() {
    // When resolution is "syntactic", a note about name-based matching should
    // be present.
    let output = run_cq_project(&cpp_project(), &["refs", "Dog", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_json_field(&json, "resolution", "syntactic");
    assert!(
        json["note"].is_string(),
        "syntactic resolution should include a note"
    );
}
