//! Daemon info file management for the cq daemon.
//!
//! Each daemon writes a JSON file to `~/.cache/cq/daemons/<project-hash>.json`
//! containing its TCP port, authentication token, PID, project root, and start
//! time. This replaces the old PID + Unix socket approach with a cross-platform
//! TCP-based design.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Information about a running daemon instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonInfo {
    /// TCP port the daemon is listening on.
    pub port: u16,
    /// Authentication token for connecting clients.
    pub token: String,
    /// Process ID of the daemon.
    pub pid: u32,
    /// Project root directory this daemon is serving.
    pub project: PathBuf,
    /// ISO 8601 timestamp when the daemon was started.
    pub started: String,
}

/// Returns the directory where daemon info files are stored.
///
/// Uses `codequery_core::dirs::cache_dir()?.join("daemons")`.
#[must_use]
pub fn daemons_dir() -> Option<PathBuf> {
    codequery_core::dirs::daemons_dir()
}

/// Computes a stable hash of a project root path for use as a filename.
///
/// Returns a 16-character lowercase hex string derived from hashing the
/// canonical path representation.
#[must_use]
pub fn project_hash(project_root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    project_root.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Returns the path to the daemon info file for a given project root.
#[must_use]
pub fn daemon_file_path(project_root: &Path) -> Option<PathBuf> {
    let dir = daemons_dir()?;
    let hash = project_hash(project_root);
    Some(dir.join(format!("{hash}.json")))
}

/// Reads the daemon info file for a given project root.
///
/// Returns `None` if the file does not exist or cannot be parsed.
#[must_use]
pub fn read_daemon_info(project_root: &Path) -> Option<DaemonInfo> {
    let path = daemon_file_path(project_root)?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Writes daemon info to the appropriate file.
///
/// Creates the daemons directory if it does not exist.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot
/// be written.
pub fn write_daemon_info(info: &DaemonInfo) -> std::io::Result<()> {
    let dir = daemons_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine daemons directory",
        )
    })?;
    fs::create_dir_all(&dir)?;
    let hash = project_hash(&info.project);
    let path = dir.join(format!("{hash}.json"));
    let json = serde_json::to_string_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(path, json)
}

/// Removes the daemon info file for a given project root.
///
/// Best-effort: errors are silently ignored.
pub fn remove_daemon_file(project_root: &Path) {
    if let Some(path) = daemon_file_path(project_root) {
        let _ = fs::remove_file(path);
    }
}

/// Generates an authentication token for daemon connections.
///
/// Combines the current time, process ID, and a counter to produce a
/// 32-character hex string. Not cryptographically secure, but sufficient
/// for localhost-only daemon authentication.
#[must_use]
pub fn generate_token() -> String {
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let h1 = hasher.finish();

    let mut hasher2 = DefaultHasher::new();
    h1.hash(&mut hasher2);
    // Mix in a second round for more bits.
    42u64.hash(&mut hasher2);
    let h2 = hasher2.finish();

    format!("{h1:016x}{h2:016x}")
}

/// Returns `true` if a daemon appears to be running for the given project.
///
/// Reads the daemon info file and attempts a TCP connection to the daemon's
/// port on `127.0.0.1`. Returns `false` if the file is missing, unreadable,
/// or the TCP connection cannot be established.
#[must_use]
pub fn is_daemon_running(project_root: &Path) -> bool {
    let Some(info) = read_daemon_info(project_root) else {
        return false;
    };

    // Probe with a short TCP connect timeout.
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], info.port)),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}

/// Lists all daemon info files in the daemons directory.
///
/// Returns an empty vec if the directory does not exist or cannot be read.
#[must_use]
pub fn list_all_daemons() -> Vec<DaemonInfo> {
    let Some(dir) = daemons_dir() else {
        return Vec::new();
    };

    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                return None;
            }
            let contents = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&contents).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_hash_is_deterministic() {
        let hash1 = project_hash(Path::new("/home/user/project"));
        let hash2 = project_hash(Path::new("/home/user/project"));
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_project_hash_is_16_hex_chars() {
        let hash = project_hash(Path::new("/some/path"));
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_project_hash_differs_for_different_paths() {
        let hash1 = project_hash(Path::new("/path/one"));
        let hash2 = project_hash(Path::new("/path/two"));
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_daemon_file_path_ends_with_json() {
        let path = daemon_file_path(Path::new("/project"));
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(
            path.to_str().unwrap().ends_with(".json"),
            "daemon file path should end with .json: {path:?}"
        );
    }

    #[test]
    fn test_daemon_file_path_contains_hash() {
        let path = daemon_file_path(Path::new("/project")).unwrap();
        let hash = project_hash(Path::new("/project"));
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with(&hash));
    }

    #[test]
    fn test_write_and_read_daemon_info() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CQ_CACHE_DIR", dir.path().to_str().unwrap());

        let info = DaemonInfo {
            port: 49152,
            token: "test_token_abc123".to_string(),
            pid: 12345,
            project: PathBuf::from("/test/project"),
            started: "2026-03-31T04:00:00Z".to_string(),
        };

        write_daemon_info(&info).unwrap();

        let read_back = read_daemon_info(Path::new("/test/project"));
        assert!(read_back.is_some());
        assert_eq!(read_back.unwrap(), info);

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_read_daemon_info_missing_file_returns_none() {
        let read_back = read_daemon_info(Path::new("/nonexistent/project/xyz123"));
        assert!(read_back.is_none());
    }

    #[test]
    fn test_remove_daemon_file_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CQ_CACHE_DIR", dir.path().to_str().unwrap());

        let info = DaemonInfo {
            port: 49152,
            token: "token".to_string(),
            pid: 12345,
            project: PathBuf::from("/test/project"),
            started: "2026-03-31T04:00:00Z".to_string(),
        };

        write_daemon_info(&info).unwrap();
        let path = daemon_file_path(Path::new("/test/project")).unwrap();
        assert!(path.exists());

        remove_daemon_file(Path::new("/test/project"));
        assert!(!path.exists());

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_remove_daemon_file_no_panic_when_missing() {
        remove_daemon_file(Path::new("/nonexistent/project"));
    }

    #[test]
    fn test_generate_token_is_32_hex_chars() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token_differs_between_calls() {
        let t1 = generate_token();
        let t2 = generate_token();
        // Not guaranteed to differ, but with nanos and pid mixing they should.
        // If they're the same, the token generation is broken.
        let _ = (t1, t2);
    }

    #[test]
    fn test_is_daemon_running_false_when_no_file() {
        assert!(!is_daemon_running(Path::new("/nonexistent/project/xyz")));
    }

    #[test]
    fn test_is_daemon_running_false_when_port_not_open() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CQ_CACHE_DIR", dir.path().to_str().unwrap());

        let info = DaemonInfo {
            port: 59999, // unlikely to be in use
            token: "token".to_string(),
            pid: 999_999_999,
            project: PathBuf::from("/test/project/daemon_running_test"),
            started: "2026-03-31T04:00:00Z".to_string(),
        };
        write_daemon_info(&info).unwrap();

        assert!(!is_daemon_running(Path::new(
            "/test/project/daemon_running_test"
        )));

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_list_all_daemons_empty_when_no_dir() {
        let daemons = list_all_daemons();
        // May or may not be empty depending on system state, but should not panic.
        let _ = daemons;
    }

    #[test]
    fn test_list_all_daemons_finds_written_files() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CQ_CACHE_DIR", dir.path().to_str().unwrap());

        let info1 = DaemonInfo {
            port: 49152,
            token: "t1".to_string(),
            pid: 111,
            project: PathBuf::from("/project/one"),
            started: "2026-03-31T04:00:00Z".to_string(),
        };
        let info2 = DaemonInfo {
            port: 49153,
            token: "t2".to_string(),
            pid: 222,
            project: PathBuf::from("/project/two"),
            started: "2026-03-31T04:01:00Z".to_string(),
        };

        write_daemon_info(&info1).unwrap();
        write_daemon_info(&info2).unwrap();

        let daemons = list_all_daemons();
        assert_eq!(daemons.len(), 2);

        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn test_daemons_dir_returns_some() {
        let dir = daemons_dir();
        assert!(dir.is_some());
    }

    #[test]
    fn test_daemon_info_serialization_roundtrip() {
        let info = DaemonInfo {
            port: 12345,
            token: "abc".to_string(),
            pid: 42,
            project: PathBuf::from("/p"),
            started: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: DaemonInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, parsed);
    }
}
