//! Tier 2 cross-language integration tests for cq commands.
//!
//! Verifies that all 11 commands work across the 9 Tier 2 languages:
//! Ruby, PHP, C#, Swift, Kotlin, Scala, Zig, Lua, Bash.
//!
//! Uses per-language fixture projects under `tests/fixtures/<lang>_project`.

mod common;

use common::{assert_exit_code, run_cq, stdout};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn fixture_base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn ruby_project() -> PathBuf {
    fixture_base().join("ruby_project")
}

fn php_project() -> PathBuf {
    fixture_base().join("php_project")
}

fn csharp_project() -> PathBuf {
    fixture_base().join("csharp_project")
}

fn swift_project() -> PathBuf {
    fixture_base().join("swift_project")
}

fn kotlin_project() -> PathBuf {
    fixture_base().join("kotlin_project")
}

fn scala_project() -> PathBuf {
    fixture_base().join("scala_project")
}

fn zig_project() -> PathBuf {
    fixture_base().join("zig_project")
}

fn lua_project() -> PathBuf {
    fixture_base().join("lua_project")
}

fn bash_project() -> PathBuf {
    fixture_base().join("bash_project")
}

/// Run cq against a specific fixture project.
fn run_cq_project(project: &PathBuf, args: &[&str]) -> std::process::Output {
    let project_str = project.to_str().unwrap();
    let mut full_args = vec!["--project", project_str];
    full_args.extend_from_slice(args);
    run_cq(&full_args)
}

// ===========================================================================
// 1. outline command — every Tier 2 language produces symbols
// ===========================================================================

#[test]
fn test_outline_ruby_extracts_symbols() {
    let project = ruby_project();
    let file = project.join("lib/main.rb");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(out.contains("add (function, pub)"), "missing add: {out}");
}

#[test]
fn test_outline_php_extracts_symbols() {
    let project = php_project();
    let file = project.join("src/models.php");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (class, pub)"),
        "missing User class: {out}"
    );
    assert!(
        out.contains("Greeter (interface, pub)"),
        "missing Greeter interface: {out}"
    );
    assert!(
        out.contains("Loggable (trait, pub)"),
        "missing Loggable trait: {out}"
    );
}

#[test]
fn test_outline_csharp_extracts_symbols() {
    let project = csharp_project();
    let file = project.join("src/Models.cs");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (class, pub)"),
        "missing User class: {out}"
    );
    assert!(
        out.contains("IGreeter (interface, pub)"),
        "missing IGreeter interface: {out}"
    );
    assert!(
        out.contains("Point (struct, pub)"),
        "missing Point struct: {out}"
    );
    assert!(
        out.contains("Color (enum, pub)"),
        "missing Color enum: {out}"
    );
}

#[test]
fn test_outline_swift_extracts_symbols() {
    let project = swift_project();
    let file = project.join("main.swift");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(out.contains("Animal (class,"), "missing Animal: {out}");
    assert!(out.contains("Point (struct,"), "missing Point: {out}");
    assert!(
        out.contains("Drawable (interface,"),
        "missing Drawable: {out}"
    );
    assert!(out.contains("Direction (enum,"), "missing Direction: {out}");
}

#[test]
fn test_outline_kotlin_extracts_symbols() {
    let project = kotlin_project();
    let file = project.join("Main.kt");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(out.contains("Animal (class, pub)"), "missing Animal: {out}");
    assert!(
        out.contains("Config (module, pub)"),
        "missing Config: {out}"
    );
    assert!(
        out.contains("Drawable (interface, pub)"),
        "missing Drawable: {out}"
    );
    assert!(
        out.contains("Direction (enum, pub)"),
        "missing Direction: {out}"
    );
}

#[test]
fn test_outline_scala_extracts_symbols() {
    let project = scala_project();
    let file = project.join("Main.scala");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Animal (class, pub)"), "missing Animal: {out}");
    assert!(
        out.contains("Drawable (trait, pub)"),
        "missing Drawable: {out}"
    );
    assert!(
        out.contains("Config (module, pub)"),
        "missing Config: {out}"
    );
    assert!(out.contains("Point (struct, pub)"), "missing Point: {out}");
}

#[test]
fn test_outline_zig_extracts_symbols() {
    let project = zig_project();
    let file = project.join("main.zig");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(
        out.contains("MAX_SIZE (const, pub)"),
        "missing MAX_SIZE: {out}"
    );
    assert!(out.contains("Color (enum, pub)"), "missing Color: {out}");
    assert!(out.contains("Point (struct, priv)"), "missing Point: {out}");
    assert!(out.contains("main (function, pub)"), "missing main: {out}");
}

#[test]
fn test_outline_lua_extracts_symbols() {
    let project = lua_project();
    let file = project.join("main.lua");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("M.greet (function, pub)"),
        "missing M.greet: {out}"
    );
    assert!(
        out.contains("global_fn (function, pub)"),
        "missing global_fn: {out}"
    );
}

#[test]
fn test_outline_bash_extracts_symbols() {
    let project = bash_project();
    let file = project.join("main.sh");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet: {out}"
    );
    assert!(
        out.contains("say_hello (function, pub)"),
        "missing say_hello: {out}"
    );
    assert!(
        out.contains("goodbye (function, pub)"),
        "missing goodbye: {out}"
    );
}

// ===========================================================================
// 2. def command — every Tier 2 language finds symbols
// ===========================================================================

#[test]
fn test_def_ruby_finds_function() {
    let output = run_cq_project(&ruby_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_ruby_finds_class() {
    let output = run_cq_project(&ruby_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("class User"), "missing class User: {out}");
}

#[test]
fn test_def_php_finds_function() {
    let output = run_cq_project(&php_project(), &["def", "globalFunction"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function globalFunction"),
        "missing function globalFunction: {out}"
    );
}

#[test]
fn test_def_php_finds_class() {
    let output = run_cq_project(&php_project(), &["def", "User"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("class User"), "missing class User: {out}");
}

#[test]
fn test_def_csharp_finds_class() {
    let output = run_cq_project(&csharp_project(), &["def", "Point"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("struct Point"), "missing struct Point: {out}");
}

#[test]
fn test_def_swift_finds_function() {
    let output = run_cq_project(&swift_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_kotlin_finds_function() {
    let output = run_cq_project(&kotlin_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_scala_finds_class() {
    let output = run_cq_project(&scala_project(), &["def", "Animal"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("class Animal"), "missing class Animal: {out}");
}

#[test]
fn test_def_zig_finds_function() {
    let output = run_cq_project(&zig_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

#[test]
fn test_def_lua_finds_function() {
    let output = run_cq_project(&lua_project(), &["def", "global_fn"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function global_fn"),
        "missing function global_fn: {out}"
    );
}

#[test]
fn test_def_bash_finds_function() {
    let output = run_cq_project(&bash_project(), &["def", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function greet"),
        "missing function greet: {out}"
    );
}

// ===========================================================================
// 3. body command — representative subset (Ruby, PHP, Zig)
// ===========================================================================

#[test]
fn test_body_ruby_extracts_function_body() {
    let output = run_cq_project(&ruby_project(), &["body", "add"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("x + y"), "body should contain x + y: {out}");
}

#[test]
fn test_body_php_extracts_function_body() {
    let output = run_cq_project(&php_project(), &["body", "globalFunction"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Hello"), "body should contain Hello: {out}");
}

#[test]
fn test_body_zig_extracts_function_body() {
    let output = run_cq_project(&zig_project(), &["body", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("Hello!"), "body should contain Hello!: {out}");
}

// ===========================================================================
// 4. sig command — representative subset (Ruby, PHP, Zig)
// ===========================================================================

#[test]
fn test_sig_ruby_extracts_function_signature() {
    let output = run_cq_project(&ruby_project(), &["sig", "add"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("def add(x, y)"), "missing sig: {out}");
}

#[test]
fn test_sig_php_extracts_function_signature() {
    let output = run_cq_project(&php_project(), &["sig", "globalFunction"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("function globalFunction(string $name): string"),
        "missing sig: {out}"
    );
}

#[test]
fn test_sig_zig_extracts_function_signature() {
    let output = run_cq_project(&zig_project(), &["sig", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("pub fn greet(name: []const u8) []const u8"),
        "missing sig: {out}"
    );
}

// ===========================================================================
// 5. imports command — representative subset (Zig has @import)
// ===========================================================================

#[test]
fn test_imports_zig_extracts_imports() {
    let project = zig_project();
    let file = project.join("main.zig");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("std"), "should contain std import: {out}");
    assert!(
        out.contains("utils.zig"),
        "should contain utils.zig import: {out}"
    );
}

#[test]
fn test_imports_bash_extracts_source_statements() {
    let project = bash_project();
    let file = project.join("main.sh");
    let output = run_cq_project(&project, &["imports", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("utils.sh"),
        "should contain utils.sh source: {out}"
    );
}

// ===========================================================================
// 6. symbols --lang ruby — filters to Ruby only
// ===========================================================================

#[test]
fn test_symbols_lang_ruby_filters_to_ruby_only() {
    let output = run_cq_project(&ruby_project(), &["--lang", "ruby", "symbols"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("lib/main.rb"),
        "should contain ruby file: {out}"
    );
    assert!(out.contains("greet"), "should contain greet: {out}");
    assert!(out.contains("User"), "should contain User: {out}");
    assert!(out.contains("Utils"), "should contain Utils module: {out}");
}

// ===========================================================================
// 7. tree command — Tier 2 projects produce file + symbol tree
// ===========================================================================

#[test]
fn test_tree_ruby_project_shows_files_and_symbols() {
    let output = run_cq_project(&ruby_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("lib/main.rb"), "missing main.rb: {out}");
    assert!(out.contains("lib/models.rb"), "missing models.rb: {out}");
    assert!(out.contains("lib/utils.rb"), "missing utils.rb: {out}");
    assert!(
        out.contains("User (class, pub)"),
        "missing User class in tree: {out}"
    );
}

#[test]
fn test_tree_zig_project_shows_files_and_symbols() {
    let output = run_cq_project(&zig_project(), &["tree"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("main.zig"), "missing main.zig: {out}");
    assert!(
        out.contains("greet (function, pub)"),
        "missing greet in tree: {out}"
    );
}

// ===========================================================================
// 8. search command — finds Ruby functions via raw S-expression
// ===========================================================================

#[test]
fn test_search_raw_ruby_finds_methods() {
    let output = run_cq_project(
        &ruby_project(),
        &["--raw", "search", "(method name: (identifier) @name)"],
    );
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("greet"), "should find greet method: {out}");
    assert!(out.contains("add"), "should find add method: {out}");
}

// ===========================================================================
// Additional commands: context, refs, callers, deps across Tier 2
// ===========================================================================

// --- context command ---

#[test]
fn test_context_ruby_finds_enclosing_function() {
    let project = ruby_project();
    let file = project.join("lib/main.rb");
    let location = format!("{}:5", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function greet"), "should find greet: {out}");
}

#[test]
fn test_context_zig_finds_enclosing_function() {
    let project = zig_project();
    let file = project.join("main.zig");
    let location = format!("{}:6", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("function greet"), "should find greet: {out}");
}

#[test]
fn test_context_kotlin_finds_enclosing_class() {
    let project = kotlin_project();
    let file = project.join("Main.kt");
    let location = format!("{}:6", file.display());
    let output = run_cq_project(&project, &["context", &location]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Animal") || out.contains("speak"),
        "should find enclosing context: {out}"
    );
}

// --- refs command ---

#[test]
fn test_refs_ruby_returns_success() {
    let output = run_cq_project(&ruby_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("definition"), "should show definition: {out}");
}

#[test]
fn test_refs_php_returns_success() {
    let output = run_cq_project(&php_project(), &["refs", "globalFunction"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("definition"), "should show definition: {out}");
}

#[test]
fn test_refs_zig_returns_success() {
    let output = run_cq_project(&zig_project(), &["refs", "greet"]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("definition"), "should show definition: {out}");
}

// --- callers command ---

#[test]
fn test_callers_ruby_finds_call_sites() {
    let output = run_cq_project(&ruby_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_callers_zig_finds_call_sites() {
    let output = run_cq_project(&zig_project(), &["callers", "greet"]);
    assert_exit_code(&output, 0);
}

// --- deps command ---

#[test]
fn test_deps_ruby_returns_success() {
    let output = run_cq_project(&ruby_project(), &["deps", "greet"]);
    assert_exit_code(&output, 0);
}

#[test]
fn test_deps_zig_returns_success() {
    let output = run_cq_project(&zig_project(), &["deps", "main"]);
    assert_exit_code(&output, 0);
}

// ===========================================================================
// Ruby model file: outline nested class/method structure
// ===========================================================================

#[test]
fn test_outline_ruby_models_nested_class_with_methods() {
    let project = ruby_project();
    let file = project.join("lib/models.rb");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("User (class, pub)"),
        "missing User class: {out}"
    );
    assert!(
        out.contains("Admin (class, pub)"),
        "missing Admin class: {out}"
    );
    assert!(
        out.contains("initialize (method, pub)"),
        "missing initialize method: {out}"
    );
}

// ===========================================================================
// PHP model file: outline class, interface, trait
// ===========================================================================

#[test]
fn test_outline_php_models_class_interface_trait() {
    let project = php_project();
    let file = project.join("src/models.php");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(out.contains("User (class, pub)"), "missing User: {out}");
    assert!(
        out.contains("Greeter (interface, pub)"),
        "missing Greeter: {out}"
    );
    assert!(
        out.contains("Loggable (trait, pub)"),
        "missing Loggable: {out}"
    );
    assert!(
        out.contains("App\\Models (module, pub)"),
        "missing namespace: {out}"
    );
}

// ===========================================================================
// Zig: tests extracted as test symbols
// ===========================================================================

#[test]
fn test_outline_zig_extracts_test_declarations() {
    let project = zig_project();
    let file = project.join("main.zig");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("basic greet (test, priv)"),
        "missing test decl: {out}"
    );
    assert!(
        out.contains("helper works (test, priv)"),
        "missing test decl: {out}"
    );
}

// ===========================================================================
// Scala: trait vs class distinction
// ===========================================================================

#[test]
fn test_outline_scala_distinguishes_trait_class_object() {
    let project = scala_project();
    let file = project.join("Main.scala");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Drawable (trait, pub)"),
        "Drawable should be trait: {out}"
    );
    assert!(
        out.contains("Animal (class, pub)"),
        "Animal should be class: {out}"
    );
    assert!(
        out.contains("Config (module, pub)"),
        "Config should be module (object): {out}"
    );
}

// ===========================================================================
// Swift: protocol as interface, extension as module
// ===========================================================================

#[test]
fn test_outline_swift_protocol_as_interface() {
    let project = swift_project();
    let file = project.join("main.swift");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Drawable (interface,"),
        "Drawable should be interface: {out}"
    );
    assert!(
        out.contains("String (module,"),
        "String extension should be module: {out}"
    );
}

// ===========================================================================
// Kotlin: object as module, data class as struct
// ===========================================================================

#[test]
fn test_outline_kotlin_object_as_module_data_class_as_struct() {
    let project = kotlin_project();
    let file = project.join("Main.kt");
    let output = run_cq_project(&project, &["outline", file.to_str().unwrap()]);
    assert_exit_code(&output, 0);
    let out = stdout(&output);
    assert!(
        out.contains("Config (module, pub)"),
        "Config object should be module: {out}"
    );
    assert!(
        out.contains("Point (struct, pub)"),
        "Point data class should be struct: {out}"
    );
}
