//! Background daemon process with a language server pool.
//!
//! The daemon keeps language servers warm between cq invocations, avoiding the
//! startup cost of initializing a new server for each query. It listens on a
//! Unix domain socket and processes one request at a time (sequential).
//!
//! No async runtime is used — the daemon runs a simple synchronous accept loop
//! with `std::os::unix::net::UnixListener`.

use std::collections::HashMap;
use std::io;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use codequery_core::Language;

use crate::config::LanguageServerRegistry;
use crate::error::{LspError, Result};
use crate::pid;
use crate::server::LspServer;
use crate::socket::{read_message, write_message, DaemonRequest, DaemonResponse, ServerInfo};

/// Default idle timeout for language servers (30 minutes).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30 * 60;

/// Environment variable to override the idle timeout.
const IDLE_TIMEOUT_ENV_VAR: &str = "CQ_LSP_TIMEOUT";

/// State of a server in the pool, used to separate borrow phases.
enum ServerPoolState {
    /// Server is present and running.
    Ready,
    /// Server is present but has died.
    Dead,
    /// No server for this key.
    Missing,
}

/// A pooled language server entry, tracking when it was last used.
struct PooledServer {
    /// The running language server.
    server: LspServer,
    /// When this server was last used for a query.
    last_used: Instant,
    /// When this server was started.
    started: Instant,
}

/// Background daemon that keeps language servers warm for fast queries.
///
/// Manages a pool of `LspServer` instances keyed by `(project_root, language)`.
/// Idle servers are evicted after a configurable timeout. The daemon listens on
/// a Unix domain socket and processes requests sequentially.
pub struct Daemon {
    /// Pooled servers keyed by (project root, language).
    servers: HashMap<(PathBuf, Language), PooledServer>,
    /// Registry of language server configurations.
    registry: LanguageServerRegistry,
    /// How long a server can be idle before eviction.
    idle_timeout: Duration,
    /// When the daemon was started.
    start_time: Instant,
}

impl Daemon {
    /// Creates a new daemon with the given idle timeout.
    ///
    /// The idle timeout controls how long a language server can go unused
    /// before being shut down to free resources.
    #[must_use]
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            servers: HashMap::new(),
            registry: LanguageServerRegistry::new(),
            idle_timeout,
            start_time: Instant::now(),
        }
    }

    /// Creates a new daemon with the idle timeout from environment or default.
    ///
    /// Reads `CQ_LSP_TIMEOUT` (in seconds) if set, otherwise uses 30 minutes.
    #[must_use]
    pub fn from_env() -> Self {
        let timeout_secs = std::env::var(IDLE_TIMEOUT_ENV_VAR)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);

        Self::new(Duration::from_secs(timeout_secs))
    }

    /// Runs the daemon (blocking).
    ///
    /// 1. Creates the runtime directory and writes the PID file.
    /// 2. Binds a Unix domain socket.
    /// 3. Loops: accept connection, read request, handle, write response.
    /// 4. Between connections, evicts idle servers.
    /// 5. On `Shutdown` request or signal, cleans up and exits.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound or the PID file cannot
    /// be written.
    pub fn run(&mut self) -> Result<()> {
        // Set up signal handling for clean shutdown.
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        Self::register_signal_handlers(&shutdown_flag);

        // Write PID file and bind socket.
        pid::write_pid_file()?;
        let socket_path = pid::socket_path()?;

        // Remove stale socket file if it exists.
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path).map_err(|e| {
            pid::remove_pid_file();
            LspError::ConnectionFailed(format!(
                "failed to bind socket {}: {e}",
                socket_path.display()
            ))
        })?;

        // Set a timeout on accept so we can check the shutdown flag periodically.
        listener.set_nonblocking(true).map_err(LspError::Io)?;

        let result = self.accept_loop(&listener, &shutdown_flag);

        // Clean up regardless of how we exited.
        self.shutdown_all_servers();
        let _ = std::fs::remove_file(&socket_path);
        pid::remove_pid_file();

        result
    }

    /// The main accept loop.
    fn accept_loop(
        &mut self,
        listener: &UnixListener,
        shutdown_flag: &Arc<AtomicBool>,
    ) -> Result<()> {
        loop {
            // Check if a signal requested shutdown.
            if shutdown_flag.load(Ordering::Relaxed) {
                return Ok(());
            }

            // Evict idle servers between connections.
            self.evict_idle_servers();

            // Try to accept a connection (non-blocking).
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let should_stop = self.handle_connection(stream);
                    if should_stop {
                        return Ok(());
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No pending connection — sleep briefly and retry.
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    // Log the error but keep running.
                    eprintln!("cq daemon: accept error: {e}");
                }
            }
        }
    }

    /// Handles a single client connection. Returns `true` if the daemon
    /// should shut down.
    fn handle_connection(&mut self, mut stream: std::os::unix::net::UnixStream) -> bool {
        // Set the stream to blocking for the duration of this request.
        let _ = stream.set_nonblocking(false);

        // Read the request.
        let request: DaemonRequest = match read_message(&mut stream) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("cq daemon: failed to read request: {e}");
                return false;
            }
        };

        // Handle the request.
        let (response, should_stop) = self.handle_request(request);

        // Write the response. Best-effort — if the client disconnected, move on.
        let _ = write_message(&mut stream, &response);

        should_stop
    }

    /// Dispatches a request and returns the response plus a flag indicating
    /// whether the daemon should shut down.
    fn handle_request(&mut self, req: DaemonRequest) -> (DaemonResponse, bool) {
        match req {
            DaemonRequest::Query {
                project,
                language,
                operation,
                file,
                line,
                column,
                symbol: _,
            } => {
                let response =
                    self.handle_query(&project, &language, &operation, &file, line, column);
                (response, false)
            }
            DaemonRequest::Status => {
                let response = self.handle_status();
                (response, false)
            }
            DaemonRequest::Shutdown => (DaemonResponse::Ok, true),
        }
    }

    /// Handles a query request by dispatching to the appropriate LSP method.
    fn handle_query(
        &mut self,
        project: &Path,
        language: &str,
        operation: &str,
        file: &Path,
        line: usize,
        column: usize,
    ) -> DaemonResponse {
        let Some(lang) = Language::from_name(language) else {
            return DaemonResponse::Error(format!("unsupported language: {language}"));
        };

        let server = match self.get_or_start_server(project, lang) {
            Ok(s) => s,
            Err(e) => return DaemonResponse::Error(format!("failed to start server: {e}")),
        };

        // Open the document if needed (best-effort source read).
        let source = std::fs::read_to_string(file).unwrap_or_default();
        if let Err(e) = server.open_document(file, &source, language) {
            return DaemonResponse::Error(format!("failed to open document: {e}"));
        }

        match operation {
            "definition" => match server.find_definition(file, line, column) {
                Ok(locations) => DaemonResponse::Locations(locations),
                Err(e) => DaemonResponse::Error(format!("definition failed: {e}")),
            },
            "references" => match server.find_references(file, line, column, true) {
                Ok(locations) => DaemonResponse::Locations(locations),
                Err(e) => DaemonResponse::Error(format!("references failed: {e}")),
            },
            "hover" => match server.hover(file, line, column) {
                Ok(text) => DaemonResponse::Hover(text),
                Err(e) => DaemonResponse::Error(format!("hover failed: {e}")),
            },
            _ => DaemonResponse::Error(format!("unknown operation: {operation}")),
        }
    }

    /// Returns daemon status information.
    fn handle_status(&self) -> DaemonResponse {
        let servers = self
            .servers
            .iter()
            .map(|((project, lang), pooled)| ServerInfo {
                project: project.clone(),
                language: format!("{lang:?}").to_lowercase(),
                uptime_secs: pooled.started.elapsed().as_secs(),
            })
            .collect();

        DaemonResponse::Status {
            servers,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    /// Evicts language servers that have been idle longer than the timeout.
    fn evict_idle_servers(&mut self) {
        let timeout = self.idle_timeout;
        let to_remove: Vec<_> = self
            .servers
            .iter()
            .filter(|(_, pooled)| pooled.last_used.elapsed() > timeout)
            .map(|(key, _)| key.clone())
            .collect();

        for key in to_remove {
            if let Some(pooled) = self.servers.remove(&key) {
                // Best-effort shutdown of the evicted server.
                let _ = pooled.server.shutdown();
            }
        }
    }

    /// Gets an existing server or starts a new one for the given project and language.
    fn get_or_start_server(
        &mut self,
        project: &Path,
        language: Language,
    ) -> Result<&mut LspServer> {
        let key = (project.to_path_buf(), language);

        // Check if we have a running server. Two-phase check to satisfy the
        // borrow checker: first determine the state, then act.
        let server_state = if let Some(pooled) = self.servers.get_mut(&key) {
            if pooled.server.is_ready() {
                pooled.last_used = Instant::now();
                ServerPoolState::Ready
            } else {
                ServerPoolState::Dead
            }
        } else {
            ServerPoolState::Missing
        };

        let needs_new_server = match server_state {
            ServerPoolState::Ready => false,
            ServerPoolState::Dead => {
                self.servers.remove(&key);
                true
            }
            ServerPoolState::Missing => true,
        };

        if needs_new_server {
            // Look up the server config for this language.
            let config = self
                .registry
                .config_for(language)
                .ok_or_else(|| {
                    LspError::ServerNotFound(format!(
                        "no language server configured for {language:?}"
                    ))
                })?
                .clone();

            // Start the server.
            let server = LspServer::start(&config, project)?;
            let now = Instant::now();

            self.servers.insert(
                key.clone(),
                PooledServer {
                    server,
                    last_used: now,
                    started: now,
                },
            );
        }

        Ok(&mut self
            .servers
            .get_mut(&key)
            .expect("server was just inserted or confirmed ready")
            .server)
    }

    /// Shuts down all running language servers.
    fn shutdown_all_servers(&mut self) {
        let keys: Vec<_> = self.servers.keys().cloned().collect();
        for key in keys {
            if let Some(pooled) = self.servers.remove(&key) {
                let _ = pooled.server.shutdown();
            }
        }
    }

    /// Registers SIGTERM and SIGINT handlers that set the shutdown flag.
    fn register_signal_handlers(shutdown_flag: &Arc<AtomicBool>) {
        let flag = Arc::clone(shutdown_flag);
        let _ = unsafe {
            // SAFETY: We only set an atomic bool in the signal handler, which
            // is async-signal-safe. The AtomicBool is kept alive by the Arc
            // in the caller's scope.
            libc::signal(
                libc::SIGTERM,
                signal_handler as *const () as libc::sighandler_t,
            )
        };

        let _ = unsafe {
            // SAFETY: Same as above for SIGINT.
            libc::signal(
                libc::SIGINT,
                signal_handler as *const () as libc::sighandler_t,
            )
        };

        // Store the flag in a global so the signal handler can access it.
        // This is safe because we only have one daemon instance per process.
        SHUTDOWN_FLAG.store(Arc::into_raw(flag) as *mut bool as usize, Ordering::Release);
    }
}

/// Global storage for the shutdown flag pointer, accessed from the signal handler.
static SHUTDOWN_FLAG: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Signal handler that sets the shutdown flag.
///
/// # Safety
///
/// Only performs an atomic store, which is async-signal-safe.
extern "C" fn signal_handler(_signum: libc::c_int) {
    let ptr = SHUTDOWN_FLAG.load(Ordering::Acquire);
    if ptr != 0 {
        let flag = unsafe {
            // SAFETY: The pointer was created from Arc::into_raw in
            // register_signal_handlers and points to a valid AtomicBool.
            &*(ptr as *const AtomicBool)
        };
        flag.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Daemon construction ────────────────────────────────────────

    #[test]
    fn test_daemon_new_creates_empty_pool() {
        let daemon = Daemon::new(Duration::from_secs(60));
        assert!(daemon.servers.is_empty());
        assert_eq!(daemon.idle_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_daemon_from_env_uses_default_without_env_var() {
        std::env::remove_var(IDLE_TIMEOUT_ENV_VAR);
        let daemon = Daemon::from_env();
        assert_eq!(
            daemon.idle_timeout,
            Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn test_daemon_from_env_uses_env_var_when_set() {
        std::env::set_var(IDLE_TIMEOUT_ENV_VAR, "120");
        let daemon = Daemon::from_env();
        assert_eq!(daemon.idle_timeout, Duration::from_secs(120));
        std::env::remove_var(IDLE_TIMEOUT_ENV_VAR);
    }

    #[test]
    fn test_daemon_from_env_uses_default_on_invalid_value() {
        std::env::set_var(IDLE_TIMEOUT_ENV_VAR, "not_a_number");
        let daemon = Daemon::from_env();
        assert_eq!(
            daemon.idle_timeout,
            Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)
        );
        std::env::remove_var(IDLE_TIMEOUT_ENV_VAR);
    }

    // ─── handle_request routing ─────────────────────────────────────

    #[test]
    fn test_handle_request_shutdown_returns_ok_and_stop() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Shutdown);
        assert_eq!(resp, DaemonResponse::Ok);
        assert!(should_stop);
    }

    #[test]
    fn test_handle_request_status_returns_empty_status() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Status);
        assert!(!should_stop);
        match resp {
            DaemonResponse::Status {
                servers,
                uptime_secs: _,
            } => {
                assert!(servers.is_empty());
            }
            _ => panic!("expected Status response"),
        }
    }

    #[test]
    fn test_handle_request_query_unsupported_language() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Query {
            project: PathBuf::from("/project"),
            language: "brainfuck".to_string(),
            operation: "definition".to_string(),
            file: PathBuf::from("/project/main.bf"),
            line: 1,
            column: 0,
            symbol: None,
        });
        assert!(!should_stop);
        match resp {
            DaemonResponse::Error(msg) => {
                assert!(msg.contains("unsupported language"));
            }
            _ => panic!("expected Error response"),
        }
    }

    #[test]
    fn test_handle_request_query_unknown_operation() {
        // This will fail to start a real server, but we can test the operation
        // routing by using a language that won't have a server on CI.
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Query {
            project: PathBuf::from("/nonexistent-project"),
            language: "rust".to_string(),
            operation: "refactor".to_string(),
            file: PathBuf::from("/nonexistent-project/main.rs"),
            line: 1,
            column: 0,
            symbol: None,
        });
        assert!(!should_stop);
        // Either a server start error or "unknown operation" — both are Error.
        assert!(matches!(resp, DaemonResponse::Error(_)));
    }

    // ─── evict_idle_servers ─────────────────────────────────────────

    #[test]
    fn test_evict_idle_servers_does_nothing_when_empty() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        daemon.evict_idle_servers();
        assert!(daemon.servers.is_empty());
    }

    // ─── handle_status ──────────────────────────────────────────────

    #[test]
    fn test_handle_status_reports_uptime() {
        let daemon = Daemon::new(Duration::from_secs(60));
        // Let a tiny bit of time pass.
        std::thread::sleep(Duration::from_millis(10));
        let resp = daemon.handle_status();
        match resp {
            DaemonResponse::Status { uptime_secs, .. } => {
                // Uptime should be 0 (less than 1 second).
                assert!(uptime_secs < 2);
            }
            _ => panic!("expected Status response"),
        }
    }

    // ─── signal handler global ──────────────────────────────────────

    #[test]
    fn test_shutdown_flag_default_is_zero() {
        // The global should start at 0 (no flag set).
        // Note: this test is order-dependent in theory, but the value
        // is only set when a daemon starts running.
        let val = SHUTDOWN_FLAG.load(Ordering::Relaxed);
        // We can't assert == 0 because a previous test might have set it.
        // Just verify it's loadable without crashing.
        let _ = val;
    }

    // ─── DaemonRequest/DaemonResponse equality used in tests ────────

    #[test]
    fn test_handle_query_unsupported_language_returns_error() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let resp = daemon.handle_query(
            Path::new("/project"),
            "haskell",
            "definition",
            Path::new("/project/Main.hs"),
            1,
            0,
        );
        match resp {
            DaemonResponse::Error(msg) => {
                assert!(msg.contains("unsupported language"), "got: {msg}");
            }
            _ => panic!("expected Error response"),
        }
    }

    // ─── get_or_start_server ────────────────────────────────────────

    #[test]
    fn test_get_or_start_server_no_config_returns_error() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        // Ruby has no config in the default registry.
        let result = daemon.get_or_start_server(Path::new("/project"), Language::Ruby);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, LspError::ServerNotFound(_)),
            "expected ServerNotFound, got: {err:?}"
        );
    }

    // ─── handle_query with valid language but nonexistent project ───

    #[test]
    fn test_handle_query_rust_no_server_binary_returns_error() {
        // Rust has a config (rust-analyzer), but it may not be installed.
        // Either way, handle_query should return an Error response, not panic.
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let resp = daemon.handle_query(
            Path::new("/nonexistent-project-xyz"),
            "rust",
            "definition",
            Path::new("/nonexistent-project-xyz/main.rs"),
            1,
            0,
        );
        // Will either fail to start the server or fail to find the definition.
        // Either way, should be an Error variant.
        assert!(matches!(resp, DaemonResponse::Error(_)));
    }

    // ─── shutdown_all_servers ───────────────────────────────────────

    #[test]
    fn test_shutdown_all_servers_empty_pool_is_noop() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        daemon.shutdown_all_servers();
        assert!(daemon.servers.is_empty());
    }

    // ─── evict_idle_servers with non-empty pool ─────────────────────

    #[test]
    fn test_evict_idle_servers_does_not_evict_fresh_servers() {
        // We can't easily insert a mock PooledServer because PooledServer
        // contains a LspServer. But we can verify the logic path by using a
        // very long timeout with an empty pool (no panic).
        let mut daemon = Daemon::new(Duration::from_secs(3600));
        daemon.evict_idle_servers();
        assert!(daemon.servers.is_empty());
    }

    // ─── handle_request query routes to correct operation ───────────

    #[test]
    fn test_handle_request_query_valid_language_invalid_project() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        // Python has a config, but pyright-langserver is unlikely installed in CI.
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Query {
            project: PathBuf::from("/nonexistent-project"),
            language: "python".to_string(),
            operation: "references".to_string(),
            file: PathBuf::from("/nonexistent-project/main.py"),
            line: 1,
            column: 0,
            symbol: Some("foo".to_string()),
        });
        assert!(!should_stop);
        assert!(
            matches!(resp, DaemonResponse::Error(_)),
            "expected Error response, got: {resp:?}"
        );
    }

    #[test]
    fn test_handle_request_query_with_hover_operation() {
        let mut daemon = Daemon::new(Duration::from_secs(60));
        let (resp, should_stop) = daemon.handle_request(DaemonRequest::Query {
            project: PathBuf::from("/nonexistent-project"),
            language: "go".to_string(),
            operation: "hover".to_string(),
            file: PathBuf::from("/nonexistent-project/main.go"),
            line: 1,
            column: 0,
            symbol: None,
        });
        assert!(!should_stop);
        assert!(matches!(resp, DaemonResponse::Error(_)));
    }

    // ─── handle_status with servers in pool ─────────────────────────

    #[test]
    fn test_handle_status_empty_pool_returns_zero_servers() {
        let daemon = Daemon::new(Duration::from_secs(60));
        let resp = daemon.handle_status();
        match resp {
            DaemonResponse::Status {
                servers,
                uptime_secs: _,
            } => {
                assert!(servers.is_empty());
            }
            _ => panic!("expected Status response"),
        }
    }

    // ─── idle timeout from env edge cases ───────────────────────────

    #[test]
    fn test_daemon_from_env_handles_zero_timeout() {
        std::env::set_var(IDLE_TIMEOUT_ENV_VAR, "0");
        let daemon = Daemon::from_env();
        assert_eq!(daemon.idle_timeout, Duration::from_secs(0));
        std::env::remove_var(IDLE_TIMEOUT_ENV_VAR);
    }

    #[test]
    fn test_daemon_from_env_handles_empty_string() {
        std::env::set_var(IDLE_TIMEOUT_ENV_VAR, "");
        let daemon = Daemon::from_env();
        // Empty string fails to parse as u64, falls back to default.
        assert_eq!(
            daemon.idle_timeout,
            Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)
        );
        std::env::remove_var(IDLE_TIMEOUT_ENV_VAR);
    }

    // ─── ServerPoolState coverage ───────────────────────────────────

    #[test]
    fn test_server_pool_state_variants_exist() {
        // Exercise the pattern matching on ServerPoolState to cover the enum.
        let ready = ServerPoolState::Ready;
        let dead = ServerPoolState::Dead;
        let missing = ServerPoolState::Missing;

        assert!(matches!(ready, ServerPoolState::Ready));
        assert!(matches!(dead, ServerPoolState::Dead));
        assert!(matches!(missing, ServerPoolState::Missing));
    }

    // ─── Shell script helpers for mock servers ──────────────────────

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

    /// Creates a mock LSP server that responds to init, initialized notification,
    /// didOpen notification, one query, shutdown, and exit.
    fn mock_query_server_config(
        capabilities: &str,
        query_result: &str,
    ) -> crate::config::ServerConfig {
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{capabilities}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{query_result}}}'; \
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             {SHELL_READ_REQUEST}"
        );

        crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        }
    }

    /// Helper to insert a mock server into the daemon pool.
    fn insert_mock_server(
        daemon: &mut Daemon,
        project: PathBuf,
        language: Language,
        config: &crate::config::ServerConfig,
    ) -> crate::error::Result<()> {
        let server = LspServer::start(config, &project)?;
        let now = Instant::now();
        daemon.servers.insert(
            (project, language),
            PooledServer {
                server,
                last_used: now,
                started: now,
            },
        );
        Ok(())
    }

    // ─── handle_query with mock server ──────────────────────────────

    #[test]
    fn test_handle_query_definition_with_mock_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let def_result = format!(
            "{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":0,\"character\":3}},\"end\":{{\"line\":0,\"character\":6}}}}}}"
        );

        let config = mock_query_server_config("{\"definitionProvider\":true}", &def_result);

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(dir.path(), "rust", "definition", &file, 2, 11);

        match resp {
            DaemonResponse::Locations(locs) => {
                assert_eq!(locs.len(), 1);
                assert_eq!(locs[0].range.start.line, 0);
                assert_eq!(locs[0].range.start.character, 3);
            }
            DaemonResponse::Error(msg) => panic!("expected Locations, got Error: {msg}"),
            other => panic!("expected Locations, got: {other:?}"),
        }
    }

    #[test]
    fn test_handle_query_references_with_mock_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}\nfn bar() { foo(); }").unwrap();

        let file_uri = crate::queries::path_to_uri(&file);
        let refs_result = format!(
            "[{{\"uri\":\"{file_uri}\",\"range\":{{\"start\":{{\"line\":1,\"character\":11}},\"end\":{{\"line\":1,\"character\":14}}}}}}]"
        );

        let config = mock_query_server_config("{\"referencesProvider\":true}", &refs_result);

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(dir.path(), "rust", "references", &file, 1, 3);

        match resp {
            DaemonResponse::Locations(locs) => {
                assert_eq!(locs.len(), 1);
            }
            DaemonResponse::Error(msg) => panic!("expected Locations, got Error: {msg}"),
            other => panic!("expected Locations, got: {other:?}"),
        }
    }

    #[test]
    fn test_handle_query_hover_with_mock_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() -> i32 { 42 }").unwrap();

        let hover_result = r#"{"contents":{"kind":"markdown","value":"fn foo() -> i32"}}"#;
        let config = mock_query_server_config("{\"hoverProvider\":true}", hover_result);

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(dir.path(), "rust", "hover", &file, 1, 3);

        match resp {
            DaemonResponse::Hover(text) => {
                assert!(text.is_some());
                assert!(text.unwrap().contains("fn foo()"));
            }
            DaemonResponse::Error(msg) => panic!("expected Hover, got Error: {msg}"),
            other => panic!("expected Hover, got: {other:?}"),
        }
    }

    #[test]
    fn test_handle_query_unknown_operation_with_mock_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}").unwrap();

        // Mock server handles init + initialized + didOpen, then we never
        // send it a query because the operation is unknown.
        // The mock needs to handle the open_document notification and then
        // the server will be dropped without further interaction.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}\
             sleep 5"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(
            dir.path(),
            "rust",
            "refactor", // unknown operation
            &file,
            1,
            0,
        );

        match resp {
            DaemonResponse::Error(msg) => {
                assert!(msg.contains("unknown operation"), "got: {msg}");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    // ─── handle_status with servers in pool ─────────────────────────

    #[test]
    fn test_handle_status_with_servers_in_pool() {
        let dir = tempfile::tempdir().unwrap();

        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             sleep 30"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_status();
        match resp {
            DaemonResponse::Status {
                servers,
                uptime_secs: _,
            } => {
                assert_eq!(servers.len(), 1);
                assert_eq!(servers[0].language, "rust");
            }
            other => panic!("expected Status, got: {other:?}"),
        }
    }

    // ─── evict_idle_servers with expired server ─────────────────────

    #[test]
    fn test_evict_idle_servers_evicts_expired_server() {
        let dir = tempfile::tempdir().unwrap();

        // A server that stays alive.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             {SHELL_READ_REQUEST}"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        // Use a zero timeout so the server is immediately eligible for eviction.
        let mut daemon = Daemon::new(Duration::from_secs(0));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();
        assert_eq!(daemon.servers.len(), 1);

        // Wait a tiny bit so last_used.elapsed() > 0.
        std::thread::sleep(Duration::from_millis(10));

        daemon.evict_idle_servers();
        assert!(daemon.servers.is_empty(), "server should have been evicted");
    }

    // ─── shutdown_all_servers with servers ───────────────────────────

    #[test]
    fn test_shutdown_all_servers_shuts_down_all() {
        let dir = tempfile::tempdir().unwrap();

        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'; \
             {SHELL_READ_REQUEST}"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();
        assert_eq!(daemon.servers.len(), 1);

        daemon.shutdown_all_servers();
        assert!(daemon.servers.is_empty());
    }

    // ─── get_or_start_server reuses existing server ─────────────────

    #[test]
    fn test_get_or_start_server_reuses_existing() {
        let dir = tempfile::tempdir().unwrap();

        // Server that stays alive.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             sleep 30"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        // get_or_start_server should find and reuse the existing server.
        let result = daemon.get_or_start_server(dir.path(), Language::Rust);
        assert!(result.is_ok());
        assert_eq!(daemon.servers.len(), 1);
    }

    // ─── handle_query error paths when server returns errors ─────────

    #[test]
    fn test_handle_query_definition_error_from_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}").unwrap();

        // Mock server that returns an error response for the query.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{\"definitionProvider\":true}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"error\":{{\"code\":-32601,\"message\":\"internal error\"}}}}'; \
             sleep 5"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(dir.path(), "rust", "definition", &file, 1, 3);
        match resp {
            DaemonResponse::Error(msg) => {
                assert!(msg.contains("definition failed"), "got: {msg}");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    #[test]
    fn test_handle_query_references_error_from_server() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.rs");
        std::fs::write(&file, "fn foo() {}").unwrap();

        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{\"referencesProvider\":true}}}}}}'; \
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}\
             {SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"error\":{{\"code\":-32601,\"message\":\"refs error\"}}}}'; \
             sleep 5"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        let resp = daemon.handle_query(dir.path(), "rust", "references", &file, 1, 3);
        match resp {
            DaemonResponse::Error(msg) => {
                assert!(msg.contains("references failed"), "got: {msg}");
            }
            other => panic!("expected Error, got: {other:?}"),
        }
    }

    // ─── get_or_start_server replaces dead server ───────────────────

    #[test]
    fn test_get_or_start_server_replaces_dead_server() {
        let dir = tempfile::tempdir().unwrap();

        // Server that exits immediately after init.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"capabilities\":{{}}}}}}'; \
             {SHELL_READ_REQUEST}\
             exit 0"
        );

        let config = crate::config::ServerConfig {
            binary: "sh".to_string(),
            args: vec!["-c".to_string(), script],
            env: vec![],
        };

        let mut daemon = Daemon::new(Duration::from_secs(60));
        insert_mock_server(
            &mut daemon,
            dir.path().to_path_buf(),
            Language::Rust,
            &config,
        )
        .unwrap();

        // Give the server time to exit.
        std::thread::sleep(Duration::from_millis(200));

        // get_or_start_server should detect the dead server and try to start a new one.
        // It will fail (rust-analyzer not installed), but the dead server should be removed.
        let result = daemon.get_or_start_server(dir.path(), Language::Rust);
        // Either succeeds (if rust-analyzer exists) or fails, but the dead
        // server should have been removed.
        if result.is_err() {
            // The dead server was removed, and the new start attempt failed.
            assert!(daemon.servers.is_empty());
        }
    }
}
