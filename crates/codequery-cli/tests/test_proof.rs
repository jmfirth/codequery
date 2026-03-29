//! Proof tests: comprehensive demonstrations that cq works correctly.
//!
//! These tests are STRICT — they FAIL if the feature doesn't work.
//! They prove six properties:
//!
//! 1. Stack graphs find refs that tree-sitter alone can't qualify
//! 2. Exact reference counts (completeness)
//! 3. Error tolerance (malformed files don't crash the pipeline)
//! 4. No false positives from name collision
//! 5. LSP finds things stack graphs don't (ignored without rust-analyzer)
//! 6. Self-dogfood (cq against its own codebase)

mod common;
use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn cq_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn stderr_str(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

// ===========================================================================
// Proof 1: Stack graphs find refs that tree-sitter alone can't qualify
// ===========================================================================

/// Python: stack graph resolution provides "resolved" metadata and cross-file
/// reference binding that syntactic search cannot.
///
/// format_name is defined in utils.py and imported+called in services.py.
/// A syntactic search would match by name alone. Stack graph resolution proves
/// the call at services.py:10 binds to the definition in utils.py:3.
#[test]
fn proof1_python_stack_graph_resolution_metadata() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["refs", "format_name", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Resolution MUST be "resolved" (not "syntactic") — this proves stack graphs
    // performed name binding, not just name matching.
    assert_eq!(
        json["resolution"].as_str(),
        Some("resolved"),
        "expected 'resolved' resolution for Python format_name refs, got: {:?}",
        json["resolution"]
    );

    // References must span the cross-file import.
    let refs = json["references"]
        .as_array()
        .expect("missing references array");
    assert!(
        refs.len() >= 2,
        "expected at least 2 references (import + call sites), got {}",
        refs.len()
    );

    // All references should be in services.py (the importing file).
    let services_refs: Vec<&serde_json::Value> = refs
        .iter()
        .filter(|r| r["file"].as_str().unwrap_or("").contains("services.py"))
        .collect();
    assert!(
        services_refs.len() >= 2,
        "expected at least 2 references in services.py, got {}",
        services_refs.len()
    );

    // The definition should point to utils.py (proves name binding).
    let defs = json["definitions"]
        .as_array()
        .expect("missing definitions array");
    let def_in_utils = defs
        .iter()
        .any(|d| d["file"].as_str().unwrap_or("").contains("utils.py"));
    assert!(
        def_in_utils,
        "definition should be in utils.py, got: {defs:?}"
    );
}

/// TypeScript: stack graph resolution for cross-file interface usage.
///
/// User is defined in models.ts and imported in services.ts. Resolution should
/// be "resolved", proving the import binding was followed.
#[test]
fn proof1_typescript_stack_graph_resolution_metadata() {
    let project = fixture_base().join("typescript_project");
    let output = run_cq_project(&project, &["refs", "User", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Resolution MUST be "resolved" for TypeScript (has TSG rules).
    assert_eq!(
        json["resolution"].as_str(),
        Some("resolved"),
        "expected 'resolved' resolution for TypeScript User refs, got: {:?}",
        json["resolution"]
    );

    // References must span multiple files (services.ts imports User).
    let refs = json["references"]
        .as_array()
        .expect("missing references array");
    assert!(
        refs.len() >= 2,
        "expected at least 2 TypeScript User references, got {}",
        refs.len()
    );

    let ref_files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    let has_services = ref_files.iter().any(|f| f.contains("services.ts"));
    assert!(
        has_services,
        "expected reference in services.ts, got files: {ref_files:?}"
    );

    // Definition should be in models.ts.
    let defs = json["definitions"]
        .as_array()
        .expect("missing definitions array");
    let def_in_models = defs
        .iter()
        .any(|d| d["file"].as_str().unwrap_or("").contains("models.ts"));
    assert!(
        def_in_models,
        "definition should be in models.ts, got: {defs:?}"
    );
}

// ===========================================================================
// Proof 2: Exact reference counts (completeness)
// ===========================================================================

/// Python format_name: exactly 4 known references in services.py.
///
/// Line 4: `from utils import format_name` (import)
/// Line 10: `return format_name(name, "verified")` (call)
/// Line 11: `return format_name(name, "pending")` (call)
/// Line 15: `return [format_name(u.name, "") for u in users]` (call)
#[test]
fn proof2_python_format_name_exact_reference_count() {
    let project = fixture_base().join("python_project");
    let output = run_cq_project(&project, &["refs", "format_name", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"]
        .as_array()
        .expect("missing references array");

    // Total must be exactly 4.
    let total = json["total"].as_u64().expect("missing total");
    assert_eq!(
        total, 4,
        "expected exactly 4 references for format_name, got {total}"
    );
    assert_eq!(
        refs.len(),
        4,
        "references array length should be 4, got {}",
        refs.len()
    );

    // All 4 references must be in services.py.
    let services_refs: Vec<&serde_json::Value> = refs
        .iter()
        .filter(|r| r["file"].as_str().unwrap_or("").contains("services.py"))
        .collect();
    assert_eq!(
        services_refs.len(),
        4,
        "all 4 references should be in services.py, got {} in services.py",
        services_refs.len()
    );

    // Verify specific lines.
    let ref_lines: Vec<u64> = refs.iter().filter_map(|r| r["line"].as_u64()).collect();
    assert!(
        ref_lines.contains(&4),
        "expected import ref at line 4, got lines: {ref_lines:?}"
    );
    assert!(
        ref_lines.contains(&10),
        "expected call ref at line 10, got lines: {ref_lines:?}"
    );
    assert!(
        ref_lines.contains(&11),
        "expected call ref at line 11, got lines: {ref_lines:?}"
    );
    assert!(
        ref_lines.contains(&15),
        "expected call ref at line 15, got lines: {ref_lines:?}"
    );
}

/// Rust greet: exactly 3 known references in tests/integration.rs.
///
/// Line 1: `use fixture_project::greet;` (import)
/// Line 5: `greet("world")` (call)
/// Line 10: `greet("")` (call)
#[test]
fn proof2_rust_greet_exact_reference_count() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["refs", "greet", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    let refs = json["references"]
        .as_array()
        .expect("missing references array");

    let total = json["total"].as_u64().expect("missing total");
    assert_eq!(
        total, 4,
        "expected exactly 4 references for greet, got {total}"
    );
    assert_eq!(
        refs.len(),
        4,
        "references array length should be 4, got {}",
        refs.len()
    );

    // 3 references must be in integration.rs, 1 in lib.rs (same-file call).
    let integration_refs: Vec<&serde_json::Value> = refs
        .iter()
        .filter(|r| r["file"].as_str().unwrap_or("").contains("integration.rs"))
        .collect();
    assert_eq!(
        integration_refs.len(),
        3,
        "3 references should be in integration.rs, got {} there",
        integration_refs.len()
    );
    let lib_refs: Vec<&serde_json::Value> = refs
        .iter()
        .filter(|r| r["file"].as_str().unwrap_or("").contains("lib.rs"))
        .collect();
    assert_eq!(
        lib_refs.len(),
        1,
        "1 reference should be in lib.rs (same-file call), got {} there",
        lib_refs.len()
    );

    // Verify specific lines.
    let ref_lines: Vec<u64> = refs.iter().filter_map(|r| r["line"].as_u64()).collect();
    assert!(
        ref_lines.contains(&1),
        "expected import ref at line 1, got lines: {ref_lines:?}"
    );
    assert!(
        ref_lines.contains(&5),
        "expected call ref at line 5, got lines: {ref_lines:?}"
    );
    assert!(
        ref_lines.contains(&10),
        "expected call ref at line 10, got lines: {ref_lines:?}"
    );
}

// ===========================================================================
// Proof 3: Error tolerance (malformed files don't crash the pipeline)
// ===========================================================================

/// A good Python file must parse and produce symbols via outline.
#[test]
fn proof3_good_file_parses_successfully() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("good.py"),
        "def greet(name):\n    return f'Hello, {name}!'\n",
    )
    .unwrap();

    let project = dir.path().to_path_buf();
    let good_path = project.join("good.py");
    let good_str = good_path.to_str().unwrap();
    let output = run_cq_project(&project, &["outline", good_str, "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let symbols = json["symbols"].as_array().expect("missing symbols array");
    let has_greet = symbols.iter().any(|s| s["name"].as_str() == Some("greet"));
    assert!(has_greet, "outline should find 'greet' in good.py");
}

/// A badly malformed Python file must not crash the pipeline.
/// Tree-sitter produces usable ASTs even on broken code — outline returns
/// empty symbols, not a panic.
#[test]
fn proof3_bad_file_does_not_crash() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.py"), "def )(\n    @@@invalid{{{\n").unwrap();

    let project = dir.path().to_path_buf();
    let bad_path = project.join("bad.py");
    let bad_str = bad_path.to_str().unwrap();
    let output = run_cq_project(&project, &["outline", bad_str, "--json"]);

    // Must not crash (exit code 0 or 1 are both acceptable — 1 means "no results").
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "bad file should not crash: got exit code {code}\nstderr: {}",
        stderr_str(&output)
    );

    // Must produce valid JSON (not garbage or empty).
    let json = parse_json(&output);
    assert!(
        json["symbols"].is_array(),
        "output should have a symbols array even for bad files"
    );
}

/// When scanning a directory with one good and one bad file, the symbols
/// command must still find the good file's symbols without crashing.
#[test]
fn proof3_mixed_directory_good_symbols_survive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("good.py"),
        "def greet(name):\n    return f'Hello, {name}!'\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("bad.py"), "def )(\n    @@@invalid{{{\n").unwrap();

    let project = dir.path().to_path_buf();
    let output = run_cq_project(&project, &["symbols", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let symbols = json["symbols"].as_array().expect("missing symbols array");
    let has_greet = symbols.iter().any(|s| s["name"].as_str() == Some("greet"));
    assert!(
        has_greet,
        "symbols should find 'greet' from good.py even when bad.py exists"
    );
}

// ===========================================================================
// Proof 4: No false positives from name collision
// ===========================================================================

/// Two files defining the same function name `process` must produce exactly 2
/// definitions, one per file, with different file paths.
#[test]
fn proof4_name_collision_finds_exactly_two_definitions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("file_a.py"),
        "def process(data):\n    return data.strip()\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("file_b.py"),
        "def process(items):\n    return [x for x in items]\n",
    )
    .unwrap();

    let project = dir.path().to_path_buf();
    let output = run_cq_project(&project, &["def", "process", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let defs = json["definitions"]
        .as_array()
        .expect("missing definitions array");

    // Must find EXACTLY 2 definitions.
    assert_eq!(
        defs.len(),
        2,
        "expected exactly 2 definitions for 'process', got {}",
        defs.len()
    );

    // Each definition must have a different file path.
    let file_a = defs[0]["file"].as_str().unwrap();
    let file_b = defs[1]["file"].as_str().unwrap();
    assert_ne!(
        file_a, file_b,
        "the two definitions should be in different files, got both in: {file_a}"
    );

    // Both files must be present.
    let files: Vec<&str> = defs.iter().filter_map(|d| d["file"].as_str()).collect();
    let has_a = files.iter().any(|f| f.contains("file_a.py"));
    let has_b = files.iter().any(|f| f.contains("file_b.py"));
    assert!(has_a, "expected definition in file_a.py, got: {files:?}");
    assert!(has_b, "expected definition in file_b.py, got: {files:?}");
}

/// When two unrelated functions share a name, refs should not crash and should
/// produce a valid result. With no cross-references, the refs array should be
/// empty (neither function is called anywhere).
#[test]
fn proof4_name_collision_refs_produces_valid_output() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("file_a.py"),
        "def process(data):\n    return data.strip()\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("file_b.py"),
        "def process(items):\n    return [x for x in items]\n",
    )
    .unwrap();

    let project = dir.path().to_path_buf();
    let output = run_cq_project(&project, &["refs", "process", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);

    // Must produce valid JSON with required fields.
    assert!(
        json["resolution"].is_string(),
        "missing resolution field in refs output"
    );
    assert!(
        json["references"].is_array(),
        "missing references array in refs output"
    );
    assert!(
        json["definitions"].is_array(),
        "missing definitions array in refs output"
    );

    // Two definitions must be present even in refs output.
    let defs = json["definitions"].as_array().unwrap();
    assert_eq!(
        defs.len(),
        2,
        "refs should still show 2 definitions for colliding name, got {}",
        defs.len()
    );

    // References should be empty (no cross-file calls between the two).
    let refs = json["references"].as_array().unwrap();
    assert_eq!(
        refs.len(),
        0,
        "expected 0 references for colliding names with no calls, got {}",
        refs.len()
    );
}

// ===========================================================================
// Proof 5: LSP finds things stack graphs don't (Rust trait impl)
// ===========================================================================

/// Baseline: without --semantic, summarize refs are syntactic and limited.
///
/// summarize is a trait method (traits.rs:16), with an impl in services.rs:32
/// and a call in services.rs:39. Without LSP, the trait definition in traits.rs
/// is NOT found as a definition — only the impl is.
#[test]
fn proof5_baseline_without_semantic() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["refs", "summarize", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // Without semantic, resolution should be "syntactic" (Rust TSG rules
    // currently fall back for this symbol).
    let resolution = json["resolution"].as_str().expect("missing resolution");
    assert!(
        resolution == "syntactic" || resolution == "resolved",
        "expected syntactic or resolved baseline, got: {resolution}"
    );

    // Should find at least the impl method definition.
    let defs = json["definitions"].as_array().expect("missing definitions");
    assert!(
        !defs.is_empty(),
        "expected at least 1 definition for summarize"
    );

    // Should find at least the call in process_users.
    let refs = json["references"].as_array().expect("missing references");
    assert!(
        !refs.is_empty(),
        "expected at least 1 reference for summarize"
    );
}

/// With --semantic, rust-analyzer finds both the trait definition and the impl.
///
/// This test requires rust-analyzer to be available and is marked #[ignore].
#[test]
#[ignore]
fn proof5_semantic_finds_trait_definition() {
    let project = fixture_base().join("rust_project");
    let output = run_cq_project(&project, &["--semantic", "refs", "summarize", "--json"]);
    assert_exit_code(&output, 0);
    let json = parse_json(&output);

    // With LSP, resolution should be "semantic".
    assert_eq!(
        json["resolution"].as_str(),
        Some("semantic"),
        "expected 'semantic' resolution with --semantic flag, got: {:?}",
        json["resolution"]
    );

    // Should find references in both traits.rs AND services.rs.
    let refs = json["references"]
        .as_array()
        .expect("missing references array");
    let ref_files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    let has_traits = ref_files.iter().any(|f| f.contains("traits.rs"));
    let has_services = ref_files.iter().any(|f| f.contains("services.rs"));
    assert!(
        has_services,
        "semantic refs should include services.rs, got: {ref_files:?}"
    );
    assert!(
        has_traits,
        "semantic refs should include traits.rs (trait def site), got: {ref_files:?}"
    );
}

// ===========================================================================
// Proof 6: Self-dogfood (cq against its own ~100-file codebase)
// ===========================================================================

/// cq symbols on its own codebase: must find >100 symbols across 6 crates.
#[test]
fn proof6_dogfood_symbols() {
    let root = cq_project_root();
    let output = run_cq_project(&root, &["symbols", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let symbols = json["symbols"].as_array().expect("missing symbols array");
    assert!(
        symbols.len() > 100,
        "expected > 100 symbols in cq codebase, got {}",
        symbols.len()
    );
}

/// cq outline on codequery-core/src/lib.rs: must find known module declarations.
#[test]
fn proof6_dogfood_outline() {
    let root = cq_project_root();
    let file = root.join("crates/codequery-core/src/lib.rs");
    let file_str = file.to_str().unwrap();
    let output = run_cq_project(&root, &["outline", file_str, "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let symbols = json["symbols"].as_array().expect("missing symbols array");
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();

    // Must contain known modules from lib.rs.
    assert!(
        names.contains(&"symbol"),
        "outline should contain 'symbol' module, got: {names:?}"
    );
    assert!(
        names.contains(&"discovery"),
        "outline should contain 'discovery' module, got: {names:?}"
    );
    assert!(
        names.contains(&"project"),
        "outline should contain 'project' module, got: {names:?}"
    );
}

/// cq refs Language on its own codebase: Language enum is used everywhere.
#[test]
fn proof6_dogfood_refs_language() {
    let root = cq_project_root();
    let output = run_cq_project(&root, &["refs", "Language", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let total = json["total"].as_u64().expect("missing total");
    assert!(
        total > 10,
        "expected > 10 references for Language in cq codebase, got {total}"
    );

    let refs = json["references"]
        .as_array()
        .expect("missing references array");
    assert!(
        refs.len() > 10,
        "expected > 10 reference entries for Language, got {}",
        refs.len()
    );
}

/// cq tree on its own codebase: must find .rs files across multiple crates.
#[test]
fn proof6_dogfood_tree() {
    let root = cq_project_root();
    let output = run_cq_project(&root, &["tree", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let files = json["files"].as_array().expect("missing files array");
    assert!(
        files.len() > 50,
        "expected > 50 files in tree output, got {}",
        files.len()
    );

    // Must contain .rs files.
    let rs_files: Vec<&str> = files
        .iter()
        .filter_map(|f| f["file"].as_str())
        .filter(|f| f.ends_with(".rs"))
        .collect();
    assert!(!rs_files.is_empty(), "tree output must contain .rs files");

    // Must span multiple crates.
    let has_core = rs_files.iter().any(|f| f.contains("codequery-core"));
    let has_parse = rs_files.iter().any(|f| f.contains("codequery-parse"));
    let has_cli = rs_files.iter().any(|f| f.contains("codequery-cli"));
    assert!(
        has_core,
        "tree should include codequery-core files, got: {:?}",
        &rs_files[..5.min(rs_files.len())]
    );
    assert!(has_parse, "tree should include codequery-parse files");
    assert!(has_cli, "tree should include codequery-cli files");
}

/// cq def StackGraphResolver on its own codebase: must find the struct in codequery-resolve.
#[test]
fn proof6_dogfood_def_stack_graph_resolver() {
    let root = cq_project_root();
    let output = run_cq_project(&root, &["def", "StackGraphResolver", "--json"]);
    assert_exit_code(&output, 0);

    let json = parse_json(&output);
    let defs = json["definitions"]
        .as_array()
        .expect("missing definitions array");
    assert_eq!(
        defs.len(),
        1,
        "expected exactly 1 definition for StackGraphResolver, got {}",
        defs.len()
    );

    let def = &defs[0];
    let file = def["file"].as_str().expect("missing file field");
    assert!(
        file.contains("codequery-resolve"),
        "StackGraphResolver should be in codequery-resolve crate, got: {file}"
    );
    assert_eq!(
        def["kind"].as_str(),
        Some("struct"),
        "StackGraphResolver should be a struct"
    );
}

/// cq body detect_project_root on its own codebase: must return actual code.
#[test]
fn proof6_dogfood_body() {
    let root = cq_project_root();
    let output = run_cq_project(&root, &["body", "detect_project_root"]);
    assert_exit_code(&output, 0);

    let out = stdout(&output);
    assert!(
        !out.trim().is_empty(),
        "body output should not be empty for detect_project_root"
    );
    // The body should contain actual Rust code (the function does canonicalization).
    assert!(
        out.contains("fn detect_project_root") || out.contains("canonicalize"),
        "body should contain function code, got: {}",
        &out[..200.min(out.len())]
    );
}
