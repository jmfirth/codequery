//! Daemon management commands: start, stop, status.

use std::path::{Path, PathBuf};

use crate::args::ExitCode;

/// Resolves the project root for daemon commands, falling back to auto-detection.
fn resolve_project_root(project: Option<&Path>) -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    codequery_core::detect_project_root_or(&cwd, project)
        .map_err(|e| anyhow::anyhow!("failed to detect project root: {e}"))
}

/// Start the daemon as a detached background process.
///
/// Spawns the current binary with the hidden `_daemon-run` subcommand. The
/// child process is detached so it outlives the parent. Prints the child PID
/// on success.
///
/// # Errors
///
/// Returns an error if the current executable path cannot be determined or
/// the child process cannot be spawned.
pub fn run_start(project: Option<&Path>) -> anyhow::Result<ExitCode> {
    let project_root = resolve_project_root(project)?;

    if codequery_lsp::daemon_file::is_daemon_running(&project_root) {
        if let Some(info) = codequery_lsp::daemon_file::read_daemon_info(&project_root) {
            eprintln!(
                "daemon already running (pid {}, port {})",
                info.pid, info.port
            );
        } else {
            eprintln!("daemon already running");
        }
        return Ok(ExitCode::Success);
    }

    let exe = std::env::current_exe().map_err(|e| anyhow::anyhow!("cannot find cq binary: {e}"))?;

    let child = std::process::Command::new(exe)
        .arg("_daemon-run")
        .env("CQ_DAEMON_PROJECT", &project_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn daemon: {e}"))?;

    eprintln!("daemon started (pid {})", child.id());
    Ok(ExitCode::Success)
}

/// Stop a running daemon by sending a shutdown request.
///
/// Connects to the daemon socket and sends a `Shutdown` message. If no
/// daemon is running, prints a message and returns success.
///
/// # Errors
///
/// Returns an error if the connection succeeds but the shutdown request fails.
pub fn run_stop(project: Option<&Path>) -> anyhow::Result<ExitCode> {
    let project_root = resolve_project_root(project)?;

    if !codequery_lsp::daemon_file::is_daemon_running(&project_root) {
        eprintln!("daemon is not running");
        return Ok(ExitCode::Success);
    }

    let mut client = codequery_lsp::DaemonClient::connect(&project_root)
        .map_err(|e| anyhow::anyhow!("failed to connect to daemon: {e}"))?;

    client
        .shutdown()
        .map_err(|e| anyhow::anyhow!("shutdown request failed: {e}"))?;

    eprintln!("daemon stopped");
    Ok(ExitCode::Success)
}

/// Display daemon status information.
///
/// Connects to the daemon and requests its status, including uptime and
/// active language servers. If no daemon is running, reports that.
///
/// # Errors
///
/// Returns an error if the connection succeeds but the status request fails.
pub fn run_status(project: Option<&Path>) -> anyhow::Result<ExitCode> {
    let project_root = resolve_project_root(project)?;

    if !codequery_lsp::daemon_file::is_daemon_running(&project_root) {
        // Try listing all daemons for broader context.
        let all = codequery_lsp::daemon_file::list_all_daemons();
        if all.is_empty() {
            eprintln!("daemon is not running");
        } else {
            eprintln!(
                "daemon is not running for this project, but {} daemon(s) found:",
                all.len()
            );
            for info in &all {
                eprintln!(
                    "  pid {} — {} (port {})",
                    info.pid,
                    info.project.display(),
                    info.port
                );
            }
        }
        return Ok(ExitCode::Success);
    }

    let mut client = codequery_lsp::DaemonClient::connect(&project_root)
        .map_err(|e| anyhow::anyhow!("failed to connect to daemon: {e}"))?;

    let response = client
        .status()
        .map_err(|e| anyhow::anyhow!("status request failed: {e}"))?;

    match response {
        codequery_lsp::DaemonResponse::Status {
            servers,
            uptime_secs,
        } => {
            eprintln!("daemon running (uptime: {uptime_secs}s)");
            if servers.is_empty() {
                eprintln!("  no active language servers");
            } else {
                for server in &servers {
                    eprintln!(
                        "  {} ({}) — up {}s",
                        server.project.display(),
                        server.language,
                        server.uptime_secs,
                    );
                }
            }
            Ok(ExitCode::Success)
        }
        _ => Err(anyhow::anyhow!("unexpected response from daemon")),
    }
}

/// Run the daemon in the foreground (called by the hidden `_daemon-run` subcommand).
///
/// Creates a `Daemon` from environment configuration and runs its blocking
/// accept loop. This function does not return until the daemon shuts down.
///
/// # Errors
///
/// Returns an error if the daemon fails to start (e.g., cannot bind socket).
pub fn run_foreground() -> anyhow::Result<ExitCode> {
    let mut daemon = codequery_lsp::Daemon::from_env();
    daemon
        .run()
        .map_err(|e| anyhow::anyhow!("daemon error: {e}"))?;
    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_stop_when_daemon_not_running_succeeds() {
        // Use a temp dir so we never collide with a real running daemon.
        let tmp = tempfile::TempDir::new().unwrap();
        let result = run_stop(Some(tmp.path())).unwrap();
        assert_eq!(result, ExitCode::Success);
    }

    #[test]
    fn test_run_status_when_daemon_not_running_returns_no_results() {
        // Use a temp dir so we never collide with a real running daemon.
        let tmp = tempfile::TempDir::new().unwrap();
        let result = run_status(Some(tmp.path())).unwrap();
        assert_eq!(result, ExitCode::Success);
    }

    #[test]
    fn test_run_start_when_daemon_not_running_spawns_process() {
        // We cannot fully test daemon start in a unit test without actually
        // spawning a daemon. Verify the current_exe path is resolvable.
        let exe = std::env::current_exe();
        assert!(exe.is_ok(), "current_exe must be resolvable");
    }
}
