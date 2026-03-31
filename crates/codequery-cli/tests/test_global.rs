mod common;

use common::{assert_exit_code, run_cq, stdout};

// Test 17: --version flag prints version and exits 0
#[test]
fn test_version_flag_prints_version() {
    let output = run_cq(&["--version"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("0.1.0"),
        "expected version number in output, got: {out}"
    );
}

// Test 18: --help flag prints usage and exits 0
#[test]
fn test_help_flag_prints_usage() {
    let output = run_cq(&["--help"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Semantic code query tool"),
        "expected tool description in help output, got: {out}"
    );
}

// Test 19: No arguments exits 2 (usage error)
#[test]
fn test_no_arguments_returns_usage_error() {
    let output = run_cq(&[]);
    assert_exit_code(&output, 2);
}

// Test 20: Unknown command exits 2 (usage error)
#[test]
fn test_unknown_command_returns_usage_error() {
    let output = run_cq(&["unknown"]);
    assert_exit_code(&output, 2);
}
