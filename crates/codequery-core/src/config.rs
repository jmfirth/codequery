//! Project-level configuration loaded from `.cq.toml`.
//!
//! Provides optional per-project settings including exclude patterns,
//! language extension overrides, and cache preferences. Configuration is
//! entirely optional — `None` is returned when no `.cq.toml` exists.

use std::collections::HashMap;
use std::path::Path;

use crate::error::{CoreError, Result};

/// Project-level configuration parsed from `.cq.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectConfig {
    /// Additional glob patterns to exclude from file discovery.
    pub exclude: Vec<String>,
    /// Extension-to-language-name overrides (e.g., `".jsx" => "javascript"`).
    pub language_overrides: HashMap<String, String>,
    /// Default cache setting for the project.
    pub cache_enabled: Option<bool>,
    /// LSP server configuration overrides.
    pub lsp: Option<LspConfig>,
}

/// LSP configuration section from `.cq.toml`.
///
/// Controls idle timeout and per-language server overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct LspConfig {
    /// Idle timeout in minutes before shutting down a language server.
    pub timeout: Option<u64>,
    /// Per-language server overrides keyed by language name (e.g., `"rust"`, `"python"`).
    pub servers: HashMap<String, LspServerOverride>,
}

/// Per-language LSP server override from `.cq.toml`.
///
/// Allows overriding the binary and/or arguments for a language server.
/// Fields that are `None` fall back to the built-in defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct LspServerOverride {
    /// Override the binary name or path for this language's server.
    pub binary: Option<String>,
    /// Override the command-line arguments for this language's server.
    pub args: Option<Vec<String>>,
}

/// The on-disk TOML structure for `.cq.toml`.
#[derive(Debug, serde::Deserialize)]
struct ConfigFile {
    project: Option<ProjectSection>,
    languages: Option<HashMap<String, String>>,
    lsp: Option<LspSection>,
}

/// The `[project]` section of `.cq.toml`.
#[derive(Debug, serde::Deserialize)]
struct ProjectSection {
    exclude: Option<Vec<String>>,
    cache: Option<bool>,
}

/// The `[lsp]` section of `.cq.toml`.
///
/// Top-level keys (`timeout`) are parsed directly. Sub-tables like `[lsp.rust]`
/// are collected into the `servers` map via serde's `flatten`.
#[derive(Debug, serde::Deserialize)]
struct LspSection {
    timeout: Option<u64>,
    #[serde(flatten)]
    servers: HashMap<String, LspServerOverrideToml>,
}

/// On-disk representation of a per-language LSP server override.
#[derive(Debug, serde::Deserialize)]
struct LspServerOverrideToml {
    binary: Option<String>,
    args: Option<Vec<String>>,
}

/// Load project configuration from `.cq.toml` at the given project root.
///
/// Returns `Ok(None)` if the config file does not exist.
/// Returns `Err` if the file exists but cannot be read or parsed.
///
/// # Errors
///
/// Returns `CoreError::Io` if the file cannot be read.
/// Returns `CoreError::Config` if the file contains invalid TOML or
/// does not match the expected schema.
pub fn load_config(project_root: &Path) -> Result<Option<ProjectConfig>> {
    let config_path = project_root.join(".cq.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&config_path)?;
    let parsed: ConfigFile = toml::from_str(&contents).map_err(|e| {
        CoreError::Config(format!(
            "invalid .cq.toml at {}: {e}",
            config_path.display()
        ))
    })?;

    let (exclude, cache_enabled) = match parsed.project {
        Some(project) => (project.exclude.unwrap_or_default(), project.cache),
        None => (Vec::new(), None),
    };

    let language_overrides = parsed.languages.unwrap_or_default();

    let lsp = parsed.lsp.map(|lsp_section| {
        let servers = lsp_section
            .servers
            .into_iter()
            .map(|(name, override_toml)| {
                (
                    name,
                    LspServerOverride {
                        binary: override_toml.binary,
                        args: override_toml.args,
                    },
                )
            })
            .collect();
        LspConfig {
            timeout: lsp_section.timeout,
            servers,
        }
    });

    Ok(Some(ProjectConfig {
        exclude,
        language_overrides,
        cache_enabled,
        lsp,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_returns_none_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let result = load_config(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_config_parses_full_config() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
exclude = ["vendor/**", "generated/**"]
cache = true

[languages]
".jsx" = "javascript"
".mjs" = "javascript"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert_eq!(config.exclude, vec!["vendor/**", "generated/**"]);
        assert_eq!(config.cache_enabled, Some(true));
        assert_eq!(
            config.language_overrides.get(".jsx"),
            Some(&"javascript".to_string())
        );
        assert_eq!(
            config.language_overrides.get(".mjs"),
            Some(&"javascript".to_string())
        );
    }

    #[test]
    fn test_load_config_parses_project_section_only() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
exclude = ["dist/**"]
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert_eq!(config.exclude, vec!["dist/**"]);
        assert_eq!(config.cache_enabled, None);
        assert!(config.language_overrides.is_empty());
    }

    #[test]
    fn test_load_config_parses_languages_section_only() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[languages]
".svelte" = "javascript"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert!(config.exclude.is_empty());
        assert_eq!(config.cache_enabled, None);
        assert_eq!(
            config.language_overrides.get(".svelte"),
            Some(&"javascript".to_string())
        );
    }

    #[test]
    fn test_load_config_parses_empty_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".cq.toml"), "").unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert!(config.exclude.is_empty());
        assert_eq!(config.cache_enabled, None);
        assert!(config.language_overrides.is_empty());
    }

    #[test]
    fn test_load_config_cache_false() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
cache = false
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert_eq!(config.cache_enabled, Some(false));
    }

    #[test]
    fn test_load_config_invalid_toml_returns_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".cq.toml"), "this is not valid toml {{{").unwrap();

        let result = load_config(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CoreError::Config(_)),
            "expected Config error, got: {err}"
        );
    }

    #[test]
    fn test_load_config_wrong_type_returns_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
exclude = "should-be-a-list"
"#,
        )
        .unwrap();

        let result = load_config(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CoreError::Config(_)),
            "expected Config error, got: {err}"
        );
    }

    #[test]
    fn test_load_config_multiple_language_overrides() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[languages]
".jsx" = "javascript"
".mjs" = "javascript"
".cjs" = "javascript"
".mts" = "typescript"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert_eq!(config.language_overrides.len(), 4);
        assert_eq!(
            config.language_overrides.get(".mts"),
            Some(&"typescript".to_string())
        );
    }

    #[test]
    fn test_load_config_parses_lsp_section_with_timeout() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[lsp]
timeout = 30
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        let lsp = config.lsp.unwrap();
        assert_eq!(lsp.timeout, Some(30));
        assert!(lsp.servers.is_empty());
    }

    #[test]
    fn test_load_config_parses_lsp_server_overrides() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[lsp]
timeout = 15

[lsp.rust]
binary = "rust-analyzer"
args = []

[lsp.python]
binary = "pylsp"
args = ["--log-file", "/tmp/pylsp.log"]
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        let lsp = config.lsp.unwrap();
        assert_eq!(lsp.timeout, Some(15));
        assert_eq!(lsp.servers.len(), 2);

        let rust_override = lsp.servers.get("rust").unwrap();
        assert_eq!(rust_override.binary, Some("rust-analyzer".to_string()));
        assert_eq!(rust_override.args, Some(vec![]));

        let python_override = lsp.servers.get("python").unwrap();
        assert_eq!(python_override.binary, Some("pylsp".to_string()));
        assert_eq!(
            python_override.args,
            Some(vec!["--log-file".to_string(), "/tmp/pylsp.log".to_string()])
        );
    }

    #[test]
    fn test_load_config_lsp_binary_only_override() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[lsp.go]
binary = "gopls-nightly"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        let lsp = config.lsp.unwrap();
        assert_eq!(lsp.timeout, None);

        let go_override = lsp.servers.get("go").unwrap();
        assert_eq!(go_override.binary, Some("gopls-nightly".to_string()));
        assert_eq!(go_override.args, None);
    }

    #[test]
    fn test_load_config_lsp_args_only_override() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[lsp.rust]
args = ["--log-file", "/tmp/ra.log"]
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        let lsp = config.lsp.unwrap();
        let rust_override = lsp.servers.get("rust").unwrap();
        assert_eq!(rust_override.binary, None);
        assert_eq!(
            rust_override.args,
            Some(vec!["--log-file".to_string(), "/tmp/ra.log".to_string()])
        );
    }

    #[test]
    fn test_load_config_no_lsp_section() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
cache = true
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert!(config.lsp.is_none());
    }

    #[test]
    fn test_load_config_full_config_with_lsp() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".cq.toml"),
            r#"
[project]
exclude = ["vendor/**"]
cache = true

[languages]
".jsx" = "javascript"

[lsp]
timeout = 30

[lsp.rust]
binary = "rust-analyzer"
args = []

[lsp.python]
binary = "pylsp"
args = ["--log-file", "/tmp/pylsp.log"]
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).unwrap().unwrap();
        assert_eq!(config.exclude, vec!["vendor/**"]);
        assert_eq!(config.cache_enabled, Some(true));
        assert_eq!(
            config.language_overrides.get(".jsx"),
            Some(&"javascript".to_string())
        );

        let lsp = config.lsp.unwrap();
        assert_eq!(lsp.timeout, Some(30));
        assert_eq!(lsp.servers.len(), 2);
        assert!(lsp.servers.contains_key("rust"));
        assert!(lsp.servers.contains_key("python"));
    }
}
