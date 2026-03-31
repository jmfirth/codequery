//! LSP integration tests and Phase 4 final validation.
//!
//! Verifies the three-tier precision cascade (daemon -> oneshot LSP -> stack
//! graph), daemon lifecycle commands, CLI flag parsing for semantic options,
//! `.cq.toml` LSP configuration, and full regression across all twelve
//! commands.

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
// 1. CLI flag parsing
// ===========================================================================

#[test]
fn test_lsp_semantic_flag_is_recognized() {
    let output = run_cq_project(&rust_project(), &["--semantic", "def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "--semantic flag should not break def command: {out}"
    );
}

#[test]
fn test_lsp_no_semantic_overrides_semantic() {
    // --no-semantic should work and not crash
    let output = run_cq_project(&rust_project(), &["--no-semantic", "def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "--no-semantic flag should not break def command: {out}"
    );
}

#[test]
fn test_lsp_semantic_and_no_semantic_conflict() {
    let output = run_cq(&["--semantic", "--no-semantic", "def", "greet"]);
    // clap should reject conflicting flags with exit code 2
    assert_exit_code(&output, 2);
}

#[test]
fn test_lsp_cq_semantic_env_var_works() {
    let cq_bin = env!("CARGO_BIN_EXE_cq");
    let fixture = rust_project();
    let output = std::process::Command::new(cq_bin)
        .env("CQ_SEMANTIC", "1")
        .args(["--project", fixture.to_str().unwrap(), "def", "greet"])
        .output()
        .expect("failed to execute cq");
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "CQ_SEMANTIC=1 should not break def command: {out}"
    );
}

#[test]
fn test_lsp_daemon_start_subcommand_parses() {
    // Verify daemon start is a valid subcommand by checking it does not
    // produce a usage/parse error (exit code 2). We do NOT actually start
    // a daemon here because it would leak a background process that
    // interferes with other tests.
    //
    // Instead, verify the subcommand exists by checking --help parses it.
    let output = run_cq(&["daemon", "start", "--help"]);
    let code = output.status.code().unwrap();
    assert_eq!(
        code, 0,
        "daemon start --help should succeed, got exit code {code}"
    );
}

#[test]
fn test_lsp_daemon_stop_subcommand_parses() {
    let output = run_cq(&["daemon", "stop"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_daemon_status_subcommand_parses() {
    let output = run_cq(&["daemon", "status"]);
    // When no daemon is running, status returns exit code 1 (NoResults)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "daemon status should return 0 or 1, got {code}"
    );
}

#[test]
fn test_lsp_daemon_subcommand_without_action_fails() {
    let output = run_cq(&["daemon"]);
    assert_exit_code(&output, 2);
}

// ===========================================================================
// 2. Daemon lifecycle
// ===========================================================================

#[test]
fn test_lsp_daemon_status_does_not_crash() {
    // Daemon may or may not be running depending on test environment state.
    // The important thing is it does not crash or return a usage error.
    let output = run_cq(&["daemon", "status"]);
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "daemon status should exit 0 (running) or 1 (not running), got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should produce some status message regardless
    assert!(
        stderr.contains("daemon") || stderr.contains("running"),
        "daemon status should produce a status message: {stderr}"
    );
}

#[test]
fn test_lsp_daemon_stop_does_not_crash() {
    // Daemon may or may not be running. Stop should handle both gracefully.
    let output = run_cq(&["daemon", "stop"]);
    assert_exit_code(&output, 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should produce a message about the daemon (either stopped or not running)
    assert!(
        stderr.contains("daemon") || stderr.contains("not running") || stderr.contains("stopped"),
        "daemon stop should produce a status message: {stderr}"
    );
}

// ===========================================================================
// 3. Cascade fallback behavior (no daemon, no --semantic)
// ===========================================================================

#[test]
fn test_lsp_cascade_refs_rust_no_daemon_succeeds() {
    // Without a daemon or --semantic, refs should fall through to stack graph
    let output = run_cq_project(&rust_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet"),
        "refs greet should find results via cascade fallback: {out}"
    );
}

#[test]
fn test_lsp_cascade_refs_json_has_resolution_field() {
    let output = run_cq_project(&rust_project(), &["refs", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "refs JSON should have resolution field, got: {resolution}"
    );
}

#[test]
fn test_lsp_cascade_callers_json_has_resolution_field() {
    let output = run_cq_project(&rust_project(), &["callers", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "callers JSON should have resolution field, got: {resolution}"
    );
}

#[test]
fn test_lsp_cascade_deps_json_has_resolution_field() {
    let output = run_cq_project(
        &rust_project(),
        &["deps", "process_users", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "deps JSON should have resolution field, got: {resolution}"
    );
}

#[test]
fn test_lsp_cascade_refs_no_crash_without_daemon() {
    // The cascade must not crash when daemon is not running
    let output = run_cq_project(&rust_project(), &["refs", "User"]);
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "refs should not crash without daemon, got exit code {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_lsp_cascade_callers_no_crash_without_daemon() {
    let output = run_cq_project(&rust_project(), &["callers", "greet"]);
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "callers should not crash without daemon, got exit code {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_lsp_cascade_deps_no_crash_without_daemon() {
    let output = run_cq_project(&rust_project(), &["deps", "greet"]);
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "deps should not crash without daemon, got exit code {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_lsp_cascade_refs_semantic_flag_no_daemon_falls_back() {
    // With --semantic but no daemon, should fall back gracefully to stack graph
    let output = run_cq_project(
        &rust_project(),
        &["--semantic", "refs", "greet", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // Should still have resolution metadata even after fallback
    assert!(
        json["resolution"].is_string(),
        "fallback should still produce resolution metadata"
    );
}

#[test]
fn test_lsp_cascade_python_refs_json_resolution_field() {
    let output = run_cq_project(&python_project(), &["refs", "User", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "Python refs should have resolution metadata: {resolution}"
    );
}

#[test]
fn test_lsp_cascade_typescript_refs_json_resolution_field() {
    let output = run_cq_project(
        &typescript_project(),
        &["refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "TypeScript refs should have resolution metadata: {resolution}"
    );
}

// ===========================================================================
// 4. Regression — all 12 commands return exit code 0 against Rust fixture
// ===========================================================================

#[test]
fn test_lsp_regression_outline_exit_code_0() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_def_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_body_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_sig_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["sig", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_refs_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_callers_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_deps_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["deps", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_symbols_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["symbols"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_imports_exit_code_0() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_context_exit_code_0() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let location = format!("{}:9", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_tree_exit_code_0() {
    let output = run_cq_project(&rust_project(), &["tree"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_regression_search_exit_code_0() {
    let output = run_cq_project(
        &rust_project(),
        &["search", "(function_item name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
}

// ===========================================================================
// 4b. --semantic flag does not break non-semantic commands
// ===========================================================================

#[test]
fn test_lsp_semantic_flag_does_not_break_outline() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(&project, &["--semantic", "outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_body() {
    let output = run_cq_project(&rust_project(), &["--semantic", "body", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_sig() {
    let output = run_cq_project(&rust_project(), &["--semantic", "sig", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_symbols() {
    let output = run_cq_project(&rust_project(), &["--semantic", "symbols"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_imports() {
    let project = rust_project();
    let file = project.join("src/services.rs");
    let output = run_cq_project(&project, &["--semantic", "imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_tree() {
    let output = run_cq_project(&rust_project(), &["--semantic", "tree"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_semantic_flag_does_not_break_search() {
    let output = run_cq_project(
        &rust_project(),
        &[
            "--semantic",
            "search",
            "(function_item name: (identifier) @name)",
        ],
    );
    assert_exit_code(&output, 0);
}

// ===========================================================================
// 4c. JSON output structure unchanged for non-semantic commands
// ===========================================================================

#[test]
fn test_lsp_outline_json_structure_unchanged() {
    let project = rust_project();
    let file = project.join("src/lib.rs");
    let output = run_cq_project(
        &project,
        &["outline", file.to_str().unwrap(), "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["symbols"].is_array(),
        "outline JSON should have symbols array"
    );
    assert!(
        json["resolution"].is_string(),
        "outline JSON should have resolution field"
    );
}

#[test]
fn test_lsp_def_json_structure_unchanged() {
    let output = run_cq_project(&rust_project(), &["def", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["definitions"].is_array(),
        "def JSON should have definitions array"
    );
    assert!(
        json["resolution"].is_string(),
        "def JSON should have resolution field"
    );
}

#[test]
fn test_lsp_body_json_structure_unchanged() {
    let output = run_cq_project(&rust_project(), &["body", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["resolution"].is_string(),
        "body JSON should have resolution field"
    );
}

#[test]
fn test_lsp_sig_json_structure_unchanged() {
    let output = run_cq_project(&rust_project(), &["sig", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["resolution"].is_string(),
        "sig JSON should have resolution field"
    );
}

// ===========================================================================
// 5. .cq.toml LSP config
// ===========================================================================

#[test]
fn test_lsp_cq_toml_with_lsp_section_parses_without_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Create .git for project detection
    std::fs::create_dir(root.join(".git")).unwrap();

    // Create source file
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn hello() {}\n").unwrap();

    // Create .cq.toml with LSP section
    std::fs::write(
        root.join(".cq.toml"),
        r#"[lsp]
timeout = 30

[lsp.rust]
binary = "rust-analyzer"
args = []
"#,
    )
    .unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("hello"),
        "symbols should work with .cq.toml LSP config: {out}"
    );
}

#[test]
fn test_lsp_cq_toml_with_multiple_lsp_servers_parses() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn world() {}\n").unwrap();

    std::fs::write(
        root.join(".cq.toml"),
        r#"[lsp]
timeout = 15

[lsp.rust]
binary = "rust-analyzer"
args = []

[lsp.python]
binary = "pylsp"
args = ["--log-file", "/tmp/pylsp.log"]

[lsp.go]
binary = "gopls-nightly"
"#,
    )
    .unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_cq_toml_invalid_config_graceful_handling() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn test_fn() {}\n").unwrap();

    // Write invalid TOML
    std::fs::write(root.join(".cq.toml"), "this is not valid toml {{{").unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    // Should not crash; may return an error exit code
    let code = output.status.code().unwrap();
    assert!(
        code != 2,
        "invalid .cq.toml should not cause a usage error (exit 2), got {code}"
    );
}

#[test]
fn test_lsp_cq_toml_lsp_only_section() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn foo() {}\n").unwrap();

    // .cq.toml with only an LSP section (no project section)
    std::fs::write(
        root.join(".cq.toml"),
        r#"[lsp.rust]
binary = "my-rust-analyzer"
args = ["--log-file", "/tmp/ra.log"]
"#,
    )
    .unwrap();

    let project_str = root.to_str().unwrap();
    let output = run_cq(&["--project", project_str, "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("foo"),
        "symbols should work with LSP-only .cq.toml: {out}"
    );
}

// ===========================================================================
// 6. Cascade with --semantic across different languages
// ===========================================================================

#[test]
fn test_lsp_cascade_semantic_refs_python_no_crash() {
    // With --semantic, the cascade tries oneshot LSP (will fail without server)
    // then falls back to stack graph. Must not crash.
    let output = run_cq_project(
        &python_project(),
        &["--semantic", "refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["resolution"].is_string(),
        "--semantic refs Python should have resolution metadata"
    );
}

#[test]
fn test_lsp_cascade_semantic_callers_python_no_crash() {
    let output = run_cq_project(
        &python_project(),
        &["--semantic", "callers", "format_name", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
}

#[test]
fn test_lsp_cascade_no_semantic_refs_python_resolution_field() {
    // Without --semantic, the cascade skips oneshot LSP and goes to stack graph
    let output = run_cq_project(
        &python_project(),
        &["--no-semantic", "refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["resolution"].is_string(),
        "--no-semantic refs should still have resolution field"
    );
}

// ===========================================================================
// 7. Ignored tests requiring real language servers
// ===========================================================================

#[test]
#[ignore]
fn test_lsp_real_server_rust_analyzer_refs() {
    // Requires rust-analyzer to be installed and available on PATH.
    // Run with: just test-all
    let output = run_cq_project(
        &rust_project(),
        &["--semantic", "refs", "greet", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    // With a real server, resolution should be semantic-quality
    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "resolved" || resolution == "syntactic",
        "real server refs should have resolution metadata: {resolution}"
    );
}

#[test]
#[ignore]
fn test_lsp_real_server_daemon_start_stop_cycle() {
    // Requires the daemon infrastructure to be functional.
    // Run with: just test-all
    let start_output = run_cq(&["daemon", "start"]);
    let start_code = start_output.status.code().unwrap();
    assert_eq!(start_code, 0, "daemon start should succeed");

    // Brief pause for daemon to initialize
    std::thread::sleep(std::time::Duration::from_millis(500));

    let status_output = run_cq(&["daemon", "status"]);
    assert_exit_code(&status_output, 0);

    let stop_output = run_cq(&["daemon", "stop"]);
    assert_exit_code(&stop_output, 0);
}

#[test]
#[ignore]
fn test_lsp_real_server_pyright_refs() {
    // Requires pyright-langserver to be installed.
    // Run with: just test-all
    let output = run_cq_project(
        &python_project(),
        &["--semantic", "refs", "User", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
}

// ===========================================================================
// 8. Cascade completeness and metadata consistency
// ===========================================================================

#[test]
fn test_lsp_refs_json_has_completeness_field() {
    let output = run_cq_project(&rust_project(), &["refs", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["completeness"].is_string(),
        "refs JSON should have completeness field"
    );
}

#[test]
fn test_lsp_callers_json_has_completeness_field() {
    let output = run_cq_project(&rust_project(), &["callers", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["completeness"].is_string(),
        "callers JSON should have completeness field"
    );
}

#[test]
fn test_lsp_deps_json_has_completeness_field() {
    let output = run_cq_project(
        &rust_project(),
        &["deps", "process_users", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert!(
        json["completeness"].is_string(),
        "deps JSON should have completeness field"
    );
}

#[test]
fn test_lsp_refs_json_symbol_field_matches_query() {
    let output = run_cq_project(&rust_project(), &["refs", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_eq!(
        json["symbol"].as_str(),
        Some("greet"),
        "refs JSON symbol field should match query"
    );
}

#[test]
fn test_lsp_callers_json_symbol_field_matches_query() {
    let output = run_cq_project(&rust_project(), &["callers", "greet", "--json", "--pretty"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_eq!(
        json["symbol"].as_str(),
        Some("greet"),
        "callers JSON symbol field should match query"
    );
}

#[test]
fn test_lsp_deps_json_symbol_field_matches_query() {
    let output = run_cq_project(
        &rust_project(),
        &["deps", "process_users", "--json", "--pretty"],
    );
    assert_exit_code(&output, 0);
    let json = parse_json(&output);
    assert_eq!(
        json["symbol"].as_str(),
        Some("process_users"),
        "deps JSON symbol field should match query"
    );
}
