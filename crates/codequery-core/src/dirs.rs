//! XDG-compliant directory resolution for cq.
//!
//! Provides centralized path resolution for all cq storage locations.
//! Follows XDG Base Directory Specification with environment variable overrides.
//!
//! - **Data** (`~/.local/share/cq/`) — durable user data (installed languages)
//! - **Cache** (`~/.cache/cq/`) — deletable without data loss (scan cache, CWASM, registry)
//! - **Runtime** (platform-specific) — ephemeral (daemon PID, socket)

#![allow(clippy::missing_panics_doc)]

use std::path::PathBuf;

/// Returns the cq data directory for durable user data.
///
/// Resolution order: `$CQ_DATA_DIR` > `$XDG_DATA_HOME/cq/` > `~/.local/share/cq/`
///
/// Used for: installed language packages, user-provided grammars.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CQ_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return Some(PathBuf::from(xdg).join("cq"));
    }
    #[allow(deprecated)]
    std::env::home_dir().map(|h| h.join(".local").join("share").join("cq"))
}

/// Returns the cq cache directory for deletable cached data.
///
/// Resolution order: `$CQ_CACHE_DIR` > `$XDG_CACHE_HOME/cq/` > `~/.cache/cq/`
///
/// Used for: scan cache, CWASM compiled grammars, registry cache.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CQ_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return Some(PathBuf::from(xdg).join("cq"));
    }
    #[allow(deprecated)]
    std::env::home_dir().map(|h| h.join(".cache").join("cq"))
}

/// Returns the directory where installed language packages are stored.
///
/// Default: `~/.local/share/cq/languages/`
#[must_use]
pub fn languages_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("languages"))
}

/// Returns the directory for cached CWASM (ahead-of-time compiled WASM grammars).
///
/// Default: `~/.cache/cq/cwasm/`
#[must_use]
pub fn cwasm_dir() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("cwasm"))
}

/// Returns the path to the cached language registry.
///
/// Default: `~/.cache/cq/registry.json`
#[must_use]
pub fn registry_cache_path() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("registry.json"))
}

/// Returns the cq runtime directory for ephemeral state.
///
/// Resolution order: `$CQ_RUNTIME_DIR` > `$XDG_RUNTIME_DIR/cq/` > `/tmp/cq-$UID/`
///
/// Used for: daemon PID file, Unix socket.
#[must_use]
pub fn runtime_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CQ_RUNTIME_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(xdg).join("cq"));
    }
    // Fallback: temp dir + cq/
    Some(std::env::temp_dir().join("cq"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_respects_cq_data_dir_env() {
        std::env::set_var("CQ_DATA_DIR", "/custom/data");
        assert_eq!(data_dir(), Some(PathBuf::from("/custom/data")));
        std::env::remove_var("CQ_DATA_DIR");
    }

    #[test]
    fn cache_dir_respects_cq_cache_dir_env() {
        std::env::set_var("CQ_CACHE_DIR", "/custom/cache");
        assert_eq!(cache_dir(), Some(PathBuf::from("/custom/cache")));
        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn languages_dir_is_under_data_dir() {
        std::env::set_var("CQ_DATA_DIR", "/custom/data");
        assert_eq!(
            languages_dir(),
            Some(PathBuf::from("/custom/data/languages"))
        );
        std::env::remove_var("CQ_DATA_DIR");
    }

    #[test]
    fn cwasm_dir_is_under_cache_dir() {
        std::env::set_var("CQ_CACHE_DIR", "/custom/cache");
        assert_eq!(cwasm_dir(), Some(PathBuf::from("/custom/cache/cwasm")));
        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn registry_cache_path_is_under_cache_dir() {
        std::env::set_var("CQ_CACHE_DIR", "/custom/cache");
        assert_eq!(
            registry_cache_path(),
            Some(PathBuf::from("/custom/cache/registry.json"))
        );
        std::env::remove_var("CQ_CACHE_DIR");
    }

    #[test]
    fn runtime_dir_respects_cq_runtime_dir_env() {
        std::env::set_var("CQ_RUNTIME_DIR", "/custom/runtime");
        assert_eq!(runtime_dir(), Some(PathBuf::from("/custom/runtime")));
        std::env::remove_var("CQ_RUNTIME_DIR");
    }

    #[test]
    fn data_dir_falls_back_to_xdg() {
        std::env::remove_var("CQ_DATA_DIR");
        std::env::set_var("XDG_DATA_HOME", "/xdg/data");
        assert_eq!(data_dir(), Some(PathBuf::from("/xdg/data/cq")));
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn cache_dir_falls_back_to_xdg() {
        std::env::remove_var("CQ_CACHE_DIR");
        std::env::set_var("XDG_CACHE_HOME", "/xdg/cache");
        assert_eq!(cache_dir(), Some(PathBuf::from("/xdg/cache/cq")));
        std::env::remove_var("XDG_CACHE_HOME");
    }
}
