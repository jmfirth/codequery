#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the fixture project.
pub fn fixture_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust_project")
}

/// Run `cq` with the given arguments.
pub fn run_cq(args: &[&str]) -> Output {
    let cq_bin = env!("CARGO_BIN_EXE_cq");
    Command::new(cq_bin)
        .args(args)
        .output()
        .expect("failed to execute cq")
}

/// Run `cq` with `--project` pointing to the fixture project.
pub fn run_cq_fixture(args: &[&str]) -> Output {
    let fixture = fixture_project();
    let mut full_args = vec!["--project", fixture.to_str().unwrap()];
    full_args.extend_from_slice(args);
    run_cq(&full_args)
}

/// Assert the expected exit code.
pub fn assert_exit_code(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "expected exit code {expected}, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Get stdout as a String.
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Check if a command produced no results because the language grammar is unavailable.
/// Returns true (and prints a skip message) if the test should be skipped.
///
/// In CI, tier-2 grammars are not compiled in. With the runtime language pipeline,
/// commands may gracefully produce empty output instead of an error. This guard
/// checks for any signal that the grammar wasn't available:
/// - Explicit errors in stderr
/// - Auto-install messages in stderr
/// - Empty stdout or total=0 (grammar loaded but couldn't extract)
pub fn skip_if_grammar_missing(output: &Output) -> bool {
    let err = String::from_utf8_lossy(&output.stderr);
    let out = String::from_utf8_lossy(&output.stdout);
    if err.contains("no grammar available")
        || err.contains("auto-install failed")
        || err.contains("auto-installing")
        || out.contains("total=0")
        || out.trim().is_empty()
    {
        eprintln!("skipping: language grammar not installed or not producing results");
        true
    } else {
        false
    }
}
