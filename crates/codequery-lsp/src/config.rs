//! Language server configuration and registry.
//!
//! Maps languages to their default language server binaries and arguments. The
//! registry provides built-in defaults for Tier 1 languages (except Java, which
//! has complex setup requirements). Supports per-language overrides from
//! `.cq.toml` and environment variables.

use std::collections::HashMap;

use codequery_core::{Language, LspServerOverride};

/// Configuration for spawning a language server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The binary name or path (e.g., "rust-analyzer", "clangd").
    pub binary: String,

    /// Command-line arguments to pass to the binary.
    pub args: Vec<String>,

    /// Additional environment variables to set for the process.
    pub env: Vec<(String, String)>,
}

/// Registry of default language server configurations per language.
///
/// Provides built-in defaults for Tier 1 languages that have straightforward
/// LSP server setups. Java is excluded because jdtls requires complex
/// workspace/launcher configuration.
#[derive(Debug)]
pub struct LanguageServerRegistry {
    /// Built-in configs indexed by language. Uses a Vec of pairs rather than
    /// a `HashMap` since the set is small and fixed.
    configs: Vec<(Language, ServerConfig)>,
}

impl LanguageServerRegistry {
    /// Creates a new registry with built-in defaults for Tier 1 languages.
    ///
    /// Default servers:
    /// - Rust: `rust-analyzer`
    /// - TypeScript/JavaScript: `typescript-language-server --stdio`
    /// - Python: `pyright-langserver --stdio`
    /// - Go: `gopls serve`
    /// - C/C++: `clangd`
    #[must_use]
    pub fn new() -> Self {
        let configs = vec![
            (
                Language::Rust,
                ServerConfig {
                    binary: "rust-analyzer".to_string(),
                    args: vec![],
                    env: vec![],
                },
            ),
            (
                Language::TypeScript,
                ServerConfig {
                    binary: "typescript-language-server".to_string(),
                    args: vec!["--stdio".to_string()],
                    env: vec![],
                },
            ),
            (
                Language::JavaScript,
                ServerConfig {
                    binary: "typescript-language-server".to_string(),
                    args: vec!["--stdio".to_string()],
                    env: vec![],
                },
            ),
            (
                Language::Python,
                ServerConfig {
                    binary: "pyright-langserver".to_string(),
                    args: vec!["--stdio".to_string()],
                    env: vec![],
                },
            ),
            (
                Language::Go,
                ServerConfig {
                    binary: "gopls".to_string(),
                    args: vec!["serve".to_string()],
                    env: vec![],
                },
            ),
            (
                Language::C,
                ServerConfig {
                    binary: "clangd".to_string(),
                    args: vec![],
                    env: vec![],
                },
            ),
            (
                Language::Cpp,
                ServerConfig {
                    binary: "clangd".to_string(),
                    args: vec![],
                    env: vec![],
                },
            ),
        ];

        Self { configs }
    }

    /// Creates a new registry with built-in defaults merged with user overrides.
    ///
    /// Overrides from `.cq.toml` are applied first, then environment variable
    /// overrides take highest precedence. Environment variables use the format
    /// `CQ_LSP_<LANG>=binary` (e.g., `CQ_LSP_RUST=my-rust-analyzer`).
    ///
    /// Only the `binary` and `args` fields from overrides are applied — other
    /// `ServerConfig` fields (like `env`) retain their defaults.
    #[must_use]
    pub fn with_overrides(overrides: &HashMap<String, LspServerOverride>) -> Self {
        let mut registry = Self::new();

        // Apply .cq.toml overrides
        for (lang_name, server_override) in overrides {
            if let Some(lang) = Language::from_name(lang_name) {
                registry.apply_override(lang, server_override);
            }
        }

        // Apply environment variable overrides (highest precedence).
        // Format: CQ_LSP_RUST=binary, CQ_LSP_PYTHON=binary, etc.
        for (lang, env_suffix) in Self::env_var_languages() {
            let var_name = format!("CQ_LSP_{env_suffix}");
            if let Ok(binary) = std::env::var(&var_name) {
                let env_override = LspServerOverride {
                    binary: Some(binary),
                    args: None,
                };
                registry.apply_override(lang, &env_override);
            }
        }

        registry
    }

    /// Returns the server configuration for the given language, if one exists.
    ///
    /// Returns `None` for languages without a built-in default (e.g., Java).
    #[must_use]
    pub fn config_for(&self, lang: Language) -> Option<&ServerConfig> {
        self.configs
            .iter()
            .find(|(l, _)| *l == lang)
            .map(|(_, config)| config)
    }

    /// Apply a single override to the registry for the given language.
    ///
    /// If the language already has a config, the override's non-`None` fields
    /// replace the corresponding defaults. If the language has no config yet,
    /// a new entry is created (requiring at least a binary name).
    fn apply_override(&mut self, lang: Language, server_override: &LspServerOverride) {
        if let Some((_, config)) = self.configs.iter_mut().find(|(l, _)| *l == lang) {
            // Merge into existing config
            if let Some(ref binary) = server_override.binary {
                config.binary.clone_from(binary);
            }
            if let Some(ref args) = server_override.args {
                config.args.clone_from(args);
            }
        } else if let Some(ref binary) = server_override.binary {
            // Create a new entry for a language that didn't have a default
            self.configs.push((
                lang,
                ServerConfig {
                    binary: binary.clone(),
                    args: server_override.args.clone().unwrap_or_default(),
                    env: vec![],
                },
            ));
        }
        // If no existing config and no binary specified, we can't create an entry
    }

    /// Returns the set of (`Language`, `ENV_SUFFIX`) pairs for env var lookups.
    fn env_var_languages() -> Vec<(Language, &'static str)> {
        vec![
            (Language::Rust, "RUST"),
            (Language::TypeScript, "TYPESCRIPT"),
            (Language::JavaScript, "JAVASCRIPT"),
            (Language::Python, "PYTHON"),
            (Language::Go, "GO"),
            (Language::C, "C"),
            (Language::Cpp, "CPP"),
            (Language::Java, "JAVA"),
        ]
    }
}

impl Default for LanguageServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_stores_binary_args_env() {
        let config = ServerConfig {
            binary: "test-server".to_string(),
            args: vec!["--stdio".to_string()],
            env: vec![("RUST_LOG".to_string(), "debug".to_string())],
        };
        assert_eq!(config.binary, "test-server");
        assert_eq!(config.args, vec!["--stdio"]);
        assert_eq!(
            config.env,
            vec![("RUST_LOG".to_string(), "debug".to_string())]
        );
    }

    #[test]
    fn test_server_config_clone_and_eq() {
        let config = ServerConfig {
            binary: "clangd".to_string(),
            args: vec![],
            env: vec![],
        };
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_registry_new_returns_defaults() {
        let registry = LanguageServerRegistry::new();
        // All Tier 1 languages except Java should have configs.
        assert!(registry.config_for(Language::Rust).is_some());
        assert!(registry.config_for(Language::TypeScript).is_some());
        assert!(registry.config_for(Language::JavaScript).is_some());
        assert!(registry.config_for(Language::Python).is_some());
        assert!(registry.config_for(Language::Go).is_some());
        assert!(registry.config_for(Language::C).is_some());
        assert!(registry.config_for(Language::Cpp).is_some());
    }

    #[test]
    fn test_registry_java_returns_none() {
        let registry = LanguageServerRegistry::new();
        assert!(registry.config_for(Language::Java).is_none());
    }

    #[test]
    fn test_registry_rust_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::Rust).unwrap();
        assert_eq!(config.binary, "rust-analyzer");
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
    }

    #[test]
    fn test_registry_typescript_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::TypeScript).unwrap();
        assert_eq!(config.binary, "typescript-language-server");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_registry_javascript_uses_typescript_server() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::JavaScript).unwrap();
        assert_eq!(config.binary, "typescript-language-server");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_registry_python_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::Python).unwrap();
        assert_eq!(config.binary, "pyright-langserver");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_registry_go_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::Go).unwrap();
        assert_eq!(config.binary, "gopls");
        assert_eq!(config.args, vec!["serve"]);
    }

    #[test]
    fn test_registry_c_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::C).unwrap();
        assert_eq!(config.binary, "clangd");
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_registry_cpp_config() {
        let registry = LanguageServerRegistry::new();
        let config = registry.config_for(Language::Cpp).unwrap();
        assert_eq!(config.binary, "clangd");
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_registry_c_and_cpp_share_clangd() {
        let registry = LanguageServerRegistry::new();
        let c_config = registry.config_for(Language::C).unwrap();
        let cpp_config = registry.config_for(Language::Cpp).unwrap();
        assert_eq!(c_config.binary, cpp_config.binary);
    }

    #[test]
    fn test_registry_default_trait() {
        let registry = LanguageServerRegistry::default();
        assert!(registry.config_for(Language::Rust).is_some());
    }

    #[test]
    fn test_registry_unsupported_language_returns_none() {
        let registry = LanguageServerRegistry::new();
        assert!(registry.config_for(Language::Ruby).is_none());
    }

    #[test]
    fn test_with_overrides_empty_overrides_returns_defaults() {
        let overrides = HashMap::new();
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Rust).unwrap();
        assert_eq!(config.binary, "rust-analyzer");
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_with_overrides_binary_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "rust".to_string(),
            LspServerOverride {
                binary: Some("my-rust-analyzer".to_string()),
                args: None,
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Rust).unwrap();
        assert_eq!(config.binary, "my-rust-analyzer");
        // args should remain the default (empty for rust)
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_with_overrides_args_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "rust".to_string(),
            LspServerOverride {
                binary: None,
                args: Some(vec!["--log-file".to_string(), "/tmp/ra.log".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Rust).unwrap();
        assert_eq!(config.binary, "rust-analyzer");
        assert_eq!(
            config.args,
            vec!["--log-file".to_string(), "/tmp/ra.log".to_string()]
        );
    }

    #[test]
    fn test_with_overrides_binary_and_args() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "python".to_string(),
            LspServerOverride {
                binary: Some("pylsp".to_string()),
                args: Some(vec!["--log-file".to_string(), "/tmp/pylsp.log".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Python).unwrap();
        assert_eq!(config.binary, "pylsp");
        assert_eq!(
            config.args,
            vec!["--log-file".to_string(), "/tmp/pylsp.log".to_string()]
        );
    }

    #[test]
    fn test_with_overrides_unrecognized_language_ignored() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "haskell".to_string(),
            LspServerOverride {
                binary: Some("hls".to_string()),
                args: None,
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        // Should not crash, and defaults should be unaffected
        assert!(registry.config_for(Language::Rust).is_some());
    }

    #[test]
    fn test_with_overrides_adds_new_language_config() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "java".to_string(),
            LspServerOverride {
                binary: Some("jdtls".to_string()),
                args: Some(vec!["--data".to_string(), "/tmp/jdt".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Java).unwrap();
        assert_eq!(config.binary, "jdtls");
        assert_eq!(
            config.args,
            vec!["--data".to_string(), "/tmp/jdt".to_string()]
        );
    }

    #[test]
    fn test_with_overrides_no_binary_no_existing_config_ignored() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "java".to_string(),
            LspServerOverride {
                binary: None,
                args: Some(vec!["--data".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        // Java has no default and no binary was specified, so no config should exist
        assert!(registry.config_for(Language::Java).is_none());
    }

    #[test]
    fn test_with_overrides_does_not_affect_other_languages() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "rust".to_string(),
            LspServerOverride {
                binary: Some("custom-ra".to_string()),
                args: None,
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        // Rust was overridden
        assert_eq!(
            registry.config_for(Language::Rust).unwrap().binary,
            "custom-ra"
        );
        // Python should be unaffected
        assert_eq!(
            registry.config_for(Language::Python).unwrap().binary,
            "pyright-langserver"
        );
    }

    #[test]
    fn test_with_overrides_multiple_languages() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "rust".to_string(),
            LspServerOverride {
                binary: Some("custom-ra".to_string()),
                args: None,
            },
        );
        overrides.insert(
            "go".to_string(),
            LspServerOverride {
                binary: Some("gopls-dev".to_string()),
                args: Some(vec!["serve".to_string(), "-rpc.trace".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        assert_eq!(
            registry.config_for(Language::Rust).unwrap().binary,
            "custom-ra"
        );
        let go_config = registry.config_for(Language::Go).unwrap();
        assert_eq!(go_config.binary, "gopls-dev");
        assert_eq!(
            go_config.args,
            vec!["serve".to_string(), "-rpc.trace".to_string()]
        );
    }

    #[test]
    fn test_env_var_override_takes_precedence() {
        // Set env var before creating registry
        std::env::set_var("CQ_LSP_RUST", "env-rust-analyzer");

        let mut overrides = HashMap::new();
        overrides.insert(
            "rust".to_string(),
            LspServerOverride {
                binary: Some("toml-rust-analyzer".to_string()),
                args: Some(vec!["--toml-arg".to_string()]),
            },
        );
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Rust).unwrap();
        // Env var should override the toml binary
        assert_eq!(config.binary, "env-rust-analyzer");
        // But the toml args should remain (env var only overrides binary)
        assert_eq!(config.args, vec!["--toml-arg"]);

        // Clean up env var
        std::env::remove_var("CQ_LSP_RUST");
    }

    #[test]
    fn test_env_var_override_without_toml_overrides() {
        std::env::set_var("CQ_LSP_PYTHON", "env-pylsp");

        let overrides = HashMap::new();
        let registry = LanguageServerRegistry::with_overrides(&overrides);
        let config = registry.config_for(Language::Python).unwrap();
        assert_eq!(config.binary, "env-pylsp");
        // Default args should remain
        assert_eq!(config.args, vec!["--stdio"]);

        std::env::remove_var("CQ_LSP_PYTHON");
    }

    #[test]
    fn test_env_var_languages_covers_tier_1() {
        let langs = LanguageServerRegistry::env_var_languages();
        let lang_set: Vec<Language> = langs.iter().map(|(l, _)| *l).collect();
        assert!(lang_set.contains(&Language::Rust));
        assert!(lang_set.contains(&Language::TypeScript));
        assert!(lang_set.contains(&Language::JavaScript));
        assert!(lang_set.contains(&Language::Python));
        assert!(lang_set.contains(&Language::Go));
        assert!(lang_set.contains(&Language::C));
        assert!(lang_set.contains(&Language::Cpp));
        assert!(lang_set.contains(&Language::Java));
    }
}
