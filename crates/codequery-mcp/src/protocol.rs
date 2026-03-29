#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 base types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP-specific types
// ---------------------------------------------------------------------------

/// MCP server capabilities advertised during initialization.
#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
}

/// Indicates the server supports tools.
#[derive(Debug, Serialize)]
pub struct ToolsCapability {
    /// Whether the tool list can change during the session.
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// MCP initialize result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

/// Server identification.
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// An MCP tool definition.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP tool call result content item.
#[derive(Debug, Serialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool call result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// The params for a `tools/call` request.
#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Standard JSON-RPC error codes.
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const PARSE_ERROR: i64 = -32700;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_initialize_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::Value::Number(1.into())));
    }

    #[test]
    fn parse_tools_list_request() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/list");
    }

    #[test]
    fn parse_tools_call_request() {
        let json = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cq_def","arguments":{"symbol":"greet","project":"/tmp/proj"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
        let params: ToolCallParams = serde_json::from_value(req.params).unwrap();
        assert_eq!(params.name, "cq_def");
        assert_eq!(params.arguments["symbol"], "greet");
        assert_eq!(params.arguments["project"], "/tmp/proj");
    }

    #[test]
    fn parse_notification_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "notifications/initialized");
        assert!(req.id.is_none());
    }

    #[test]
    fn serialize_success_response() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::Value::Number(1.into())),
            serde_json::json!({"key": "value"}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""result""#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn serialize_error_response() {
        let resp = JsonRpcResponse::error(
            Some(serde_json::Value::Number(1.into())),
            METHOD_NOT_FOUND,
            "method not found",
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""error""#));
        assert!(json.contains(r#""code":-32601"#));
        assert!(!json.contains(r#""result""#));
    }

    #[test]
    fn serialize_initialize_result() {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                tools: ToolsCapability {
                    list_changed: false,
                },
            },
            server_info: ServerInfo {
                name: "cq-mcp".to_string(),
                version: "0.1.0".to_string(),
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["protocolVersion"], "2024-11-05");
        assert_eq!(json["serverInfo"]["name"], "cq-mcp");
        assert_eq!(json["capabilities"]["tools"]["listChanged"], false);
    }

    #[test]
    fn serialize_tool_definition() {
        let tool = ToolDefinition {
            name: "cq_def".to_string(),
            description: "Find symbol definitions".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {"type": "string", "description": "Symbol name"},
                },
                "required": ["symbol"]
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "cq_def");
        assert_eq!(json["inputSchema"]["type"], "object");
    }

    #[test]
    fn serialize_tool_call_result() {
        let result = ToolCallResult {
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text: "some output".to_string(),
            }],
            is_error: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "some output");
        // isError should be absent when None
        assert!(json.get("isError").is_none());
    }

    #[test]
    fn serialize_tool_call_error_result() {
        let result = ToolCallResult {
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text: "something failed".to_string(),
            }],
            is_error: Some(true),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["isError"], true);
    }
}
