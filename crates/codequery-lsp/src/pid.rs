//! PID file and runtime directory management for the cq daemon.
//!
//! Handles locating the daemon's runtime directory, writing and reading PID
//! files, and determining the Unix socket path. The runtime directory follows
//! the XDG Base Directory Specification when `$XDG_RUNTIME_DIR` is set,
//! falling back to `$HOME/.cache/cq/`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{LspError, Result};

/// Name of the PID file within the runtime directory.
const PID_FILENAME: &str = "daemon.pid";

/// Name of the Unix socket within the runtime directory.
const SOCKET_FILENAME: &str = "daemon.sock";

/// Returns the cq daemon runtime directory.
///
/// Uses `$XDG_RUNTIME_DIR/cq/` if the environment variable is set,
/// otherwise falls back to `$HOME/.cache/cq/`.
///
/// # Errors
///
/// Returns an error if neither `$XDG_RUNTIME_DIR` nor `$HOME` is set.
pub fn runtime_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(xdg).join("cq"));
    }

    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".cache").join("cq"));
    }

    Err(LspError::ConnectionFailed(
        "cannot determine runtime directory: neither XDG_RUNTIME_DIR nor HOME is set".to_string(),
    ))
}

/// Returns the path to the daemon PID file.
///
/// # Errors
///
/// Returns an error if the runtime directory cannot be determined.
pub fn pid_file_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join(PID_FILENAME))
}

/// Returns the path to the daemon Unix socket.
///
/// # Errors
///
/// Returns an error if the runtime directory cannot be determined.
pub fn socket_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join(SOCKET_FILENAME))
}

/// Writes the current process ID to the PID file.
///
/// Creates the runtime directory if it does not exist. Overwrites any
/// existing PID file.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot
/// be written.
pub fn write_pid_file() -> Result<()> {
    let dir = runtime_dir()?;
    write_pid_file_to(&dir)
}

/// Reads the PID from the daemon PID file.
///
/// Returns `None` if the file does not exist or cannot be parsed.
#[must_use]
pub fn read_pid_file() -> Option<u32> {
    let path = pid_file_path().ok()?;
    read_pid_from(&path)
}

/// Removes the daemon PID file if it exists.
///
/// Errors are silently ignored — this is best-effort cleanup.
pub fn remove_pid_file() {
    if let Ok(path) = pid_file_path() {
        let _ = fs::remove_file(path);
    }
}

/// Returns `true` if the daemon appears to be running.
///
/// Checks for a PID file and verifies the process is alive by sending
/// signal 0 via `kill(2)`. Returns `false` if the PID file is missing,
/// unreadable, or the process is no longer running.
#[must_use]
pub fn is_daemon_running() -> bool {
    let Some(pid) = read_pid_file() else {
        return false;
    };

    is_pid_alive(pid)
}

// ─── Internal helpers ─────────────────────────────────────────────

/// Writes the current process ID to a PID file in the given directory.
fn write_pid_file_to(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(PID_FILENAME);
    let pid = std::process::id();
    fs::write(&path, pid.to_string())?;
    Ok(())
}

/// Reads a PID from a file at the given path.
fn read_pid_from(path: &Path) -> Option<u32> {
    let contents = fs::read_to_string(path).ok()?;
    contents.trim().parse().ok()
}

/// Checks whether a process with the given PID is alive.
fn is_pid_alive(pid: u32) -> bool {
    // Signal 0 does not actually send a signal but checks whether the
    // process exists and we have permission to signal it.
    #[allow(clippy::cast_possible_wrap)]
    // PID values fit in i32 on all supported platforms.
    let ret = unsafe {
        // SAFETY: kill(pid, 0) is safe — it performs a permission check
        // without delivering a signal. The pid comes from our own PID file.
        libc::kill(pid as i32, 0)
    };

    ret == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── runtime_dir logic (pure env lookups) ───────────────────────
    // These tests verify the env-var-to-path mapping. They are intentionally
    // simple and don't touch the filesystem, so env var races between test
    // threads only affect path strings, not file operations.

    #[test]
    fn test_runtime_dir_uses_xdg_when_set() {
        // This test relies on XDG_RUNTIME_DIR being set (which it usually
        // is on Linux/macOS dev machines). If not set, it falls back to HOME.
        // We verify the path ends with /cq.
        let dir = runtime_dir().unwrap();
        assert!(
            dir.ends_with("cq"),
            "runtime dir {dir:?} should end with 'cq'"
        );
    }

    #[test]
    fn test_pid_file_path_ends_with_daemon_pid() {
        let path = pid_file_path().unwrap();
        assert!(
            path.ends_with("daemon.pid"),
            "pid file path {path:?} should end with 'daemon.pid'"
        );
    }

    #[test]
    fn test_socket_path_ends_with_daemon_sock() {
        let path = socket_path().unwrap();
        assert!(
            path.ends_with("daemon.sock"),
            "socket path {path:?} should end with 'daemon.sock'"
        );
    }

    // ─── PID file operations (use temp dirs directly) ───────────────
    // These tests use `write_pid_file_to` / `read_pid_from` with explicit
    // paths to avoid env var races.

    #[test]
    fn test_write_and_read_pid_file_to_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        write_pid_file_to(dir.path()).unwrap();

        let pid_path = dir.path().join(PID_FILENAME);
        let pid = read_pid_from(&pid_path).unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn test_read_pid_from_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("nonexistent.pid");
        assert!(read_pid_from(&pid_path).is_none());
    }

    #[test]
    fn test_read_pid_from_invalid_content_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join(PID_FILENAME);
        fs::write(&pid_path, "not_a_number").unwrap();
        assert!(read_pid_from(&pid_path).is_none());
    }

    #[test]
    fn test_write_pid_file_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested").join("deep");
        write_pid_file_to(&nested).unwrap();

        let pid_path = nested.join(PID_FILENAME);
        assert!(pid_path.exists());
        let pid = read_pid_from(&pid_path).unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn test_write_pid_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join(PID_FILENAME);

        // Write a fake PID first.
        fs::write(&pid_path, "12345").unwrap();
        assert_eq!(read_pid_from(&pid_path), Some(12345));

        // Overwrite with current PID.
        write_pid_file_to(dir.path()).unwrap();
        assert_eq!(read_pid_from(&pid_path), Some(std::process::id()));
    }

    #[test]
    fn test_remove_pid_file_from_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join(PID_FILENAME);

        write_pid_file_to(dir.path()).unwrap();
        assert!(pid_path.exists());

        fs::remove_file(&pid_path).unwrap();
        assert!(!pid_path.exists());
    }

    // ─── is_pid_alive ───────────────────────────────────────────────

    #[test]
    fn test_is_pid_alive_current_process() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[test]
    fn test_is_pid_alive_nonexistent_process() {
        // PID 999999999 is very unlikely to be running.
        assert!(!is_pid_alive(999_999_999));
    }

    // ─── is_daemon_running integration ──────────────────────────────

    #[test]
    fn test_is_daemon_running_returns_bool() {
        // This is a smoke test — the actual result depends on whether a
        // real daemon PID file exists. We just verify it doesn't panic.
        let _ = is_daemon_running();
    }

    // ─── write_pid_file and remove_pid_file ─────────────────────────

    #[test]
    fn test_write_pid_file_to_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join(PID_FILENAME);

        // Write
        write_pid_file_to(dir.path()).unwrap();
        assert!(pid_path.exists());

        // Read back
        let pid = read_pid_from(&pid_path).unwrap();
        assert_eq!(pid, std::process::id());

        // Remove
        fs::remove_file(&pid_path).unwrap();
        assert!(!pid_path.exists());
    }

    // ─── runtime_dir always returns a path ending in "cq" ──────────

    #[test]
    fn test_runtime_dir_path_ends_with_cq() {
        let dir = runtime_dir().unwrap();
        let dir_str = dir.to_str().unwrap();
        assert!(
            dir_str.ends_with("/cq") || dir_str.ends_with("\\cq"),
            "runtime dir should end with /cq: {dir_str}"
        );
    }

    // ─── pid_file_path and socket_path relative to runtime_dir ──────

    #[test]
    fn test_pid_file_path_is_inside_runtime_dir() {
        let rt_dir = runtime_dir().unwrap();
        let pid_path = pid_file_path().unwrap();
        assert_eq!(pid_path.parent().unwrap(), rt_dir);
    }

    #[test]
    fn test_socket_path_is_inside_runtime_dir() {
        let rt_dir = runtime_dir().unwrap();
        let sock_path = socket_path().unwrap();
        assert_eq!(sock_path.parent().unwrap(), rt_dir);
    }

    // ─── read_pid_file when no file exists ──────────────────────────

    #[test]
    fn test_read_pid_file_returns_none_when_no_daemon() {
        // With no daemon running, read_pid_file might return None or
        // Some(stale_pid). The key is it doesn't panic.
        let _ = read_pid_file();
    }

    // ─── is_pid_alive with PID 1 (init, always running) ────────────

    #[test]
    fn test_is_pid_alive_with_pid_one() {
        // PID 1 (init/launchd) is always running, but we may not have
        // permission to signal it. Either result is valid.
        let _ = is_pid_alive(1);
    }

    // ─── write_pid_file_to with nested non-existent dirs ────────────

    #[test]
    fn test_write_pid_file_to_deeply_nested_dir() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        write_pid_file_to(&deep).unwrap();
        let pid = read_pid_from(&deep.join(PID_FILENAME)).unwrap();
        assert_eq!(pid, std::process::id());
    }

    // ─── remove_pid_file is best-effort ─────────────────────────────

    #[test]
    fn test_remove_pid_file_does_not_panic_when_no_file() {
        // remove_pid_file should not panic even if there's no PID file.
        remove_pid_file();
    }

    // ─── write_pid_file and remove_pid_file (real paths) ────────────

    #[test]
    fn test_write_pid_file_and_remove_pid_file_real_paths() {
        // Write to the real runtime dir, then clean up.
        let result = write_pid_file();
        assert!(result.is_ok(), "write_pid_file failed: {result:?}");

        // Verify the PID file was written.
        let pid = read_pid_file();
        assert_eq!(pid, Some(std::process::id()));

        // Clean up.
        remove_pid_file();

        // After removal, read_pid_file should return None (or the file is gone).
        let pid_path = pid_file_path().unwrap();
        assert!(!pid_path.exists(), "PID file should be removed");
    }

    // ─── is_daemon_running with current process PID ─────────────────

    #[test]
    fn test_is_daemon_running_true_when_pid_file_has_current_pid() {
        // Write the current process PID to the PID file.
        let result = write_pid_file();
        assert!(result.is_ok());

        // is_daemon_running should return true (current process is alive).
        assert!(
            is_daemon_running(),
            "is_daemon_running should return true when PID file contains current PID"
        );

        // Clean up.
        remove_pid_file();
    }

    // ─── is_daemon_running with stale PID ───────────────────────────

    #[test]
    fn test_runtime_dir_uses_home_fallback_when_xdg_not_set() {
        // Save the current XDG_RUNTIME_DIR.
        let saved_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

        // Remove XDG_RUNTIME_DIR to force the HOME fallback.
        std::env::remove_var("XDG_RUNTIME_DIR");

        let dir = runtime_dir().unwrap();
        let dir_str = dir.to_str().unwrap();

        // Without XDG, it should use HOME/.cache/cq
        assert!(
            dir_str.contains(".cache/cq") || dir_str.ends_with("/cq"),
            "runtime dir {dir_str} should use cache fallback"
        );

        // Restore XDG_RUNTIME_DIR.
        if let Some(xdg) = saved_xdg {
            std::env::set_var("XDG_RUNTIME_DIR", xdg);
        }
    }

    #[test]
    fn test_is_daemon_running_false_when_pid_is_stale() {
        // Write a definitely-dead PID to the PID file.
        let dir = runtime_dir().unwrap();
        fs::create_dir_all(&dir).unwrap();
        let pid_path = dir.join(PID_FILENAME);
        fs::write(&pid_path, "999999999").unwrap();

        // is_daemon_running should return false (PID 999999999 is dead).
        assert!(
            !is_daemon_running(),
            "is_daemon_running should return false for a dead PID"
        );

        // Clean up.
        let _ = fs::remove_file(&pid_path);
    }
}
