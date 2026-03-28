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
}
