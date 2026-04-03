use crate::protocol::{ContentItem, ToolCallResult, ToolDefinition};
use serde_json::json;
use std::process::Command;

// ---------------------------------------------------------------------------
// Tool registry
// ---------------------------------------------------------------------------

/// Return all MCP tool definitions.
#[allow(clippy::too_many_lines)]
pub fn all_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "cq_def".to_string(),
            description: "Find where a symbol is defined across the project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to find (supports qualified names like Struct::method)"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file (e.g. 'src/lib')"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_body".to_string(),
            description: "Extract the full source body of a symbol definition".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to extract (supports qualified names)"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_sig".to_string(),
            description: "Extract the type signature of a symbol (without body)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to extract signature for"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_refs".to_string(),
            description: "Find all references to a symbol across the project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to find references for"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_callers".to_string(),
            description: "Find all call sites of a function across the project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Function name to find callers for"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_outline".to_string(),
            description: "List all symbols in a file with their kinds and nesting".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "File path to outline (relative or absolute)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "cq_symbols".to_string(),
            description: "List all symbols in the project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "description": "Filter by symbol kind (function, struct, class, etc.)"},
                    "scope": {"type": "string", "description": "Restrict to a subdirectory or file (e.g. 'crates/codequery-core')"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                }
            }),
        },
        ToolDefinition {
            name: "cq_imports".to_string(),
            description: "List imports and use statements in a file".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "File path to list imports for"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "cq_search".to_string(),
            description: "Structural search using tree-sitter S-expression queries. Use cq_tree to explore node types for a language.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Tree-sitter S-expression query (e.g. '(function_item name: (identifier) @name)')"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "cq_context".to_string(),
            description: "Show the enclosing symbol context around a file location".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string", "description": "Location as file:line (e.g. src/main.rs:42)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["location"]
            }),
        },
        ToolDefinition {
            name: "cq_tree".to_string(),
            description: "Show project file and directory structure".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Root path to display (defaults to project root)"},
                    "scope": {"type": "string", "description": "Restrict to a subdirectory"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                }
            }),
        },
        ToolDefinition {
            name: "cq_deps".to_string(),
            description: "Show dependency relationships for a symbol".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to show dependencies for"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_hover".to_string(),
            description: "Show type info, docs, and signature at a source location".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string", "description": "Location as file:line[:column]"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["location"]
            }),
        },
        ToolDefinition {
            name: "cq_diagnostics".to_string(),
            description: "Show syntax errors and language server diagnostics for a file or project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "File to check (omit for whole project)"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                }
            }),
        },
        ToolDefinition {
            name: "cq_rename".to_string(),
            description: "Rename a symbol across the project. Dry-run at syntactic precision, applies at semantic/resolved.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "old": {"type": "string", "description": "Current symbol name"},
                    "new": {"type": "string", "description": "New symbol name"},
                    "apply": {"type": "boolean", "description": "Force apply changes regardless of precision tier"},
                    "dry_run": {"type": "boolean", "description": "Force preview mode without applying"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["old", "new"]
            }),
        },
        ToolDefinition {
            name: "cq_dead".to_string(),
            description: "Find unreferenced (dead) symbols in the project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "description": "Filter by symbol kind"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                }
            }),
        },
        ToolDefinition {
            name: "cq_callchain".to_string(),
            description: "Trace multi-level call hierarchy for a symbol".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name to trace"},
                    "depth": {"type": "integer", "description": "Maximum depth, default 3"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "cq_hierarchy".to_string(),
            description: "Show type hierarchy — supertypes and subtypes".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Type name to show hierarchy for"},
                    "scope": {"type": "string", "description": "Restrict search to a subdirectory or file"},
                    "lang": {"type": "string", "description": "Filter by language (e.g. rust, python, typescript)"},
                    "project": {"type": "string", "description": "Project root directory (defaults to cwd)"}
                },
                "required": ["symbol"]
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Tool dispatch
// ---------------------------------------------------------------------------

/// Execute a tool call by shelling out to the `cq` binary.
///
/// Returns a `ToolCallResult` with either the JSON output from cq or an error
/// message if the command failed.
pub fn execute_tool(name: &str, arguments: &serde_json::Value) -> ToolCallResult {
    let result = match name {
        "cq_def" => run_symbol_command("def", arguments),
        "cq_body" => run_symbol_command("body", arguments),
        "cq_sig" => run_symbol_command("sig", arguments),
        "cq_refs" => run_symbol_command("refs", arguments),
        "cq_callers" => run_symbol_command("callers", arguments),
        "cq_deps" => run_symbol_command("deps", arguments),
        "cq_outline" => run_file_command("outline", arguments),
        "cq_imports" => run_file_command("imports", arguments),
        "cq_symbols" => run_symbols_command(arguments),
        "cq_search" => run_search_command(arguments),
        "cq_context" => run_context_command(arguments),
        "cq_tree" => run_tree_command(arguments),
        "cq_hover" => run_hover_command(arguments),
        "cq_diagnostics" => run_diagnostics_command(arguments),
        "cq_rename" => run_rename_command(arguments),
        "cq_dead" => run_dead_command(arguments),
        "cq_callchain" => run_callchain_command(arguments),
        "cq_hierarchy" => run_symbol_command("hierarchy", arguments),
        _ => Err(format!("Unknown tool: {name}")),
    };

    match result {
        Ok(output) => ToolCallResult {
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text: output,
            }],
            is_error: None,
        },
        Err(err) => ToolCallResult {
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text: err,
            }],
            is_error: Some(true),
        },
    }
}

// ---------------------------------------------------------------------------
// Command builders
// ---------------------------------------------------------------------------

/// Run a command that takes a `symbol` positional argument (def, body, sig, refs, callers, deps).
fn run_symbol_command(subcommand: &str, args: &serde_json::Value) -> Result<String, String> {
    let symbol = args
        .get("symbol")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: symbol".to_string())?;

    let mut cmd_args: Vec<String> = vec![subcommand.to_string()];
    cmd_args.push(symbol.to_string());

    call_cq(&cmd_args, args)
}

/// Run a command that takes a `file` positional argument (outline, imports).
fn run_file_command(subcommand: &str, args: &serde_json::Value) -> Result<String, String> {
    let file = args
        .get("file")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: file".to_string())?;

    let mut cmd_args: Vec<String> = vec![subcommand.to_string()];
    cmd_args.push(file.to_string());

    call_cq(&cmd_args, args)
}

/// Run the `symbols` command which takes optional `kind` filter.
fn run_symbols_command(args: &serde_json::Value) -> Result<String, String> {
    let mut cmd_args: Vec<String> = Vec::new();

    if let Some(kind) = args.get("kind").and_then(serde_json::Value::as_str) {
        cmd_args.push("--kind".to_string());
        cmd_args.push(kind.to_string());
    }

    cmd_args.push("symbols".to_string());

    call_cq(&cmd_args, args)
}

/// Run the `search` command with S-expression pattern.
fn run_search_command(args: &serde_json::Value) -> Result<String, String> {
    let pattern = args
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: pattern".to_string())?;

    let cmd_args: Vec<String> = vec!["search".to_string(), pattern.to_string()];

    call_cq(&cmd_args, args)
}

/// Run the `context` command.
fn run_context_command(args: &serde_json::Value) -> Result<String, String> {
    let location = args
        .get("location")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: location".to_string())?;

    let cmd_args: Vec<String> = vec!["context".to_string(), location.to_string()];

    call_cq(&cmd_args, args)
}

/// Run the `tree` command.
fn run_tree_command(args: &serde_json::Value) -> Result<String, String> {
    let mut cmd_args: Vec<String> = vec!["tree".to_string()];

    if let Some(path) = args.get("path").and_then(serde_json::Value::as_str) {
        cmd_args.push(path.to_string());
    }

    call_cq(&cmd_args, args)
}

/// Run the `hover` command at a source location.
fn run_hover_command(args: &serde_json::Value) -> Result<String, String> {
    let location = args
        .get("location")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: location".to_string())?;

    let cmd_args: Vec<String> = vec!["hover".to_string(), location.to_string()];

    call_cq(&cmd_args, args)
}

/// Run the `diagnostics` command for a file or whole project.
fn run_diagnostics_command(args: &serde_json::Value) -> Result<String, String> {
    let mut cmd_args: Vec<String> = vec!["diagnostics".to_string()];

    if let Some(file) = args.get("file").and_then(serde_json::Value::as_str) {
        cmd_args.push(file.to_string());
    }

    call_cq(&cmd_args, args)
}

/// Run the `rename` command.
fn run_rename_command(args: &serde_json::Value) -> Result<String, String> {
    let old = args
        .get("old")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: old".to_string())?;

    let new = args
        .get("new")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: new".to_string())?;

    let mut cmd_args: Vec<String> = vec!["rename".to_string(), old.to_string(), new.to_string()];

    if args
        .get("apply")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        cmd_args.push("--apply".to_string());
    } else if args
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        cmd_args.push("--dry-run".to_string());
    }

    call_cq(&cmd_args, args)
}

/// Run the `dead` command to find unreferenced symbols.
fn run_dead_command(args: &serde_json::Value) -> Result<String, String> {
    let mut cmd_args: Vec<String> = Vec::new();

    if let Some(kind) = args.get("kind").and_then(serde_json::Value::as_str) {
        cmd_args.push("--kind".to_string());
        cmd_args.push(kind.to_string());
    }

    cmd_args.push("dead".to_string());

    call_cq(&cmd_args, args)
}

/// Run the `callchain` command.
fn run_callchain_command(args: &serde_json::Value) -> Result<String, String> {
    let symbol = args
        .get("symbol")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Missing required argument: symbol".to_string())?;

    let mut cmd_args: Vec<String> = vec!["callchain".to_string(), symbol.to_string()];

    if let Some(depth) = args.get("depth").and_then(serde_json::Value::as_i64) {
        cmd_args.push("--depth".to_string());
        cmd_args.push(depth.to_string());
    }

    call_cq(&cmd_args, args)
}

// ---------------------------------------------------------------------------
// cq subprocess execution
// ---------------------------------------------------------------------------

/// Shell out to the `cq` binary and return its stdout.
///
/// If `project` is present in `args`, it is passed as `--project <dir>`.
/// On non-zero exit, the stderr content is returned as an error.
fn call_cq(cmd_args: &[String], tool_args: &serde_json::Value) -> Result<String, String> {
    let mut args: Vec<String> = Vec::new();

    // CQ_SEMANTIC: pass through as env var to cq subprocess.
    // Default is off (no LSP). Harness configures via CQ_SEMANTIC env var.
    // Don't add --semantic flag — the binary reads CQ_SEMANTIC directly.

    // Cache disabled by default: AI agents edit files between queries, so
    // cached results go stale. Enable with CQ_CACHE=1 for read-only workloads.
    if std::env::var("CQ_CACHE").as_deref() != Ok("1") {
        args.push("--no-cache".to_string());
    }

    // Inject --project if provided
    if let Some(project) = tool_args.get("project").and_then(serde_json::Value::as_str) {
        args.push("--project".to_string());
        args.push(project.to_string());
    }

    // Inject --in (scope) if provided
    if let Some(scope) = tool_args.get("scope").and_then(serde_json::Value::as_str) {
        args.push("--in".to_string());
        args.push(scope.to_string());
    }

    // Inject --lang if provided
    if let Some(lang) = tool_args.get("lang").and_then(serde_json::Value::as_str) {
        args.push("--lang".to_string());
        args.push(lang.to_string());
    }

    args.extend_from_slice(cmd_args);

    let cq_bin = std::env::var("CQ_BIN").unwrap_or_else(|_| "cq".to_string());
    let output = Command::new(&cq_bin)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute {cq_bin}: {e}"))?;

    // Exit code 0 = success (including no results — total=0 in meta header).
    // Non-zero = actual error (bad args, missing grammar, etc.).
    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code == 0 {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Err(if stderr.is_empty() {
            if stdout.is_empty() {
                format!("cq exited with status {}", output.status)
            } else {
                stdout
            }
        } else {
            stderr
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tools_returns_eighteen_tools() {
        let tools = all_tools();
        assert_eq!(tools.len(), 18);
    }

    #[test]
    fn all_tools_have_unique_names() {
        let tools = all_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 18);
    }

    #[test]
    fn all_tools_have_descriptions() {
        for tool in all_tools() {
            assert!(
                !tool.description.is_empty(),
                "tool {} has empty description",
                tool.name
            );
        }
    }

    #[test]
    fn all_tools_have_object_schemas() {
        for tool in all_tools() {
            assert_eq!(
                tool.input_schema["type"], "object",
                "tool {} schema is not object type",
                tool.name
            );
        }
    }

    #[test]
    fn tool_names_match_expected() {
        let tools = all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"cq_def"));
        assert!(names.contains(&"cq_body"));
        assert!(names.contains(&"cq_sig"));
        assert!(names.contains(&"cq_refs"));
        assert!(names.contains(&"cq_callers"));
        assert!(names.contains(&"cq_outline"));
        assert!(names.contains(&"cq_symbols"));
        assert!(names.contains(&"cq_imports"));
        assert!(names.contains(&"cq_search"));
        assert!(names.contains(&"cq_context"));
        assert!(names.contains(&"cq_tree"));
        assert!(names.contains(&"cq_deps"));
        assert!(names.contains(&"cq_hover"));
        assert!(names.contains(&"cq_diagnostics"));
        assert!(names.contains(&"cq_rename"));
        assert!(names.contains(&"cq_dead"));
        assert!(names.contains(&"cq_callchain"));
        assert!(names.contains(&"cq_hierarchy"));
    }

    #[test]
    fn symbol_tools_require_symbol_param() {
        let symbol_tools = [
            "cq_def",
            "cq_body",
            "cq_sig",
            "cq_refs",
            "cq_callers",
            "cq_deps",
        ];
        let tools = all_tools();
        for name in &symbol_tools {
            let tool = tools.iter().find(|t| t.name == *name).unwrap();
            let required = tool.input_schema["required"]
                .as_array()
                .expect("missing required array");
            assert!(
                required.iter().any(|v| v == "symbol"),
                "tool {name} does not require symbol"
            );
        }
    }

    #[test]
    fn file_tools_require_file_param() {
        let file_tools = ["cq_outline", "cq_imports"];
        let tools = all_tools();
        for name in &file_tools {
            let tool = tools.iter().find(|t| t.name == *name).unwrap();
            let required = tool.input_schema["required"]
                .as_array()
                .expect("missing required array");
            assert!(
                required.iter().any(|v| v == "file"),
                "tool {name} does not require file"
            );
        }
    }

    #[test]
    fn unknown_tool_returns_error() {
        let result = execute_tool("nonexistent", &json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[test]
    fn missing_symbol_returns_error() {
        let result = execute_tool("cq_def", &json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .text
            .contains("Missing required argument: symbol"));
    }

    #[test]
    fn missing_file_returns_error() {
        let result = execute_tool("cq_outline", &json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .text
            .contains("Missing required argument: file"));
    }

    #[test]
    fn missing_pattern_returns_error() {
        let result = execute_tool("cq_search", &json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .text
            .contains("Missing required argument: pattern"));
    }

    #[test]
    fn missing_location_returns_error() {
        let result = execute_tool("cq_context", &json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .text
            .contains("Missing required argument: location"));
    }
}
