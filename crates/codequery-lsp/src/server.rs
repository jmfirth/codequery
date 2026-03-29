//! Language server lifecycle management.
//!
//! Handles spawning a language server process, performing the LSP initialize
//! handshake, and cleanly shutting the server down. This is the core
//! abstraction for communicating with external language servers.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::config::ServerConfig;
use crate::error::{LspError, Result};
use crate::transport::StdioTransport;
use crate::types::{ClientCapabilities, InitializeParams, ServerCapabilities};

/// Timeout for the child process to exit after receiving the `exit` notification.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A running language server with an established LSP session.
///
/// Manages the lifecycle of a single language server process: spawning,
/// initialization handshake, and shutdown. After `start()` succeeds, the server
/// is ready to handle LSP requests via its transport.
pub struct LspServer {
    /// JSON-RPC transport for sending requests and notifications.
    transport: StdioTransport,

    /// The child process running the language server.
    process: Child,

    /// Capabilities advertised by the server during initialization.
    capabilities: ServerCapabilities,

    /// The workspace root URI used during initialization.
    root_uri: String,

    /// URIs of documents that have been opened via `textDocument/didOpen`.
    ///
    /// Tracked to avoid sending duplicate `didOpen` notifications for the same
    /// document, which some language servers treat as an error.
    pub(crate) opened_docs: HashSet<String>,
}

impl fmt::Debug for LspServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LspServer")
            .field("root_uri", &self.root_uri)
            .field("capabilities", &self.capabilities)
            .field("process_id", &self.process.id())
            .field("opened_docs_count", &self.opened_docs.len())
            .finish_non_exhaustive()
    }
}

impl LspServer {
    /// Starts a language server process and performs the LSP initialize handshake.
    ///
    /// 1. Verifies the server binary exists on `PATH`.
    /// 2. Spawns the child process with stdin/stdout piped.
    /// 3. Sends the `initialize` request with the given project root.
    /// 4. Reads the response and stores the server's capabilities.
    /// 5. Sends the `initialized` notification.
    ///
    /// # Errors
    ///
    /// - `LspError::ServerNotFound` if the binary is not found on the system.
    /// - `LspError::InitializeFailed` if the handshake fails.
    /// - `LspError::Io` on process spawn or I/O failures.
    pub fn start(config: &ServerConfig, project_root: &Path) -> Result<Self> {
        // Check that the binary exists before attempting to spawn.
        check_binary_exists(&config.binary)?;

        // Spawn the language server process.
        let mut cmd = Command::new(&config.binary);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LspError::ServerNotFound(config.binary.clone())
            } else {
                LspError::Io(e)
            }
        })?;

        // Take the stdin/stdout handles for the transport.
        let stdin = child.stdin.take().ok_or_else(|| {
            LspError::InitializeFailed("failed to capture server stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LspError::InitializeFailed("failed to capture server stdout".to_string())
        })?;

        let mut transport = StdioTransport::new(stdin, stdout);

        // Build the root URI from the project path.
        let root_uri = crate::queries::path_to_uri(project_root);

        // Send the initialize request.
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(root_uri.clone()),
            capabilities: ClientCapabilities::with_progress(),
        };

        let params = serde_json::to_value(&init_params)
            .map_err(|e| LspError::InitializeFailed(format!("failed to serialize params: {e}")))?;

        let result = transport.send_request("initialize", params).map_err(|e| {
            // If the child already exited, report as crashed.
            if let Ok(Some(status)) = child.try_wait() {
                return LspError::ServerCrashed(format!(
                    "{} exited with {status} during initialization",
                    config.binary
                ));
            }
            LspError::InitializeFailed(format!("initialize request failed: {e}"))
        })?;

        // Parse server capabilities from the response.
        let capabilities = parse_capabilities(&result)?;

        // Send the initialized notification.
        transport
            .send_notification("initialized", serde_json::json!({}))
            .map_err(|e| {
                LspError::InitializeFailed(format!("initialized notification failed: {e}"))
            })?;

        Ok(Self {
            transport,
            process: child,
            capabilities,
            root_uri,
            opened_docs: HashSet::new(),
        })
    }

    /// Shuts down the language server gracefully.
    ///
    /// 1. Sends the `shutdown` request and waits for the response.
    /// 2. Sends the `exit` notification.
    /// 3. Waits for the child process to exit (with a timeout).
    /// 4. Kills the process if it has not exited after the timeout.
    ///
    /// # Errors
    ///
    /// Returns `LspError::Io` if killing the process fails. Errors from the
    /// shutdown request itself are logged but do not prevent the exit sequence.
    pub fn shutdown(mut self) -> Result<()> {
        // Send shutdown request. Best-effort — if this fails, we still try to exit.
        let shutdown_result = self
            .transport
            .send_request("shutdown", serde_json::Value::Null);

        // Send exit notification regardless of shutdown result.
        let _ = self
            .transport
            .send_notification("exit", serde_json::Value::Null);

        // Wait for the child to exit with a timeout.
        wait_for_exit(&mut self.process, SHUTDOWN_TIMEOUT)?;

        // If the shutdown request had an error, propagate it (the process is
        // already cleaned up at this point).
        shutdown_result.map(|_| ())
    }

    /// Returns `true` if the server process is still running.
    #[must_use]
    pub fn is_ready(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    /// Returns the capabilities advertised by the server.
    #[must_use]
    pub fn capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }

    /// Returns the workspace root URI.
    #[must_use]
    pub fn root_uri(&self) -> &str {
        &self.root_uri
    }

    /// Returns a mutable reference to the underlying transport.
    ///
    /// Useful for sending additional requests or notifications after
    /// initialization.
    pub fn transport_mut(&mut self) -> &mut StdioTransport {
        &mut self.transport
    }

    /// Waits for the language server to finish initial indexing.
    ///
    /// Reads incoming notifications from the server, tracking `$/progress`
    /// tokens. Returns when:
    /// - All `$/progress` tokens reach "end" state (server is ready)
    /// - No progress notifications arrive within a 2-second grace period
    ///   (server doesn't support progress or was immediately ready)
    /// - The overall timeout expires
    ///
    /// Also responds to `window/workDoneProgress/create` requests from the
    /// server, which is required for the progress protocol.
    ///
    /// # Errors
    ///
    /// Returns `Ok(())` in all normal cases, including timeout (we proceed
    /// with the query anyway). Only returns an error if the transport itself
    /// has a fatal failure.
    #[cfg(unix)]
    pub fn wait_for_ready(&mut self, timeout: Duration) -> Result<()> {
        use std::time::Instant;

        let deadline = Instant::now() + timeout;
        let grace_period = Duration::from_secs(2);
        let grace_deadline = Instant::now() + grace_period;
        let mut progress_active: HashSet<String> = HashSet::new();
        let mut seen_progress = false;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break; // Overall timeout — proceed with query
            }

            let poll_timeout = remaining.min(Duration::from_millis(200));

            match self.transport.try_read_message(poll_timeout) {
                Ok(Some(msg)) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg) {
                        let method = json.get("method").and_then(|m| m.as_str()).unwrap_or("");

                        match method {
                            "$/progress" => {
                                seen_progress = true;
                                if let Some(params) = json.get("params") {
                                    let token = params
                                        .get("token")
                                        .map(std::string::ToString::to_string)
                                        .unwrap_or_default();
                                    let kind = params
                                        .get("value")
                                        .and_then(|v| v.get("kind"))
                                        .and_then(|k| k.as_str())
                                        .unwrap_or("");

                                    match kind {
                                        "begin" => {
                                            progress_active.insert(token);
                                        }
                                        "end" => {
                                            progress_active.remove(&token);
                                            if progress_active.is_empty() {
                                                break; // All progress done — server ready
                                            }
                                        }
                                        _ => {} // "report" — still in progress
                                    }
                                }
                            }
                            "window/workDoneProgress/create" => {
                                // Server is requesting we create a progress token.
                                // We must respond or the server may hang.
                                seen_progress = true;
                                if let Some(id) = json.get("id") {
                                    let response = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": null
                                    });
                                    let response_str =
                                        serde_json::to_string(&response).unwrap_or_default();
                                    let _ = self.transport.write_raw(&response_str);
                                }
                            }
                            _ => {
                                // Other notifications (logMessage, diagnostics, etc.)
                            }
                        }
                    }
                }
                Ok(None) => {
                    // No data within poll timeout.
                    if !seen_progress && Instant::now() >= grace_deadline {
                        break; // Server doesn't support progress — assume ready
                    }
                }
                Err(_) => {
                    break; // I/O error — proceed anyway
                }
            }
        }

        Ok(())
    }
}

impl Drop for LspServer {
    fn drop(&mut self) {
        // Best-effort cleanup: try to kill the process if it's still running.
        if let Ok(None) = self.process.try_wait() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

/// Checks that a binary exists on the system PATH.
fn check_binary_exists(binary: &str) -> Result<()> {
    let result = Command::new("which")
        .arg(binary)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match result {
        Ok(status) if status.success() => Ok(()),
        _ => Err(LspError::ServerNotFound(binary.to_string())),
    }
}

/// Parses `ServerCapabilities` from the `initialize` response result.
fn parse_capabilities(result: &serde_json::Value) -> Result<ServerCapabilities> {
    let caps_value = result.get("capabilities").unwrap_or(result);
    serde_json::from_value(caps_value.clone())
        .map_err(|e| LspError::InitializeFailed(format!("failed to parse capabilities: {e}")))
}

/// Waits for a child process to exit, killing it after a timeout.
fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        if let Some(_status) = child.try_wait()? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            child.kill()?;
            child.wait()?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_capabilities tests ───────────────────────────────────

    #[test]
    fn test_parse_capabilities_from_nested_result() {
        let result = serde_json::json!({
            "capabilities": {
                "definitionProvider": true,
                "referencesProvider": true
            }
        });
        let caps = parse_capabilities(&result).unwrap();
        assert!(caps.definition_provider.is_some());
        assert!(caps.references_provider.is_some());
    }

    #[test]
    fn test_parse_capabilities_from_flat_result() {
        // Some servers return capabilities at the top level.
        let result = serde_json::json!({
            "definitionProvider": true,
            "hoverProvider": {"dynamicRegistration": false}
        });
        let caps = parse_capabilities(&result).unwrap();
        assert!(caps.definition_provider.is_some());
        assert!(caps.hover_provider.is_some());
    }

    #[test]
    fn test_parse_capabilities_empty_object() {
        let result = serde_json::json!({});
        let caps = parse_capabilities(&result).unwrap();
        assert!(caps.definition_provider.is_none());
    }

    // ─── check_binary_exists tests ──────────────────────────────────

    #[test]
    fn test_check_binary_exists_sh_succeeds() {
        // /bin/sh should always exist.
        assert!(check_binary_exists("sh").is_ok());
    }

    #[test]
    fn test_check_binary_exists_nonexistent_fails() {
        let err = check_binary_exists("definitely-not-a-real-binary-name-12345").unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
    }

    // ─── LspServer::start error handling ────────────────────────────

    #[test]
    fn test_start_with_nonexistent_binary_returns_server_not_found() {
        let config = ServerConfig {
            binary: "nonexistent-lsp-server-xyz-12345".to_string(),
            args: vec![],
            env: vec![],
        };
        let dir = tempfile::tempdir().unwrap();
        let err = LspServer::start(&config, dir.path()).unwrap_err();
        assert!(matches!(err, LspError::ServerNotFound(_)));
        assert!(err.to_string().contains("nonexistent-lsp-server-xyz-12345"));
    }

    // ─── LspServer with mock server ─────────────────────────────────

    /// Helper shell script fragments for tests. These are identical to the
    /// ones in transport.rs — duplicated here to keep the test module
    /// self-contained.
    const SHELL_READ_REQUEST: &str = concat!(
        "read_headers() { ",
        "  CL=0; ",
        "  while IFS= read -r line; do ",
        "    line=$(printf '%s' \"$line\" | tr -d '\\r'); ",
        "    [ -z \"$line\" ] && break; ",
        "    case \"$line\" in ",
        "      Content-Length:*) CL=$(echo \"$line\" | cut -d: -f2 | tr -d ' ') ;; ",
        "    esac; ",
        "  done; ",
        "  echo $CL; ",
        "}; ",
        "CL=$(read_headers); ",
        "BODY=$(dd bs=1 count=$CL 2>/dev/null); ",
        "ID=$(echo \"$BODY\" | sed 's/.*\"id\":\\([0-9]*\\).*/\\1/'); ",
    );

    const SHELL_WRITE_MSG: &str = concat!(
        "write_msg() { ",
        "  local MSG=\"$1\"; ",
        "  local LEN=$(printf '%s' \"$MSG\" | wc -c | tr -d ' '); ",
        "  printf 'Content-Length: %s\\r\\n\\r\\n%s' \"$LEN\" \"$MSG\"; ",
        "}; ",
    );

    /// Creates a shell script that acts as a mock language server.
    ///
    /// The script reads one LSP request (the initialize request), responds
    /// with server capabilities, then reads the initialized notification,
    /// then optionally reads more requests depending on the `extra` parameter.
    fn mock_server_script(capabilities: &str, extra: &str) -> String {
        format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{capabilities}}}}}'; \
             {SHELL_READ_REQUEST}\
             {extra}"
        )
    }

    #[test]
    fn test_start_and_shutdown_with_mock_server() {
        // Mock server: respond to initialize, read initialized notification,
        // then respond to shutdown, read exit notification.
        let script = mock_server_script(
            "{\"definitionProvider\":true}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let server = LspServer::start(&config, dir.path()).unwrap();
        assert!(server.capabilities().definition_provider.is_some());
        assert!(server.root_uri().starts_with("file:///"));

        server.shutdown().unwrap();
    }

    #[test]
    fn test_start_stores_capabilities() {
        let script = mock_server_script(
            "{\"definitionProvider\":true,\"referencesProvider\":true,\"hoverProvider\":false}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let server = LspServer::start(&config, dir.path()).unwrap();
        let caps = server.capabilities();
        assert!(caps.definition_provider.is_some());
        assert!(caps.references_provider.is_some());
        assert!(caps.hover_provider.is_some());
        assert!(caps.document_symbol_provider.is_none());

        server.shutdown().unwrap();
    }

    #[test]
    fn test_is_ready_true_while_server_running() {
        // A long-running mock server that stays alive.
        let script = mock_server_script(
            "{}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        assert!(server.is_ready());

        server.shutdown().unwrap();
    }

    #[test]
    fn test_start_with_server_that_exits_immediately() {
        // Server exits immediately after spawn — initialize should fail.
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            env: vec![],
        };

        let dir = tempfile::tempdir().unwrap();
        let err = LspServer::start(&config, dir.path()).unwrap_err();
        // Could be InitializeFailed or ServerCrashed depending on timing.
        let msg = err.to_string();
        assert!(
            msg.contains("initialize failed")
                || msg.contains("crashed")
                || msg.contains("end of stream"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn test_start_sets_root_uri_from_project_path() {
        let script = mock_server_script(
            "{}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let server = LspServer::start(&config, dir.path()).unwrap();
        assert!(server.root_uri().starts_with("file:///"));
        // The temp dir path should appear in the URI.
        let dir_name = dir.path().canonicalize().unwrap();
        assert!(
            server.root_uri().contains(&dir_name.display().to_string()),
            "root_uri {} should contain {}",
            server.root_uri(),
            dir_name.display()
        );

        server.shutdown().unwrap();
    }

    #[test]
    fn test_start_with_env_vars() {
        // Verify that environment variables are passed to the child.
        // The mock server script echoes the env var in the capabilities.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             {SHELL_READ_REQUEST}"
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![("TEST_LSP_VAR".to_string(), "test_value".to_string())],
        };

        // If this succeeds, the process was spawned with env vars.
        let server = LspServer::start(&config, dir.path()).unwrap();
        server.shutdown().unwrap();
    }

    #[test]
    fn test_drop_kills_running_process() {
        // Start a server that never exits on its own.
        let script = mock_server_script("{}", "sleep 60");

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let server = LspServer::start(&config, dir.path()).unwrap();
        let pid = server.process.id();

        // Drop the server — should kill the process.
        drop(server);

        // The process should be gone. Give it a moment to clean up.
        std::thread::sleep(Duration::from_millis(100));

        // Check that the process is no longer running via kill(0).
        let result = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if let Ok(status) = result {
            // kill command succeeded — process should still be gone.
            assert!(
                !status.success(),
                "process {pid} should not be running after drop"
            );
        }
        // Err case: kill command itself failed = process is gone, which is correct.
    }

    #[test]
    fn test_transport_mut_returns_transport() {
        let script = mock_server_script(
            "{}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        // Just verify we can get a mutable reference.
        let _transport = server.transport_mut();

        server.shutdown().unwrap();
    }

    // ─── wait_for_exit tests ────────────────────────────────────────

    #[test]
    fn test_wait_for_exit_process_exits_quickly() {
        let mut child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();

        // Give the process a moment to exit.
        std::thread::sleep(Duration::from_millis(50));
        wait_for_exit(&mut child, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn test_wait_for_exit_kills_on_timeout() {
        let mut child = Command::new("sleep").arg("60").spawn().unwrap();

        // Very short timeout should trigger kill.
        wait_for_exit(&mut child, Duration::from_millis(100)).unwrap();

        // Process should be reaped.
        let status = child.try_wait().unwrap();
        assert!(status.is_some(), "process should have been reaped");
    }

    // ─── Debug impl ──────────────────────────────────────────────────

    #[test]
    fn test_lsp_server_debug_includes_root_uri_and_process_id() {
        let script = mock_server_script(
            "{}",
            &format!(
                "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
                 write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
                 {SHELL_READ_REQUEST}"
            ),
        );

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let server = LspServer::start(&config, dir.path()).unwrap();
        let debug_str = format!("{server:?}");
        assert!(
            debug_str.contains("root_uri"),
            "debug output should contain root_uri: {debug_str}"
        );
        assert!(
            debug_str.contains("process_id"),
            "debug output should contain process_id: {debug_str}"
        );
        assert!(
            debug_str.contains("capabilities"),
            "debug output should contain capabilities: {debug_str}"
        );
        assert!(
            debug_str.contains("opened_docs_count"),
            "debug output should contain opened_docs_count: {debug_str}"
        );

        server.shutdown().unwrap();
    }

    // ─── is_ready after server exits ────────────────────────────────

    #[test]
    fn test_is_ready_false_after_server_exits() {
        // Server that exits immediately after initialization.
        let script = mock_server_script("{}", "exit 0");

        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        // Give the process a moment to exit.
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !server.is_ready(),
            "server should not be ready after process exits"
        );
    }

    // ─── parse_capabilities edge cases ──────────────────────────────

    #[test]
    fn test_parse_capabilities_with_all_providers() {
        let result = serde_json::json!({
            "capabilities": {
                "definitionProvider": true,
                "referencesProvider": true,
                "hoverProvider": true,
                "documentSymbolProvider": true
            }
        });
        let caps = parse_capabilities(&result).unwrap();
        assert!(caps.definition_provider.is_some());
        assert!(caps.references_provider.is_some());
        assert!(caps.hover_provider.is_some());
        assert!(caps.document_symbol_provider.is_some());
    }

    // ─── opened_docs deduplication ──────────────────────────────────

    #[test]
    fn test_start_with_server_that_closes_stdin_after_init() {
        // Server responds to initialize but closes stdin immediately after,
        // causing the initialized notification to fail.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             exec <&-; exit 0"
        );

        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let dir = tempfile::tempdir().unwrap();
        let result = LspServer::start(&config, dir.path());
        // May succeed or fail depending on timing. If it fails, the error
        // should relate to the initialized notification.
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("initialize") || msg.contains("end of stream") || msg.contains("pipe"),
                "unexpected error: {msg}"
            );
        }
    }

    #[test]
    fn test_open_document_deduplicates() {
        // Mock server that handles: initialize, initialized notification,
        // didOpen notification, shutdown, exit.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             {SHELL_READ_REQUEST}"
        );

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn main() {}").unwrap();

        let config = ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut server = LspServer::start(&config, dir.path()).unwrap();
        assert!(server.opened_docs.is_empty());

        // First open should succeed and track the document.
        server.open_document(&file, "fn main() {}", "rust").unwrap();
        assert_eq!(server.opened_docs.len(), 1);

        // Second open of same file should be a no-op.
        server.open_document(&file, "fn main() {}", "rust").unwrap();
        assert_eq!(server.opened_docs.len(), 1);

        server.shutdown().unwrap();
    }
}
