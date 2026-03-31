#![warn(clippy::pedantic)]

//! Integration tests for the cq-mcp binary.
//!
//! These tests spawn the actual binary and communicate via stdin/stdout
//! to verify the MCP protocol works end-to-end.

use std::io::Write;
use std::process::{Command, Stdio};

/// Spawn cq-mcp, send lines to stdin, close stdin, and collect stdout lines.
fn run_mcp(input_lines: &[&str]) -> Vec<serde_json::Value> {
    let binary = env!("CARGO_BIN_EXE_cq-mcp");
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start cq-mcp");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        for line in input_lines {
            writeln!(stdin, "{line}").expect("failed to write to stdin");
        }
    }
    // Drop stdin to signal EOF
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("response is not valid JSON"))
        .collect()
}

#[test]
fn initialize_returns_protocol_version_and_server_info() {
    let responses = run_mcp(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
    ]);

    assert_eq!(responses.len(), 1);
    let resp = &responses[0];
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "cq-mcp");
    assert_eq!(resp["result"]["serverInfo"]["version"], "0.1.0");
    assert!(resp["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_returns_all_eighteen_tools() {
    let responses = run_mcp(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ]);

    // Should get 2 responses (initialize + tools/list; notification gets none)
    assert_eq!(responses.len(), 2);

    let list_resp = &responses[1];
    assert_eq!(list_resp["id"], 2);
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("tools should be array");
    assert_eq!(tools.len(), 18);

    // Verify each tool has required fields
    for tool in tools {
        assert!(tool["name"].is_string(), "tool missing name");
        assert!(tool["description"].is_string(), "tool missing description");
        assert!(tool["inputSchema"].is_object(), "tool missing inputSchema");
    }
}

#[test]
fn notification_produces_no_response() {
    let responses =
        run_mcp(&[r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#]);

    assert!(responses.is_empty());
}

#[test]
fn unknown_method_returns_error() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":1,"method":"bogus/method","params":{}}"#]);

    assert_eq!(responses.len(), 1);
    let resp = &responses[0];
    assert_eq!(resp["id"], 1);
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32601);
}

#[test]
fn invalid_json_returns_parse_error() {
    let responses = run_mcp(&["this is not json"]);

    assert_eq!(responses.len(), 1);
    let resp = &responses[0];
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32700);
}

#[test]
fn tools_call_with_missing_required_arg_returns_tool_error() {
    let responses = run_mcp(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"cq_def","arguments":{}}}"#,
    ]);

    assert_eq!(responses.len(), 1);
    let resp = &responses[0];
    assert_eq!(resp["id"], 1);
    // Tool errors come back as success with isError in the result
    let result = &resp["result"];
    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Missing required argument"));
}

#[test]
fn ping_returns_empty_result() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":42,"method":"ping","params":{}}"#]);

    assert_eq!(responses.len(), 1);
    let resp = &responses[0];
    assert_eq!(resp["id"], 42);
    assert_eq!(resp["result"], serde_json::json!({}));
}

#[test]
fn multiple_requests_produce_ordered_responses() {
    let responses = run_mcp(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}"#,
    ]);

    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[2]["id"], 3);
}

#[test]
fn empty_lines_are_ignored() {
    let responses = run_mcp(&[
        "",
        r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
        "",
        "",
    ]);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["id"], 1);
}
