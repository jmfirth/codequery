//! JSON-RPC protocol types for LSP communication.
//!
//! Implements the JSON-RPC 2.0 message types used by the Language Server Protocol.
//! Request IDs are generated from a global atomic counter to ensure uniqueness
//! within a process.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global atomic counter for generating unique JSON-RPC request IDs.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Generates the next unique request ID for a JSON-RPC request.
///
/// IDs are monotonically increasing within a process, starting at 1.
pub fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// A JSON-RPC 2.0 request message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    /// The JSON-RPC protocol version. Always "2.0".
    pub jsonrpc: String,

    /// The request identifier, used to match responses to requests.
    pub id: u64,

    /// The method to be invoked (e.g., "textDocument/definition").
    pub method: String,

    /// The method parameters, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Creates a new JSON-RPC request with an auto-generated ID.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: next_request_id(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    /// The JSON-RPC protocol version. Always "2.0".
    pub jsonrpc: String,

    /// The request identifier this response corresponds to.
    pub id: u64,

    /// The result of the request, present on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// The error, present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Returns `true` if this response indicates success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Returns `true` if this response indicates an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// A JSON-RPC 2.0 notification message (no ID, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcNotification {
    /// The JSON-RPC protocol version. Always "2.0".
    pub jsonrpc: String,

    /// The notification method (e.g., "initialized", "textDocument/didOpen").
    pub method: String,

    /// The notification parameters, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Creates a new JSON-RPC notification.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    /// The error code (e.g., -32600 for invalid request).
    pub code: i64,

    /// A short description of the error.
    pub message: String,

    /// Additional information about the error, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_request_id_increments() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert!(id2 > id1, "IDs should be monotonically increasing");
    }

    #[test]
    fn test_json_rpc_request_new_sets_version_and_method() {
        let req = JsonRpcRequest::new("textDocument/definition", None);
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "textDocument/definition");
        assert!(req.params.is_none());
        assert!(req.id > 0);
    }

    #[test]
    fn test_json_rpc_request_new_with_params() {
        let params = serde_json::json!({"key": "value"});
        let req = JsonRpcRequest::new("test/method", Some(params.clone()));
        assert_eq!(req.params, Some(params));
    }

    #[test]
    fn test_json_rpc_request_serializes_to_json() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "initialize");
        assert!(json.get("params").is_none());
    }

    #[test]
    fn test_json_rpc_request_deserializes_from_json() {
        let json = r#"{"jsonrpc":"2.0","id":5,"method":"test","params":{"a":1}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, 5);
        assert_eq!(req.method, "test");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(serde_json::json!({"capabilities": {}})),
            error: None,
        };
        assert!(resp.is_success());
        assert!(!resp.is_error());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "invalid request".to_string(),
                data: None,
            }),
        };
        assert!(!resp.is_success());
        assert!(resp.is_error());
    }

    #[test]
    fn test_json_rpc_response_serializes_to_json() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 42,
            result: Some(serde_json::json!(null)),
            error: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 42);
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_json_rpc_response_deserializes_from_json() {
        let json = r#"{"jsonrpc":"2.0","id":3,"result":{"data":"ok"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 3);
        assert!(resp.is_success());
    }

    #[test]
    fn test_json_rpc_notification_new() {
        let notif = JsonRpcNotification::new("initialized", None);
        assert_eq!(notif.jsonrpc, "2.0");
        assert_eq!(notif.method, "initialized");
        assert!(notif.params.is_none());
    }

    #[test]
    fn test_json_rpc_notification_serializes_without_params() {
        let notif = JsonRpcNotification::new("exit", None);
        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["method"], "exit");
        assert!(json.get("params").is_none());
    }

    #[test]
    fn test_json_rpc_notification_serializes_with_params() {
        let params = serde_json::json!({"textDocument": {"uri": "file:///test.rs"}});
        let notif = JsonRpcNotification::new("textDocument/didOpen", Some(params));
        let json = serde_json::to_value(&notif).unwrap();
        assert!(json.get("params").is_some());
    }

    #[test]
    fn test_json_rpc_notification_deserializes_from_json() {
        let json = r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":"hello"}}"#;
        let notif: JsonRpcNotification = serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "window/logMessage");
    }

    #[test]
    fn test_json_rpc_error_serializes_to_json() {
        let err = JsonRpcError {
            code: -32601,
            message: "method not found".to_string(),
            data: Some(serde_json::json!("details")),
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], -32601);
        assert_eq!(json["message"], "method not found");
        assert_eq!(json["data"], "details");
    }

    #[test]
    fn test_json_rpc_error_serializes_without_data() {
        let err = JsonRpcError {
            code: -32700,
            message: "parse error".to_string(),
            data: None,
        };
        let json = serde_json::to_value(&err).unwrap();
        assert!(json.get("data").is_none());
    }
}
