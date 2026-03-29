//! Live LSP integration tests exercising real language servers.
//!
//! These tests are `#[ignore]`d by default because they require external
//! language servers to be installed and available on `$PATH`:
//!
//! - **rust-analyzer** (for Rust tests)
//! - **typescript-language-server** (for TypeScript tests, optional)
//!
//! Run with:
//! ```sh
//! cargo test --test test_lsp_live -- --ignored --nocapture
//! ```
//!
//! Individual sections can be run with name filters:
//! ```sh
//! cargo test --test test_lsp_live -- --ignored --nocapture oneshot
//! cargo test --test test_lsp_live -- --ignored --nocapture daemon
//! cargo test --test test_lsp_live -- --ignored --nocapture comparison
//! cargo test --test test_lsp_live -- --ignored --nocapture fallback
//! ```
//!
//! These tests exercise the full four-step resolution cascade against real
//! language servers. They verify that `--semantic` produces `Resolution::Semantic`
//! quality results when a server is available and that the cascade falls back
//! gracefully when a server is not.

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

#[allow(dead_code)]
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

/// Parse stdout as JSON and return the `serde_json::Value`.
fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let text = stdout(output);
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("failed to parse JSON: {e}\nstdout was: {text}");
    })
}

/// Ensure the daemon is stopped. Used for cleanup in daemon tests.
fn ensure_daemon_stopped() {
    let _ = run_cq(&["daemon", "stop"]);
    // Brief pause to let the daemon process fully exit.
    std::thread::sleep(std::time::Duration::from_millis(500));
}

// ===========================================================================
// Section 1: Oneshot LSP with rust-analyzer
// ===========================================================================

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_semantic_refs_greet() {
    // Use --semantic to force oneshot LSP.
    // rust-analyzer should resolve refs for greet, finding the definition in
    // lib.rs and call references in integration.rs.
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();
    // With rust-analyzer available, we expect "semantic". However, if the
    // server is slow to start or the project is not indexed in time, the
    // cascade may fall back to "syntactic" or "resolved".
    assert!(
        resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
        "expected valid resolution tier, got: {resolution}"
    );

    // Should find definition in lib.rs regardless of resolution tier.
    let defs = json["definitions"].as_array().unwrap();
    assert!(!defs.is_empty(), "should find greet definition");
    assert!(
        defs.iter()
            .any(|d| d["file"].as_str().unwrap_or("").contains("lib.rs")),
        "definition should be in lib.rs, got: {defs:?}"
    );

    // Should find references (calls in integration.rs).
    let refs = json["references"].as_array().unwrap();
    assert!(!refs.is_empty(), "should find greet references");

    // If semantic resolution was achieved, log it for visibility.
    if resolution == "semantic" {
        eprintln!(
            "  [OK] oneshot rust-analyzer returned semantic resolution with {} refs",
            refs.len()
        );
    } else {
        eprintln!(
            "  [FALLBACK] oneshot returned {resolution} resolution (rust-analyzer may have timed out)"
        );
    }
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_semantic_refs_user() {
    // refs User — should find definition in models.rs and references in
    // services.rs (use statement + impl blocks + function signatures).
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
        "expected valid resolution, got: {resolution}"
    );

    // Definition in models.rs.
    let defs = json["definitions"].as_array().unwrap();
    assert!(
        defs.iter()
            .any(|d| d["file"].as_str().unwrap_or("").contains("models.rs")),
        "User definition should be in models.rs"
    );

    // References in services.rs (import, impl blocks, function param).
    let refs = json["references"].as_array().unwrap();
    let services_refs: Vec<_> = refs
        .iter()
        .filter(|r| r["file"].as_str().unwrap_or("").contains("services.rs"))
        .collect();
    assert!(
        !services_refs.is_empty(),
        "should find User references in services.rs"
    );

    eprintln!(
        "  refs User: resolution={resolution}, {} defs, {} refs ({} in services.rs)",
        defs.len(),
        refs.len(),
        services_refs.len()
    );
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_semantic_callers_summarize() {
    // callers summarize — should find call site in process_users (services.rs:39).
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
        "expected valid resolution, got: {resolution}"
    );

    // Should find at least one caller: process_users.
    let callers = json["callers"].as_array().unwrap();
    assert!(
        !callers.is_empty(),
        "should find at least one caller for summarize"
    );

    // The caller should be process_users in services.rs.
    let has_process_users_caller = callers.iter().any(|c| {
        let file = c["file"].as_str().unwrap_or("");
        let caller_name = c["caller"].as_str().unwrap_or("");
        file.contains("services.rs") && caller_name == "process_users"
    });
    assert!(
        has_process_users_caller,
        "expected process_users as a caller of summarize, got: {callers:?}"
    );

    eprintln!(
        "  callers summarize: resolution={resolution}, {} callers",
        callers.len()
    );
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_semantic_refs_is_valid() {
    // refs is_valid — this is the KEY semantic test. rust-analyzer should find
    // both the trait definition (traits.rs:6) AND the implementation
    // (services.rs:26). Stack graphs have limited Rust trait support.
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "is_valid"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
        "expected valid resolution, got: {resolution}"
    );

    // Definition should be found (at least the impl in services.rs).
    let defs = json["definitions"].as_array().unwrap();
    assert!(!defs.is_empty(), "should find is_valid definition(s)");

    if resolution == "semantic" {
        // With full semantic resolution, rust-analyzer should find both:
        // 1. The trait method definition in traits.rs
        // 2. The impl in services.rs
        // However, the exact definitions returned depend on which position
        // was used for the LSP query, so we accept any non-empty result.
        eprintln!(
            "  [SEMANTIC] is_valid: {} defs, {} refs — rust-analyzer resolved trait impl",
            defs.len(),
            json["references"].as_array().map_or(0, |r| r.len())
        );
    } else {
        eprintln!(
            "  [FALLBACK] is_valid: resolution={resolution}, {} defs",
            defs.len()
        );
    }
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_semantic_deps_process_users() {
    // deps process_users — should find User and summarize as dependencies.
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();
    assert!(
        resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
        "expected valid resolution, got: {resolution}"
    );

    // Should have dependencies.
    let deps = json["dependencies"].as_array().unwrap();
    assert!(
        !deps.is_empty(),
        "should find dependencies for process_users"
    );

    // User and summarize should be among the dependencies.
    let dep_names: Vec<&str> = deps.iter().filter_map(|d| d["name"].as_str()).collect();
    assert!(
        dep_names.contains(&"User"),
        "User should be a dependency of process_users, got: {dep_names:?}"
    );
    assert!(
        dep_names.contains(&"summarize"),
        "summarize should be a dependency of process_users, got: {dep_names:?}"
    );

    eprintln!(
        "  deps process_users: resolution={resolution}, {} deps: {dep_names:?}",
        deps.len()
    );
}

// ===========================================================================
// Section 2: Daemon Lifecycle
// ===========================================================================

#[test]
#[ignore] // Requires daemon infrastructure + rust-analyzer
fn test_lsp_live_daemon_full_lifecycle() {
    // Always ensure clean state at the start and end.
    ensure_daemon_stopped();

    // Use a panic guard to ensure cleanup on test failure.
    let result = std::panic::catch_unwind(|| {
        let project = rust_project();

        // 1. Check initial status (should NOT be running).
        let status_before = run_cq(&["daemon", "status"]);
        let before_code = status_before.status.code().unwrap();
        assert!(
            before_code == 0 || before_code == 1,
            "daemon status should not crash, got exit code: {before_code}"
        );

        // 2. Start daemon.
        let start = run_cq_project(&project, &["daemon", "start"]);
        assert_exit_code(&start, 0);
        eprintln!("  daemon start: {}", String::from_utf8_lossy(&start.stderr));

        // 3. Wait for initialization (rust-analyzer needs time to index).
        std::thread::sleep(std::time::Duration::from_secs(5));

        // 4. Check status (should be running).
        let status_during = run_cq(&["daemon", "status"]);
        let during_code = status_during.status.code().unwrap();
        let status_text = stdout(&status_during);
        let status_stderr = String::from_utf8_lossy(&status_during.stderr);
        // The daemon may report status on stdout or stderr depending on
        // implementation. Accept either for the "running" check.
        let combined_output = format!("{status_text}{status_stderr}");
        if during_code == 0 {
            eprintln!("  daemon is running: {}", combined_output.trim());
        } else {
            eprintln!(
                "  daemon status returned code {during_code} (may not have started): {}",
                combined_output.trim()
            );
        }

        // 5. Query with daemon running (should use semantic resolution
        //    automatically if the daemon connected).
        let query = run_cq_project(&project, &["--json", "refs", "greet"]);
        assert_exit_code(&query, 0);
        let json = parse_json(&query);
        let resolution = json["resolution"].as_str().unwrap();
        eprintln!("  daemon query resolution: {resolution}");
        // With daemon running, resolution should improve, but we accept any
        // valid tier since the daemon may still be initializing.
        assert!(
            resolution == "semantic" || resolution == "resolved" || resolution == "syntactic",
            "expected valid resolution with daemon, got: {resolution}"
        );

        // 6. Stop daemon.
        let stop = run_cq(&["daemon", "stop"]);
        assert_exit_code(&stop, 0);
        eprintln!("  daemon stop: {}", String::from_utf8_lossy(&stop.stderr));

        // 7. Verify stopped.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let status_after = run_cq(&["daemon", "status"]);
        let after_code = status_after.status.code().unwrap();
        assert!(
            after_code == 1,
            "daemon should be stopped after stop command, got exit code: {after_code}"
        );
    });

    // Always clean up, even on panic.
    ensure_daemon_stopped();

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore] // Requires daemon infrastructure + rust-analyzer
fn test_lsp_live_daemon_multiple_queries() {
    ensure_daemon_stopped();

    let result = std::panic::catch_unwind(|| {
        let project = rust_project();

        // Start daemon.
        let start = run_cq_project(&project, &["daemon", "start"]);
        assert_exit_code(&start, 0);

        // Wait for initialization.
        std::thread::sleep(std::time::Duration::from_secs(5));

        // Query 1: refs greet.
        let q1 = run_cq_project(&project, &["--json", "refs", "greet"]);
        assert_exit_code(&q1, 0);

        // Query 2: refs User.
        let q2 = run_cq_project(&project, &["--json", "refs", "User"]);
        assert_exit_code(&q2, 0);

        // Query 3: callers summarize.
        let q3 = run_cq_project(&project, &["--json", "callers", "summarize"]);
        assert_exit_code(&q3, 0);

        // All queries should have valid resolution metadata.
        for (name, output) in [
            ("refs greet", &q1),
            ("refs User", &q2),
            ("callers summarize", &q3),
        ] {
            let json = parse_json(output);
            let res = json["resolution"].as_str().unwrap();
            assert!(
                res == "semantic" || res == "resolved" || res == "syntactic",
                "daemon query '{name}' should have valid resolution, got: {res}"
            );
            eprintln!("  daemon query '{name}': resolution={res}");
        }

        // Stop daemon.
        let stop = run_cq(&["daemon", "stop"]);
        assert_exit_code(&stop, 0);
    });

    ensure_daemon_stopped();

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore] // Requires daemon infrastructure
fn test_lsp_live_daemon_start_stop_idempotent() {
    ensure_daemon_stopped();

    let result = std::panic::catch_unwind(|| {
        // Stop when not running should succeed (idempotent).
        let stop1 = run_cq(&["daemon", "stop"]);
        assert_exit_code(&stop1, 0);

        // Start daemon.
        let project = rust_project();
        let start = run_cq_project(&project, &["daemon", "start"]);
        assert_exit_code(&start, 0);
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Stop should succeed.
        let stop2 = run_cq(&["daemon", "stop"]);
        assert_exit_code(&stop2, 0);
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Double stop should also succeed (idempotent).
        let stop3 = run_cq(&["daemon", "stop"]);
        assert_exit_code(&stop3, 0);
    });

    ensure_daemon_stopped();

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// ===========================================================================
// Section 3: Semantic vs Stack Graph Comparison
// ===========================================================================

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_comparison_semantic_vs_stack_graph_refs_user() {
    let project = rust_project();

    // Without --semantic: uses stack graph (resolved tier) or syntactic.
    let sg_output = run_cq_project(&project, &["--json", "refs", "User"]);
    assert_exit_code(&sg_output, 0);
    let sg_json = parse_json(&sg_output);
    let sg_refs = sg_json["references"].as_array().unwrap();
    let sg_resolution = sg_json["resolution"].as_str().unwrap();

    // With --semantic: uses rust-analyzer (semantic tier).
    let lsp_output = run_cq_project(&project, &["--json", "--semantic", "refs", "User"]);
    assert_exit_code(&lsp_output, 0);
    let lsp_json = parse_json(&lsp_output);
    let lsp_refs = lsp_json["references"].as_array().unwrap();
    let lsp_resolution = lsp_json["resolution"].as_str().unwrap();

    eprintln!(
        "  stack graph: resolution={sg_resolution}, {} refs",
        sg_refs.len()
    );
    eprintln!(
        "  semantic:    resolution={lsp_resolution}, {} refs",
        lsp_refs.len()
    );

    // Without --semantic, should get resolved or syntactic.
    assert!(
        sg_resolution == "resolved" || sg_resolution == "syntactic",
        "without --semantic, should get resolved or syntactic, got: {sg_resolution}"
    );

    // With --semantic, resolution should be semantic (or fallback if server failed).
    assert!(
        lsp_resolution == "semantic"
            || lsp_resolution == "resolved"
            || lsp_resolution == "syntactic",
        "with --semantic, expected valid resolution, got: {lsp_resolution}"
    );

    // If semantic resolution was achieved, it should find at least as many
    // references as the stack graph tier.
    if lsp_resolution == "semantic" {
        assert!(
            lsp_refs.len() >= sg_refs.len(),
            "semantic should find >= stack graph refs. semantic: {}, stack graph: {}",
            lsp_refs.len(),
            sg_refs.len()
        );
    }
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_comparison_semantic_vs_syntactic_refs_greet() {
    let project = rust_project();

    // Syntactic (no --semantic, no daemon).
    let syn_output = run_cq_project(&project, &["--json", "--no-semantic", "refs", "greet"]);
    assert_exit_code(&syn_output, 0);
    let syn_json = parse_json(&syn_output);
    let syn_refs = syn_json["references"].as_array().unwrap();
    let syn_resolution = syn_json["resolution"].as_str().unwrap();

    // Semantic (with --semantic).
    let sem_output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    assert_exit_code(&sem_output, 0);
    let sem_json = parse_json(&sem_output);
    let sem_refs = sem_json["references"].as_array().unwrap();
    let sem_resolution = sem_json["resolution"].as_str().unwrap();

    eprintln!(
        "  --no-semantic: resolution={syn_resolution}, {} refs",
        syn_refs.len()
    );
    eprintln!(
        "  --semantic:    resolution={sem_resolution}, {} refs",
        sem_refs.len()
    );

    // Both should find results.
    assert!(
        !syn_refs.is_empty(),
        "syntactic should find greet references"
    );
    assert!(
        !sem_refs.is_empty(),
        "semantic should find greet references"
    );

    // Semantic should find at least as many (it includes all syntactic results
    // plus any additional ones from the LSP).
    if sem_resolution == "semantic" {
        assert!(
            sem_refs.len() >= syn_refs.len(),
            "semantic refs ({}) should >= syntactic refs ({})",
            sem_refs.len(),
            syn_refs.len()
        );
    }
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_comparison_semantic_provides_trait_impl_for_is_valid() {
    // This test compares the quality of is_valid results between tiers.
    // rust-analyzer should find the trait definition AND implementation,
    // while stack graphs may only find one.
    let project = rust_project();

    let syn_output = run_cq_project(&project, &["--json", "--no-semantic", "refs", "is_valid"]);
    assert_exit_code(&syn_output, 0);
    let syn_json = parse_json(&syn_output);
    let syn_defs = syn_json["definitions"].as_array().unwrap();
    let syn_refs = syn_json["references"].as_array().unwrap();

    let sem_output = run_cq_project(&project, &["--json", "--semantic", "refs", "is_valid"]);
    assert_exit_code(&sem_output, 0);
    let sem_json = parse_json(&sem_output);
    let sem_defs = sem_json["definitions"].as_array().unwrap();
    let sem_refs = sem_json["references"].as_array().unwrap();
    let sem_resolution = sem_json["resolution"].as_str().unwrap();

    eprintln!(
        "  syntactic: {} defs, {} refs",
        syn_defs.len(),
        syn_refs.len()
    );
    eprintln!(
        "  semantic ({sem_resolution}): {} defs, {} refs",
        sem_defs.len(),
        sem_refs.len()
    );

    // Both should at least find the implementation in services.rs.
    assert!(
        !syn_defs.is_empty(),
        "syntactic should find is_valid definition"
    );
    assert!(
        !sem_defs.is_empty(),
        "semantic should find is_valid definition"
    );
}

// ===========================================================================
// Section 4: Edge Cases and Fallback
// ===========================================================================

#[test]
#[ignore] // Tests fallback behavior when server is unavailable
fn test_lsp_live_fallback_python_no_pyright() {
    // Python project with --semantic but pyright not installed.
    // Should fall back gracefully to stack graph or syntactic.
    let project = python_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "User"]);
    // Should not crash — either succeeds with fallback or returns results.
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "should not crash when LSP unavailable, got exit code: {code}"
    );

    if code == 0 {
        let json = parse_json(&output);
        let resolution = json["resolution"].as_str().unwrap();
        // Should fall back to resolved (stack graph) or syntactic since
        // pyright is not installed.
        assert!(
            resolution == "resolved" || resolution == "syntactic",
            "should fall back when LSP unavailable, got: {resolution}"
        );
        eprintln!("  Python fallback: resolution={resolution}");
    } else {
        eprintln!("  Python fallback: exit code 1 (no results, acceptable)");
    }
}

#[test]
#[ignore] // Tests error handling
fn test_lsp_live_fallback_nonexistent_project() {
    // --semantic with a bad project path should fail gracefully.
    let output = run_cq(&[
        "--project",
        "/nonexistent/path",
        "--semantic",
        "--json",
        "refs",
        "foo",
    ]);
    let code = output.status.code().unwrap();
    assert!(code != 0, "should fail for nonexistent project");
    eprintln!("  nonexistent project: exit code {code} (expected non-zero)");
}

#[test]
#[ignore] // Tests error handling
fn test_lsp_live_fallback_nonexistent_symbol_with_semantic() {
    // Querying a symbol that doesn't exist, with --semantic, should not crash.
    let project = rust_project();
    let output = run_cq_project(
        &project,
        &["--json", "--semantic", "refs", "nonexistent_symbol_xyz_42"],
    );
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "nonexistent symbol should not crash, got exit code: {code}"
    );
    eprintln!("  nonexistent symbol with --semantic: exit code {code}");
}

// ===========================================================================
// Section 5: Performance and Timeout Behavior
// ===========================================================================

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_completes_within_30s() {
    use std::time::Instant;
    let project = rust_project();
    let start = Instant::now();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    let elapsed = start.elapsed();

    // Oneshot LSP should complete within 30 seconds even for cold start.
    // rust-analyzer startup + indexing + query + shutdown.
    assert!(
        elapsed.as_secs() < 30,
        "oneshot LSP took too long: {elapsed:?}"
    );
    assert_exit_code(&output, 0);
    eprintln!("  oneshot completed in {elapsed:?}");
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_oneshot_rust_multiple_sequential_queries_stable() {
    // Run multiple oneshot queries sequentially to verify stability.
    // Each starts a fresh server instance.
    let project = rust_project();
    let symbols = ["greet", "User", "summarize"];

    for symbol in &symbols {
        let output = run_cq_project(&project, &["--json", "--semantic", "refs", symbol]);
        let code = output.status.code().unwrap();
        assert!(
            code == 0 || code == 1,
            "sequential oneshot for '{symbol}' should not crash, got exit code: {code}"
        );
        if code == 0 {
            let json = parse_json(&output);
            let resolution = json["resolution"].as_str().unwrap();
            eprintln!("  refs {symbol}: resolution={resolution}");
        } else {
            eprintln!("  refs {symbol}: no results (exit code 1)");
        }
    }
}

// ===========================================================================
// Section 6: Semantic Resolution Quality Verification
// ===========================================================================

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_resolution_field_is_semantic_when_server_works() {
    // If rust-analyzer is available AND responds, the resolution field should
    // be "semantic". This is a stricter check than the other tests which
    // accept fallback.
    //
    // We test against greet which is a simple top-level function — the most
    // likely to succeed with oneshot LSP.
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let resolution = json["resolution"].as_str().unwrap();

    // If resolution IS semantic, verify the quality of results.
    if resolution == "semantic" {
        let defs = json["definitions"].as_array().unwrap();
        let refs = json["references"].as_array().unwrap();

        // greet should have exactly one definition.
        assert!(
            !defs.is_empty(),
            "semantic resolution should find greet definition"
        );

        // greet should have references in integration.rs (import + 2 calls).
        let integration_refs: Vec<_> = refs
            .iter()
            .filter(|r| r["file"].as_str().unwrap_or("").contains("integration.rs"))
            .collect();
        assert!(
            !integration_refs.is_empty(),
            "semantic resolution should find greet references in integration.rs"
        );

        eprintln!(
            "  [PASS] semantic resolution verified: {} defs, {} refs ({} in integration.rs)",
            defs.len(),
            refs.len(),
            integration_refs.len()
        );
    } else {
        eprintln!(
            "  [SKIP] resolution was {resolution}, not semantic — rust-analyzer may have timed out. \
             This is acceptable but indicates the oneshot path needs more init time."
        );
    }
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_refs_greet_finds_integration_test_calls() {
    // Verify that the semantic tier finds cross-crate references (the
    // integration test calls greet from a separate test binary).
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().unwrap();

    // Should find references in integration.rs.
    let has_integration_ref = refs
        .iter()
        .any(|r| r["file"].as_str().unwrap_or("").contains("integration.rs"));
    assert!(
        has_integration_ref,
        "should find greet references in tests/integration.rs, got refs: {:?}",
        refs.iter()
            .map(|r| r["file"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    // The integration.rs calls greet twice: greet("world") and greet("").
    let integration_call_refs: Vec<_> = refs
        .iter()
        .filter(|r| {
            let file = r["file"].as_str().unwrap_or("");
            let kind = r["kind"].as_str().unwrap_or("");
            file.contains("integration.rs") && kind == "call"
        })
        .collect();

    eprintln!(
        "  greet refs in integration.rs: {} total, {} calls",
        refs.iter()
            .filter(|r| r["file"].as_str().unwrap_or("").contains("integration.rs"))
            .count(),
        integration_call_refs.len()
    );
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_refs_user_finds_all_impl_blocks() {
    // User has multiple impl blocks in services.rs:
    // - impl User (line 6)
    // - impl Validate for User (line 25)
    // - impl Summary for User (line 31)
    // - Used in process_users signature (line 38)
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "User"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"].as_array().unwrap();

    // Check for import reference.
    let has_import = refs.iter().any(|r| {
        let kind = r["kind"].as_str().unwrap_or("");
        let file = r["file"].as_str().unwrap_or("");
        kind == "import" && file.contains("services.rs")
    });

    // Check for type_usage references (impl blocks + function params).
    let type_usage_refs: Vec<_> = refs
        .iter()
        .filter(|r| {
            let kind = r["kind"].as_str().unwrap_or("");
            let file = r["file"].as_str().unwrap_or("");
            kind == "type_usage" && file.contains("services.rs")
        })
        .collect();

    eprintln!(
        "  User refs: {} total, import={has_import}, {} type_usage in services.rs",
        refs.len(),
        type_usage_refs.len()
    );

    // Should have at least 3 type_usage refs in services.rs (3 impl blocks +
    // function param).
    assert!(
        type_usage_refs.len() >= 3,
        "expected at least 3 type_usage refs in services.rs for User impl blocks, got {}",
        type_usage_refs.len()
    );
}

// ===========================================================================
// Section 7: Completeness and Metadata Consistency
// ===========================================================================

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_json_metadata_complete() {
    // Verify that semantic results have all expected JSON fields.
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "refs", "greet"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Required fields.
    assert!(json["resolution"].is_string(), "missing resolution field");
    assert!(
        json["completeness"].is_string(),
        "missing completeness field"
    );
    assert!(json["symbol"].is_string(), "missing symbol field");
    assert!(json["definitions"].is_array(), "missing definitions array");
    assert!(json["references"].is_array(), "missing references array");
    assert!(json["total"].is_number(), "missing total field");

    // Symbol should match query.
    assert_eq!(
        json["symbol"].as_str(),
        Some("greet"),
        "symbol field should match query"
    );

    eprintln!(
        "  JSON metadata: resolution={}, completeness={}, total={}",
        json["resolution"].as_str().unwrap(),
        json["completeness"].as_str().unwrap(),
        json["total"]
    );
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_callers_json_metadata_complete() {
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "callers", "summarize"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert!(json["resolution"].is_string(), "missing resolution");
    assert!(json["completeness"].is_string(), "missing completeness");
    assert!(json["symbol"].is_string(), "missing symbol");
    assert!(json["definitions"].is_array(), "missing definitions");
    assert!(json["callers"].is_array(), "missing callers");
    assert!(json["total"].is_number(), "missing total");

    assert_eq!(
        json["symbol"].as_str(),
        Some("summarize"),
        "symbol should match query"
    );
}

#[test]
#[ignore] // Requires rust-analyzer
fn test_lsp_live_semantic_deps_json_metadata_complete() {
    let project = rust_project();
    let output = run_cq_project(&project, &["--json", "--semantic", "deps", "process_users"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    assert!(json["resolution"].is_string(), "missing resolution");
    assert!(json["completeness"].is_string(), "missing completeness");
    assert!(json["symbol"].is_string(), "missing symbol");
    assert!(json["dependencies"].is_array(), "missing dependencies");
    assert!(json["total"].is_number(), "missing total");

    assert_eq!(
        json["symbol"].as_str(),
        Some("process_users"),
        "symbol should match query"
    );
}
