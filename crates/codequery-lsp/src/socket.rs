//! Request/response protocol for the cq daemon Unix socket.
//!
//! Defines the message types exchanged between cq client invocations and the
//! background daemon process over a Unix domain socket. Messages are
//! length-prefixed JSON: a 4-byte big-endian u32 length followed by the JSON
//! payload.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{LspError, Result};
use crate::types::LspLocation;

/// A request sent from a cq client to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DaemonRequest {
    /// Execute an LSP query against a project.
    Query {
        /// The project root directory.
        project: PathBuf,
        /// The language identifier (e.g., "rust", "python").
        language: String,
        /// The LSP operation to perform (e.g., "definition", "references", "hover").
        operation: String,
        /// The file to query.
        file: PathBuf,
        /// 1-based line number.
        line: usize,
        /// 0-based column offset.
        column: usize,
        /// Optional symbol name for context.
        symbol: Option<String>,
    },
    /// Request the daemon's status (running servers, uptime).
    Status,
    /// Request the daemon to shut down gracefully.
    Shutdown,
}

/// Information about a running language server in the daemon pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    /// The project root this server is attached to.
    pub project: PathBuf,
    /// The language this server handles.
    pub language: String,
    /// How long this server has been running, in seconds.
    pub uptime_secs: u64,
}

/// A response sent from the daemon to a cq client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DaemonResponse {
    /// LSP locations returned from a definition or references query.
    Locations(Vec<LspLocation>),
    /// Hover text returned from a hover query.
    Hover(Option<String>),
    /// Daemon status information.
    Status {
        /// Running language servers.
        servers: Vec<ServerInfo>,
        /// Daemon uptime in seconds.
        uptime_secs: u64,
    },
    /// Acknowledgement (e.g., for shutdown).
    Ok,
    /// An error occurred processing the request.
    Error(String),
}

/// Writes a daemon message (request or response) to a stream.
///
/// Format: 4-byte big-endian length prefix followed by the JSON payload.
///
/// # Errors
///
/// Returns an error if serialization or writing fails.
pub fn write_message<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<()> {
    let json = serde_json::to_vec(message)?;
    let len = json.len();

    #[allow(clippy::cast_possible_truncation)]
    // Daemon messages are small JSON payloads; they will never exceed u32::MAX.
    let len_bytes = (len as u32).to_be_bytes();

    writer.write_all(&len_bytes)?;
    writer.write_all(&json)?;
    writer.flush()?;
    Ok(())
}

/// Reads a daemon message (request or response) from a stream.
///
/// Expects a 4-byte big-endian length prefix followed by the JSON payload.
///
/// # Errors
///
/// Returns an error if reading or deserialization fails.
pub fn read_message<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> Result<T> {
    const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            LspError::ConnectionFailed("connection closed".to_string())
        } else {
            LspError::Io(e)
        }
    })?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    // Guard against absurdly large messages (16 MiB limit).
    if len > MAX_MESSAGE_SIZE {
        return Err(LspError::ConnectionFailed(format!(
            "message too large: {len} bytes (max {MAX_MESSAGE_SIZE})"
        )));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    let message = serde_json::from_slice(&buf)?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::types::{Position, Range};

    // ─── DaemonRequest serialization ────────────────────────────────

    #[test]
    fn test_daemon_request_query_roundtrip() {
        let req = DaemonRequest::Query {
            project: PathBuf::from("/home/user/project"),
            language: "rust".to_string(),
            operation: "definition".to_string(),
            file: PathBuf::from("/home/user/project/src/main.rs"),
            line: 10,
            column: 4,
            symbol: Some("foo".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_daemon_request_query_no_symbol() {
        let req = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: "python".to_string(),
            operation: "references".to_string(),
            file: PathBuf::from("/project/app.py"),
            line: 5,
            column: 0,
            symbol: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_daemon_request_status_roundtrip() {
        let req = DaemonRequest::Status;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_daemon_request_shutdown_roundtrip() {
        let req = DaemonRequest::Shutdown;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DaemonRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
    }

    // ─── DaemonResponse serialization ───────────────────────────────

    #[test]
    fn test_daemon_response_locations_roundtrip() {
        let resp = DaemonResponse::Locations(vec![LspLocation {
            uri: "file:///src/main.rs".to_string(),
            range: Range::new(Position::new(5, 0), Position::new(5, 10)),
        }]);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_locations_empty() {
        let resp = DaemonResponse::Locations(vec![]);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_hover_some() {
        let resp = DaemonResponse::Hover(Some("fn foo() -> i32".to_string()));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_hover_none() {
        let resp = DaemonResponse::Hover(None);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_status_roundtrip() {
        let resp = DaemonResponse::Status {
            servers: vec![ServerInfo {
                project: PathBuf::from("/project"),
                language: "rust".to_string(),
                uptime_secs: 120,
            }],
            uptime_secs: 300,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_ok_roundtrip() {
        let resp = DaemonResponse::Ok;
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_daemon_response_error_roundtrip() {
        let resp = DaemonResponse::Error("something went wrong".to_string());
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    // ─── ServerInfo ─────────────────────────────────────────────────

    #[test]
    fn test_server_info_roundtrip() {
        let info = ServerInfo {
            project: PathBuf::from("/home/user/project"),
            language: "typescript".to_string(),
            uptime_secs: 60,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, parsed);
    }

    // ─── write_message / read_message ───────────────────────────────

    #[test]
    fn test_write_and_read_request() {
        let req = DaemonRequest::Status;
        let mut buf = Vec::new();
        write_message(&mut buf, &req).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonRequest = read_message(&mut cursor).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_write_and_read_response() {
        let resp = DaemonResponse::Ok;
        let mut buf = Vec::new();
        write_message(&mut buf, &resp).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_write_and_read_query_request() {
        let req = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: "go".to_string(),
            operation: "hover".to_string(),
            file: PathBuf::from("/project/main.go"),
            line: 42,
            column: 7,
            symbol: Some("Handler".to_string()),
        };
        let mut buf = Vec::new();
        write_message(&mut buf, &req).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonRequest = read_message(&mut cursor).unwrap();
        assert_eq!(req, parsed);
    }

    #[test]
    fn test_write_and_read_locations_response() {
        let resp = DaemonResponse::Locations(vec![
            LspLocation {
                uri: "file:///a.rs".to_string(),
                range: Range::new(Position::new(1, 0), Position::new(1, 5)),
            },
            LspLocation {
                uri: "file:///b.rs".to_string(),
                range: Range::new(Position::new(10, 2), Position::new(10, 8)),
            },
        ]);
        let mut buf = Vec::new();
        write_message(&mut buf, &resp).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_read_message_empty_stream_returns_error() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let result: Result<DaemonRequest> = read_message(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_message_truncated_length_returns_error() {
        // Only 2 bytes instead of 4.
        let mut cursor = Cursor::new(vec![0u8, 1]);
        let result: Result<DaemonRequest> = read_message(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_message_truncated_payload_returns_error() {
        // Length says 100 bytes but only 5 are present.
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(b"short");
        let mut cursor = Cursor::new(buf);
        let result: Result<DaemonRequest> = read_message(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_message_rejects_oversized() {
        // Length claims to be 32 MiB — should be rejected.
        let mut buf = Vec::new();
        buf.extend_from_slice(&(32 * 1024 * 1024u32).to_be_bytes());
        let mut cursor = Cursor::new(buf);
        let result: Result<DaemonRequest> = read_message(&mut cursor);
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_message_format_is_length_prefixed() {
        let req = DaemonRequest::Shutdown;
        let mut buf = Vec::new();
        write_message(&mut buf, &req).unwrap();

        // First 4 bytes should be the big-endian length of the JSON payload.
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(len, buf.len() - 4);

        // The payload should be valid JSON.
        let payload = &buf[4..];
        let _: DaemonRequest = serde_json::from_slice(payload).unwrap();
    }

    #[test]
    fn test_multiple_messages_on_same_stream() {
        let mut buf = Vec::new();
        write_message(&mut buf, &DaemonRequest::Status).unwrap();
        write_message(&mut buf, &DaemonRequest::Shutdown).unwrap();

        let mut cursor = Cursor::new(buf);
        let first: DaemonRequest = read_message(&mut cursor).unwrap();
        let second: DaemonRequest = read_message(&mut cursor).unwrap();
        assert_eq!(first, DaemonRequest::Status);
        assert_eq!(second, DaemonRequest::Shutdown);
    }
}
