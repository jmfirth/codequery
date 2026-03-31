//! Synchronous daemon client for cq.
//!
//! Provides a synchronous client that connects to a running cq daemon over a
//! TCP socket on localhost. Used by the resolution cascade to query LSP servers
//! managed by the daemon without paying startup costs.

use std::net::TcpStream;
use std::path::Path;

use codequery_core::Language;

use crate::daemon_file;
use crate::error::{LspError, Result};
use crate::socket::{read_message, write_message, DaemonRequest, DaemonResponse};
use crate::types::LspLocation;

/// Synchronous client for querying the cq daemon.
///
/// Connects to a running daemon via TCP on localhost and issues single
/// request/response exchanges. The connection is held open for the lifetime
/// of the client, allowing multiple queries on the same connection.
#[derive(Debug)]
pub struct DaemonClient {
    /// The connected TCP stream to the daemon.
    stream: TcpStream,
}

impl DaemonClient {
    /// Connect to a running daemon for the given project.
    ///
    /// Reads the daemon info file, connects via TCP, and authenticates with
    /// the stored token. Returns an error if the daemon is not running or
    /// the connection cannot be established.
    ///
    /// # Errors
    ///
    /// - `LspError::DaemonNotRunning` if no daemon info file exists or the
    ///   daemon is not reachable.
    /// - `LspError::ConnectionFailed` if the TCP connection or authentication
    ///   fails.
    pub fn connect(project_root: &Path) -> Result<Self> {
        let info = daemon_file::read_daemon_info(project_root).ok_or(LspError::DaemonNotRunning)?;

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
        let mut stream = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(2))
            .map_err(|e| {
                // Connection failed — remove stale daemon file.
                daemon_file::remove_daemon_file(project_root);
                LspError::ConnectionFailed(format!(
                    "failed to connect to daemon at 127.0.0.1:{}: {e}",
                    info.port
                ))
            })?;

        // Authenticate with the daemon.
        write_message(
            &mut stream,
            &DaemonRequest::Authenticate { token: info.token },
        )?;

        let response: DaemonResponse = read_message(&mut stream)?;
        match response {
            DaemonResponse::AuthOk => {}
            DaemonResponse::Error(msg) => {
                return Err(LspError::ConnectionFailed(format!(
                    "daemon authentication failed: {msg}"
                )));
            }
            _ => {
                return Err(LspError::ConnectionFailed(
                    "unexpected response during authentication".to_string(),
                ));
            }
        }

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
    use std::net::TcpListener as TestTcpListener;
    use std::path::PathBuf;

    /// Creates a TCP pair (client, server) for testing.
    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TestTcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

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
        let result = DaemonClient::connect(Path::new("/nonexistent/project/test123"));
        assert!(result.is_err());
    }

    #[test]
    fn test_connect_error_is_daemon_not_running_or_connection_failed() {
        let result = DaemonClient::connect(Path::new("/nonexistent/project/test456"));
        let err = result.unwrap_err();
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

    // ─── Buffer-based integration test ────────────────────────────────

    #[test]
    fn test_send_and_receive_via_buffer() {
        use std::io::Cursor;

        let response = DaemonResponse::Locations(vec![LspLocation {
            uri: "file:///src/main.rs".to_string(),
            range: crate::types::Range::new(
                crate::types::Position::new(5, 0),
                crate::types::Position::new(5, 10),
            ),
        }]);

        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

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

    // ─── language_to_daemon_string completeness ─────────────────────

    #[test]
    fn test_language_to_daemon_string_all_tier1_and_tier2() {
        let languages = [
            (Language::Rust, "rust"),
            (Language::TypeScript, "typescript"),
            (Language::JavaScript, "javascript"),
            (Language::Python, "python"),
            (Language::Go, "go"),
            (Language::C, "c"),
            (Language::Cpp, "cpp"),
            (Language::Java, "java"),
            (Language::Ruby, "ruby"),
            (Language::Php, "php"),
            (Language::CSharp, "csharp"),
            (Language::Swift, "swift"),
            (Language::Kotlin, "kotlin"),
            (Language::Scala, "scala"),
            (Language::Zig, "zig"),
            (Language::Lua, "lua"),
            (Language::Bash, "bash"),
            (Language::Html, "html"),
            (Language::Css, "css"),
            (Language::Json, "json"),
            (Language::Yaml, "yaml"),
            (Language::Toml, "toml"),
        ];

        for (lang, expected) in languages {
            let s = language_to_daemon_string(lang);
            assert_eq!(s, expected, "language_to_daemon_string({lang:?})");
        }
    }

    // ─── DaemonRequest construction helpers ─────────────────────────

    #[test]
    fn test_query_refs_request_structure() {
        let request = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: language_to_daemon_string(Language::Go),
            operation: "references".to_string(),
            file: PathBuf::from("/project/main.go"),
            line: 20,
            column: 8,
            symbol: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["Query"]["language"], "go");
        assert_eq!(json["Query"]["operation"], "references");
        assert_eq!(json["Query"]["line"], 20);
    }

    #[test]
    fn test_query_definition_request_structure() {
        let request = DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: language_to_daemon_string(Language::TypeScript),
            operation: "definition".to_string(),
            file: PathBuf::from("/project/app.ts"),
            line: 15,
            column: 3,
            symbol: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["Query"]["language"], "typescript");
        assert_eq!(json["Query"]["operation"], "definition");
    }

    // ─── Status and Shutdown request construction ───────────────────

    #[test]
    fn test_status_request_serializes() {
        let request = DaemonRequest::Status;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, "\"Status\"");
    }

    #[test]
    fn test_shutdown_request_serializes() {
        let request = DaemonRequest::Shutdown;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, "\"Shutdown\"");
    }

    // ─── DaemonResponse matching in send_and_receive_locations ──────

    #[test]
    fn test_ok_response_is_unexpected_for_locations_query() {
        use std::io::Cursor;

        let response = DaemonResponse::Ok;
        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

        let mut cursor = Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();

        match parsed {
            DaemonResponse::Locations(_) => panic!("should not be Locations"),
            DaemonResponse::Error(_) => panic!("should not be Error"),
            other => {
                let msg = format!("unexpected daemon response: {other:?}");
                assert!(msg.contains("Ok"));
            }
        }
    }

    // ─── Hover response handling ────────────────────────────────────

    #[test]
    fn test_hover_response_serialization() {
        let response = DaemonResponse::Hover(Some("fn foo() -> i32".to_string()));
        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();

        match parsed {
            DaemonResponse::Hover(Some(text)) => {
                assert_eq!(text, "fn foo() -> i32");
            }
            _ => panic!("expected Hover response"),
        }
    }

    #[test]
    fn test_status_response_serialization_with_servers() {
        use crate::socket::ServerInfo;

        let response = DaemonResponse::Status {
            servers: vec![
                ServerInfo {
                    project: PathBuf::from("/project1"),
                    language: "rust".to_string(),
                    uptime_secs: 60,
                },
                ServerInfo {
                    project: PathBuf::from("/project2"),
                    language: "python".to_string(),
                    uptime_secs: 120,
                },
            ],
            uptime_secs: 300,
        };
        let mut buf = Vec::new();
        write_message(&mut buf, &response).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let parsed: DaemonResponse = read_message(&mut cursor).unwrap();

        match parsed {
            DaemonResponse::Status {
                servers,
                uptime_secs,
            } => {
                assert_eq!(servers.len(), 2);
                assert_eq!(uptime_secs, 300);
            }
            _ => panic!("expected Status response"),
        }
    }

    // ─── DaemonClient methods via mock TCP pair ─────────────────────

    #[test]
    fn test_status_via_mock_tcp() {
        use crate::socket::ServerInfo;

        let status_resp = DaemonResponse::Status {
            servers: vec![ServerInfo {
                project: PathBuf::from("/project"),
                language: "rust".to_string(),
                uptime_secs: 60,
            }],
            uptime_secs: 120,
        };

        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(&mut daemon_stream, &status_resp).unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let resp = client.status().unwrap();

        match resp {
            DaemonResponse::Status {
                servers,
                uptime_secs,
            } => {
                assert_eq!(servers.len(), 1);
                assert_eq!(uptime_secs, 120);
            }
            other => panic!("expected Status, got: {other:?}"),
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_shutdown_via_mock_tcp() {
        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(&mut daemon_stream, &DaemonResponse::Ok).unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        client.shutdown().unwrap();

        handle.join().unwrap();
    }

    #[test]
    fn test_shutdown_error_response_returns_error() {
        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(
                &mut daemon_stream,
                &DaemonResponse::Error("shutdown refused".to_string()),
            )
            .unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let result = client.shutdown();
        assert!(result.is_err());

        handle.join().unwrap();
    }

    #[test]
    fn test_query_refs_via_mock_tcp() {
        let locations_resp = DaemonResponse::Locations(vec![LspLocation {
            uri: "file:///src/main.rs".to_string(),
            range: crate::types::Range::new(
                crate::types::Position::new(5, 0),
                crate::types::Position::new(5, 10),
            ),
        }]);

        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(&mut daemon_stream, &locations_resp).unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let locs = client
            .query_refs(
                Path::new("/project"),
                Language::Rust,
                Path::new("/project/src/main.rs"),
                5,
                0,
            )
            .unwrap();

        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].uri, "file:///src/main.rs");

        handle.join().unwrap();
    }

    #[test]
    fn test_query_definition_via_mock_tcp() {
        let locations_resp = DaemonResponse::Locations(vec![LspLocation {
            uri: "file:///src/lib.rs".to_string(),
            range: crate::types::Range::new(
                crate::types::Position::new(10, 4),
                crate::types::Position::new(10, 12),
            ),
        }]);

        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(&mut daemon_stream, &locations_resp).unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let locs = client
            .query_definition(
                Path::new("/project"),
                Language::Rust,
                Path::new("/project/src/main.rs"),
                5,
                0,
            )
            .unwrap();

        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].uri, "file:///src/lib.rs");

        handle.join().unwrap();
    }

    #[test]
    fn test_query_refs_error_response() {
        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(
                &mut daemon_stream,
                &DaemonResponse::Error("server crashed".to_string()),
            )
            .unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let result = client.query_refs(
            Path::new("/project"),
            Language::Rust,
            Path::new("/project/src/main.rs"),
            1,
            0,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("server crashed"));

        handle.join().unwrap();
    }

    #[test]
    fn test_query_refs_unexpected_response_type() {
        let (client_stream, mut daemon_stream) = tcp_pair();

        let handle = std::thread::spawn(move || {
            let _req: crate::socket::DaemonRequest = read_message(&mut daemon_stream).unwrap();
            write_message(
                &mut daemon_stream,
                &DaemonResponse::Status {
                    servers: vec![],
                    uptime_secs: 0,
                },
            )
            .unwrap();
        });

        let mut client = DaemonClient {
            stream: client_stream,
        };
        let result = client.query_refs(
            Path::new("/project"),
            Language::Rust,
            Path::new("/project/src/main.rs"),
            1,
            0,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unexpected"));

        handle.join().unwrap();
    }
}
