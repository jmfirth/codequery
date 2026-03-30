#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! MCP (Model Context Protocol) server for cq.
//!
//! Reads JSON-RPC 2.0 messages from stdin and dispatches them as cq commands,
//! returning structured results over stdout. This enables AI tools (Claude, etc.)
//! to call cq functionality through the MCP tool-call interface.

mod protocol;
mod tools;

use protocol::{
    InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo,
    ToolCallParams, ToolsCapability, INTERNAL_ERROR, METHOD_NOT_FOUND, PARSE_ERROR,
};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::process::Command;

/// Start the cq daemon for the current project.
/// This keeps language servers warm for fast semantic queries.
fn start_daemon() {
    let _ = Command::new("cq")
        .args(["daemon", "start"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    eprintln!("cq-mcp: daemon started");
}

/// Stop the cq daemon on shutdown.
fn stop_daemon() {
    let _ = Command::new("cq")
        .args(["daemon", "stop"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    eprintln!("cq-mcp: daemon stopped");
}

fn main() {
    // Start daemon on MCP server init — keeps LSP servers warm for
    // fast semantic queries. The daemon tracks file changes, so results
    // stay fresh even as the AI agent edits code between queries.
    start_daemon();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("cq-mcp: stdin read error: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_message(trimmed);

        // Notifications (no id) get no response
        if let Some(resp) = response {
            if let Ok(json) = serde_json::to_string(&resp) {
                let _ = writeln!(stdout, "{json}");
                let _ = stdout.flush();
            }
        }
    }

    // Clean shutdown: stop the daemon when the MCP server exits.
    stop_daemon();
}

/// Route a single JSON-RPC message and return a response (or `None` for notifications).
fn handle_message(raw: &str) -> Option<JsonRpcResponse> {
    let request: JsonRpcRequest = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(e) => {
            return Some(JsonRpcResponse::error(
                None,
                PARSE_ERROR,
                format!("Invalid JSON: {e}"),
            ));
        }
    };

    // Notifications have no id and expect no response
    request.id.as_ref()?;

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id),
        "tools/list" => handle_tools_list(request.id),
        "tools/call" => handle_tools_call(request.id, &request.params),
        "ping" => JsonRpcResponse::success(request.id, json!({})),
        _ => JsonRpcResponse::error(
            request.id,
            METHOD_NOT_FOUND,
            format!("Unknown method: {}", request.method),
        ),
    };

    Some(response)
}

/// Handle the `initialize` method.
fn handle_initialize(id: Option<serde_json::Value>) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: ToolsCapability {
                list_changed: false,
            },
        },
        server_info: ServerInfo {
            name: "cq-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    match serde_json::to_value(&result) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, format!("Serialization error: {e}")),
    }
}

/// Handle the `tools/list` method.
fn handle_tools_list(id: Option<serde_json::Value>) -> JsonRpcResponse {
    let tools = tools::all_tools();
    match serde_json::to_value(json!({ "tools": tools })) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, format!("Serialization error: {e}")),
    }
}

/// Handle the `tools/call` method.
fn handle_tools_call(id: Option<serde_json::Value>, params: &serde_json::Value) -> JsonRpcResponse {
    let call_params: ToolCallParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                protocol::INVALID_PARAMS,
                format!("Invalid tool call params: {e}"),
            );
        }
    };

    let result = tools::execute_tool(&call_params.name, &call_params.arguments);

    match serde_json::to_value(&result) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, format!("Serialization error: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Message routing
    // -----------------------------------------------------------------------

    #[test]
    fn initialize_returns_server_info() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "cq-mcp");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_returns_twelve_tools() {
        let msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success");
        let tools = result["tools"].as_array().expect("tools should be array");
        assert_eq!(tools.len(), 12);
    }

    #[test]
    fn tools_list_contains_expected_tool() {
        let msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success");
        let tools = result["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"cq_def"));
        assert!(names.contains(&"cq_body"));
        assert!(names.contains(&"cq_refs"));
    }

    #[test]
    fn notification_returns_none() {
        let msg = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        assert!(handle_message(msg).is_none());
    }

    #[test]
    fn unknown_method_returns_error() {
        let msg = r#"{"jsonrpc":"2.0","id":5,"method":"nonexistent","params":{}}"#;
        let resp = handle_message(msg).expect("should return response");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let resp = handle_message("not json at all").expect("should return error");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, PARSE_ERROR);
    }

    #[test]
    fn ping_returns_empty_object() {
        let msg = r#"{"jsonrpc":"2.0","id":10,"method":"ping","params":{}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success");
        assert_eq!(result, json!({}));
    }

    #[test]
    fn tools_call_with_invalid_params() {
        let msg = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":"bad"}"#;
        let resp = handle_message(msg).expect("should return response");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, protocol::INVALID_PARAMS);
    }

    #[test]
    fn tools_call_unknown_tool() {
        let msg = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nonexistent","arguments":{}}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success (error in content)");
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Unknown tool"));
    }

    #[test]
    fn tools_call_missing_required_arg() {
        let msg = r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"cq_def","arguments":{}}}"#;
        let resp = handle_message(msg).expect("should return response");
        let result = resp.result.expect("should be success (error in content)");
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Missing required argument"));
    }
}
