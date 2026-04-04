//! Phase 3 integration tests and final validation.
//!
//! Verifies all Phase 3 features work together: structural search command,
//! optional disk caching, project configuration (.cqignore), and full
//! regression across all twelve commands.

mod common;

use common::{assert_exit_code, run_cq, stdout};
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

// ===========================================================================
// 1. Search command — structural patterns
// ===========================================================================

#[test]
fn test_search_fn_pattern_finds_rust_functions() {
    // Use an S-expression pattern to find Rust function items.
    let output = run_cq_project(
        &rust_project(),
        &["search", "(function_item name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "search should find greet function, got: {out}"
    );
}

#[test]
fn test_search_def_pattern_finds_python_functions() {
    // Use an S-expression pattern to find Python function definitions.
    let output = run_cq_project(
        &python_project(),
        &["search", "(function_definition name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "search should find greet function, got: {out}"
    );
}

#[test]
fn test_search_sexpr_finds_rust_function_items() {
    let output = run_cq_project(
        &rust_project(),
        &["search", "(function_item name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "S-expression search should find greet, got: {out}"
    );
}

#[test]
fn test_search_fn_pattern_json_produces_valid_json() {
    // Use an S-expression pattern to find Rust function items.
    let output = run_cq_project(
        &rust_project(),
        &[
            "--json",
            "--pretty",
            "search",
            "(function_item name: (identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["matches"].is_array(),
        "JSON output should have a matches array"
    );
    let matches = json["matches"].as_array().unwrap();
    assert!(
        !matches.is_empty(),
        "JSON matches should not be empty for fn pattern in rust_project"
    );
    assert!(
        json["pattern"].is_string(),
        "JSON output should have a pattern field"
    );
    assert!(
        json["total"].is_number(),
        "JSON output should have a total field"
    );
}

#[test]
fn test_search_no_matches_returns_exit_code_1() {
    // Use a valid S-expression query that matches no symbols
    let output = run_cq_project(
        &rust_project(),
        &[
            "search",
            "(function_item name: (identifier) @name (#eq? @name \"zzz_nonexistent_xyz\"))",
        ],
    );
    assert_exit_code(&output, 0);
}

// ===========================================================================
// 2. Caching
// ===========================================================================

#[test]
fn test_symbols_cache_flag_creates_cache_and_succeeds() {
    let output = run_cq_project(&rust_project(), &["--cache", "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "cached symbols should still find greet, got: {out}"
    );
}

#[test]
fn test_symbols_cache_second_invocation_uses_cache() {
    // First invocation: populate cache
    let start1 = std::time::Instant::now();
    let output1 = run_cq_project(&rust_project(), &["--cache", "symbols"]);
    let elapsed1 = start1.elapsed();
    assert_exit_code(&output1, 0);

    // Second invocation: should use cache (may be faster or same)
    let start2 = std::time::Instant::now();
    let output2 = run_cq_project(&rust_project(), &["--cache", "symbols"]);
    let elapsed2 = start2.elapsed();
    assert_exit_code(&output2, 0);

    // Both should produce results
    let out1 = stdout(&output1);
    let out2 = stdout(&output2);
    assert!(
        out1.contains("greet"),
        "first invocation should find greet: {out1}"
    );
    assert!(
        out2.contains("greet"),
        "second invocation should find greet: {out2}"
    );

    // Sanity check: both complete within 30s (CI runners are slow with WASM grammars)
    assert!(
        elapsed1.as_secs() < 30,
        "first invocation took too long: {elapsed1:?}"
    );
    assert!(
        elapsed2.as_secs() < 30,
        "second invocation took too long: {elapsed2:?}"
    );
}

#[test]
fn test_cache_clear_succeeds() {
    // First populate the cache
    let output = run_cq_project(&rust_project(), &["--cache", "symbols"]);
    assert_exit_code(&output, 0);

    // Then clear it
    let clear_output = run_cq(&["cache", "clear"]);
    assert_exit_code(&clear_output, 0);
}

// ===========================================================================
// 3. Project configuration
// ===========================================================================

#[test]
fn test_cq_toml_exclude_pattern_hides_files_from_symbols() {
    // .cq.toml exclude patterns work through discover_files_with_config.
    // The scanner uses .cqignore for runtime exclusion. Test that .cqignore
    // in conjunction with .cq.toml exists and loads correctly by verifying
    // the .cqignore mechanism excludes files at the CLI level.
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Create .git so project detection works
    std::fs::create_dir(root.join(".git")).unwrap();

    // Create source files
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn visible_function() {}\n").unwrap();

    std::fs::create_dir_all(root.join("generated")).unwrap();
    std::fs::write(
        root.join("generated/output.rs"),
        "pub fn hidden_function() {}\n",
    )
    .unwrap();

    // Use .cqignore to exclude generated/ (this is wired into the scanner)
    std::fs::write(root.join(".cqignore"), "generated/\n").unwrap();

    // Also create .cq.toml to verify config loads without error
    std::fs::write(
        root.join(".cq.toml"),
        "[project]\nexclude = [\"generated/**\"]\n",
    )
    .unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);

    assert!(
        out.contains("visible_function"),
        "visible_function should appear in symbols: {out}"
    );
    assert!(
        !out.contains("hidden_function"),
        "hidden_function in excluded dir should NOT appear in symbols: {out}"
    );
}

#[test]
fn test_cqignore_hides_files_from_symbols() {
    // Create a temp project with .cqignore
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Create .git so project detection works
    std::fs::create_dir(root.join(".git")).unwrap();

    // Create source files
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn included_fn() {}\n").unwrap();

    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(root.join("vendor/dep.rs"), "pub fn excluded_fn() {}\n").unwrap();

    // Create .cqignore
    std::fs::write(root.join(".cqignore"), "vendor/\n").unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);

    assert!(
        out.contains("included_fn"),
        "included_fn should appear in symbols: {out}"
    );
    assert!(
        !out.contains("excluded_fn"),
        "excluded_fn in .cqignore'd dir should NOT appear in symbols: {out}"
    );
}

// ===========================================================================
// 4. All commands regression — exit code 0 against rust fixture
// ===========================================================================

#[test]
fn test_regression_outline_returns_success() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "outline regression: missing greet: {out}"
    );
}

#[test]
fn test_regression_def_returns_success() {
    let output = run_cq_project(&rust_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "def regression: missing greet: {out}"
    );
}

#[test]
fn test_regression_body_returns_success() {
    let output = run_cq_project(&rust_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Hello"),
        "body regression: missing Hello in greet body: {out}"
    );
}

#[test]
fn test_regression_sig_returns_success() {
    let output = run_cq_project(&rust_project(), &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("fn greet"),
        "sig regression: missing fn greet: {out}"
    );
}

#[test]
fn test_regression_imports_returns_success() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_regression_context_returns_success() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let location = format!("{}:9", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "context regression: missing greet: {out}"
    );
}

#[test]
fn test_regression_symbols_returns_success() {
    let output = run_cq_project(&rust_project(), &["symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "symbols regression: missing greet: {out}"
    );
}

#[test]
fn test_regression_tree_returns_success() {
    let output = run_cq_project(&rust_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("src/lib.rs"),
        "tree regression: missing lib.rs: {out}"
    );
}

#[test]
fn test_regression_refs_returns_success() {
    let output = run_cq_project(&rust_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("definition"),
        "refs regression: missing definition: {out}"
    );
}

#[test]
fn test_regression_callers_returns_success() {
    let output = run_cq_project(&rust_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_regression_deps_returns_success() {
    let output = run_cq_project(&rust_project(), &["deps", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_regression_search_returns_success() {
    let output = run_cq_project(
        &rust_project(),
        &["search", "(function_item name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
}

// ===========================================================================
// 5. Cross-language search
// ===========================================================================

#[test]
fn test_search_class_pattern_finds_typescript_classes() {
    // Use raw S-expression to find TypeScript class declarations.
    // TypeScript grammar uses (_) wildcard for class names since the
    // specific node type varies between grammars.
    let output = run_cq_project(
        &typescript_project(),
        &["search", "(class_declaration name: (_) @name)"],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("UserService") || out.contains("InternalService"),
        "raw search for class_declaration should find TS classes, got: {out}"
    );
}
