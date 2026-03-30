//! Upgrade check command: checks GitHub releases for a newer version of cq.
//!
//! Queries the GitHub releases API, compares the latest tag to the current
//! binary version, and prints instructions if an upgrade is available.

use crate::args::ExitCode;

/// The GitHub repository used for release checks.
const GITHUB_REPO: &str = "jmfirth/codequery";

/// `cq upgrade` — check for a newer version and print instructions.
///
/// # Errors
///
/// Returns `Result` for consistency with all other command handlers; network
/// failures are handled gracefully and reported as exit codes, not errors.
#[allow(clippy::unnecessary_wraps)]
// All command handlers share the same Result<ExitCode> signature for uniform dispatch
pub fn run() -> anyhow::Result<ExitCode> {
    let current = env!("CARGO_PKG_VERSION");
    eprintln!("Current version: v{current}");
    eprintln!("Checking for updates...");

    // Shell out to curl for the GitHub releases API
    // This avoids adding an HTTP client dependency to the binary
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest"),
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let body = String::from_utf8_lossy(&out.stdout);
            // Parse just the tag_name field without pulling in a full JSON parser
            // (serde_json is already a dependency, so we can use it)
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(tag) = json.get("tag_name").and_then(|v| v.as_str()) {
                    let latest = tag.strip_prefix('v').unwrap_or(tag);
                    if latest == current {
                        eprintln!("Already up to date (v{current})");
                        return Ok(ExitCode::Success);
                    }
                    eprintln!("New version available: v{latest} (current: v{current})");
                    eprintln!();
                    eprintln!("To upgrade:");
                    eprintln!(
                        "  cargo install --git https://github.com/{GITHUB_REPO} codequery-cli"
                    );
                    eprintln!();
                    eprintln!("Or download from:");
                    eprintln!("  https://github.com/{GITHUB_REPO}/releases/tag/v{latest}");
                    return Ok(ExitCode::Success);
                }
            }
            // Might be a 404 or rate-limited response
            eprintln!("No releases found for {GITHUB_REPO}");
            eprintln!("Check https://github.com/{GITHUB_REPO}/releases manually");
            Ok(ExitCode::NoResults)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("Failed to check for updates: {stderr}");
            eprintln!("Check https://github.com/{GITHUB_REPO}/releases manually");
            Ok(ExitCode::NoResults)
        }
        Err(e) => {
            eprintln!("Could not run curl: {e}");
            eprintln!("Check https://github.com/{GITHUB_REPO}/releases manually");
            Ok(ExitCode::NoResults)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_current_version_is_set() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(!version.is_empty());
    }
}
