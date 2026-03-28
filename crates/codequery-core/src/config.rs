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
}

/// The on-disk TOML structure for `.cq.toml`.
#[derive(Debug, serde::Deserialize)]
struct ConfigFile {
    project: Option<ProjectSection>,
    languages: Option<HashMap<String, String>>,
}

/// The `[project]` section of `.cq.toml`.
#[derive(Debug, serde::Deserialize)]
struct ProjectSection {
    exclude: Option<Vec<String>>,
    cache: Option<bool>,
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

    Ok(Some(ProjectConfig {
        exclude,
        language_overrides,
        cache_enabled,
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
}
