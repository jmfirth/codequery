//! Synchronous daemon client for cq.
//!
//! Provides a synchronous client that connects to a running cq daemon over a
//! Unix domain socket. Used by the resolution cascade to query LSP servers
//! managed by the daemon without paying startup costs.

use std::os::unix::net::UnixStream;
use std::path::Path;

use codequery_core::Language;

use crate::error::{LspError, Result};
use crate::pid;
use crate::socket::{read_message, write_message, DaemonRequest, DaemonResponse};
use crate::types::LspLocation;

/// Synchronous client for querying the cq daemon.
///
/// Connects to a running daemon via Unix domain socket and issues single
/// request/response exchanges. The connection is held open for the lifetime
/// of the client, allowing multiple queries on the same connection.
#[derive(Debug)]
pub struct DaemonClient {
    /// The connected Unix stream to the daemon.
    stream: UnixStream,
}

impl DaemonClient {
    /// Connect to a running daemon.
    ///
    /// Looks up the daemon socket path and attempts to connect. Returns an
    /// error if the daemon is not running or the connection cannot be
    /// established.
    ///
    /// # Errors
    ///
    /// - `LspError::DaemonNotRunning` if no daemon PID file exists or the
    ///   process is not alive.
    /// - `LspError::ConnectionFailed` if the socket cannot be connected to.
    pub fn connect() -> Result<Self> {
        if !pid::is_daemon_running() {
            return Err(LspError::DaemonNotRunning);
        }

        let socket_path = pid::socket_path()?;
        let stream = UnixStream::connect(&socket_path).map_err(|e| {
            LspError::ConnectionFailed(format!(
                "failed to connect to daemon socket {}: {e}",
                socket_path.display()
            ))
        })?;

        Ok(Self { stream })
    }

    /// Query for references via the daemon.
    ///
    /// Sends a references query to the daemon and returns the resulting LSP
    /// locations. The daemon manages language server lifecycle and reuses
    /// warm servers.
    ///
    /// # Errors
    ///
    /// Returns an error if the request/response exchange fails or the daemon
    /// returns an error response.
    pub fn query_refs(
        &mut self,
        project: &Path,
        language: Language,
        file: &Path,
        line: usize,
        column: usize,
    ) -> Result<Vec<LspLocation>> {
        let request = DaemonRequest::Query {
            project: project.to_path_buf(),
            language: language_to_daemon_string(language),
            operation: "references".to_string(),
            file: file.to_path_buf(),
            line,
            column,
            symbol: None,
        };

        self.send_and_receive_locations(&request)
    }

    /// Query for definition via the daemon.
    ///
    /// Sends a definition query to the daemon and returns the resulting LSP
    /// locations.
    ///
    /// # Errors
    ///
    /// Returns an error if the request/response exchange fails or the daemon
    /// returns an error response.
    pub fn query_definition(
        &mut self,
        project: &Path,
        language: Language,
        file: &Path,
        line: usize,
        column: usize,
    ) -> Result<Vec<LspLocation>> {
        let request = DaemonRequest::Query {
            project: project.to_path_buf(),
            language: language_to_daemon_string(language),
            operation: "definition".to_string(),
            file: file.to_path_buf(),
            line,
            column,
            symbol: None,
        };

        self.send_and_receive_locations(&request)
    }

    /// Get daemon status.
    ///
    /// Returns the daemon's status response including running servers and
    /// uptime information.
    ///
    /// # Errors
    ///
    /// Returns an error if the request/response exchange fails.
    pub fn status(&mut self) -> Result<DaemonResponse> {
        write_message(&mut self.stream, &DaemonRequest::Status)?;
        read_message(&mut self.stream)
    }

    /// Tell the daemon to shut down.
    ///
    /// Sends a shutdown request and waits for the acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns an error if the request/response exchange fails.
    pub fn shutdown(&mut self) -> Result<()> {
        write_message(&mut self.stream, &DaemonRequest::Shutdown)?;
        let response: DaemonResponse = read_message(&mut self.stream)?;
        match response {
            DaemonResponse::Error(msg) => Err(LspError::ConnectionFailed(msg)),
            _ => Ok(()),
        }
    }

    /// Sends a request and extracts locations from the response.
    fn send_and_receive_locations(&mut self, request: &DaemonRequest) -> Result<Vec<LspLocation>> {
        write_message(&mut self.stream, request)?;
        let response: DaemonResponse = read_message(&mut self.stream)?;

        match response {
            DaemonResponse::Locations(locs) => Ok(locs),
            DaemonResponse::Error(msg) => Err(LspError::ConnectionFailed(format!(
                "daemon query error: {msg}"
            ))),
            other => Err(LspError::ConnectionFailed(format!(
                "unexpected daemon response: {other:?}"
            ))),
        }
    }
}

/// Converts a `Language` enum to the string format expected by the daemon protocol.
///
/// The daemon parses language strings via `Language::from_name`, which accepts
/// lowercase language names.
fn language_to_daemon_string(language: Language) -> String {
    format!("{language:?}").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ─── language_to_daemon_string ────────────────────────────────────

    #[test]
    fn test_language_to_daemon_string_rust() {
        assert_eq!(language_to_daemon_string(Language::Rust), "rust");
    }

    #[test]
    fn test_language_to_daemon_string_python() {
        assert_eq!(language_to_daemon_string(Language::Python), "python");
    }

    #[test]
    fn test_language_to_daemon_string_typescript() {
        assert_eq!(
            language_to_daemon_string(Language::TypeScript),
            "typescript"
        );
    }

    #[test]
    fn test_language_to_daemon_string_javascript() {
        assert_eq!(
            language_to_daemon_string(Language::JavaScript),
            "javascript"
        );
    }

    #[test]
    fn test_language_to_daemon_string_go() {
        assert_eq!(language_to_daemon_string(Language::Go), "go");
    }

    #[test]
    fn test_language_to_daemon_string_c() {
        assert_eq!(language_to_daemon_string(Language::C), "c");
    }

    #[test]
    fn test_language_to_daemon_string_cpp() {
        assert_eq!(language_to_daemon_string(Language::Cpp), "cpp");
    }

    #[test]
    fn test_language_to_daemon_string_java() {
        assert_eq!(language_to_daemon_string(Language::Java), "java");
    }

    #[test]
    fn test_language_to_daemon_string_roundtrips_via_from_name() {
        let languages = [
            Language::Rust,
            Language::TypeScript,
            Language::JavaScript,
            Language::Python,
            Language::Go,
            Language::C,
            Language::Cpp,
            Language::Java,
        ];
        for lang in languages {
            let s = language_to_daemon_string(lang);
            let parsed = Language::from_name(&s);
            assert_eq!(parsed, Some(lang), "roundtrip failed for {lang:?} -> {s:?}");
        }
    }

    // ─── DaemonClient::connect ────────────────────────────────────────

    #[test]
    fn test_connect_when_daemon_not_running_returns_error() {
        // No daemon is running during tests, so connect should fail.
        let result = DaemonClient::connect();
        assert!(result.is_err());
    }

    #[test]
    fn test_connect_error_is_daemon_not_running_or_connection_failed() {
        let result = DaemonClient::connect();
        let err = result.unwrap_err();
        // Either the daemon is not running (no PID file) or the socket
        // connection failed (PID file exists but socket is stale).
        assert!(
            matches!(
                err,
                LspError::DaemonNotRunning | LspError::ConnectionFailed(_)
            ),
            "unexpected error variant: {err:?}"
        );
    }

    // ─── Protocol message construction ────────────────────────────────

    #[test]
    fn test_query_refs_builds_correct_request() {
        let request = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: language_to_daemon_string(Language::Rust),
            operation: "references".to_string(),
            file: PathBuf::from("/project/src/main.rs"),
            line: 10,
            column: 4,
            symbol: None,
        };

        match &request {
            DaemonRequest::Query {
                project,
                language,
                operation,
                file,
                line,
                column,
                symbol,
            } => {
                assert_eq!(project, &PathBuf::from("/project"));
                assert_eq!(language, "rust");
                assert_eq!(operation, "references");
                assert_eq!(file, &PathBuf::from("/project/src/main.rs"));
                assert_eq!(*line, 10);
                assert_eq!(*column, 4);
                assert!(symbol.is_none());
            }
            _ => panic!("expected Query variant"),
        }
    }

    #[test]
    fn test_query_definition_builds_correct_request() {
        let request = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: language_to_daemon_string(Language::Python),
            operation: "definition".to_string(),
            file: PathBuf::from("/project/app.py"),
            line: 5,
            column: 0,
            symbol: None,
        };

        match &request {
            DaemonRequest::Query {
                language,
                operation,
                ..
            } => {
                assert_eq!(language, "python");
                assert_eq!(operation, "definition");
            }
            _ => panic!("expected Query variant"),
        }
    }

    // ─── Socket-based integration test ────────────────────────────────

    #[test]
    fn test_send_and_receive_via_socketpair() {
        use std::io::Cursor;

        // Simulate a DaemonResponse::Locations being received.
        let response = DaemonResponse::Locations(vec![LspLocation {
            uri: "file:///src/main.rs".to_string(),
            range: crate::types::Range::new(
                crate::types::Position::new(5, 0),
                crate::types::Position::new(5, 10),
            ),
        }]);

        // Write the response to a buffer.
        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

        // Read it back.
        let mut cursor = Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();

        match parsed {
            DaemonResponse::Locations(locs) => {
                assert_eq!(locs.len(), 1);
                assert_eq!(locs[0].uri, "file:///src/main.rs");
                assert_eq!(locs[0].range.start.line, 5);
            }
            _ => panic!("expected Locations response"),
        }
    }

    #[test]
    fn test_error_response_is_propagated() {
        use std::io::Cursor;

        let response = DaemonResponse::Error("server crashed".to_string());
        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();

        match parsed {
            DaemonResponse::Error(msg) => {
                assert_eq!(msg, "server crashed");
            }
            _ => panic!("expected Error response"),
        }
    }
}
