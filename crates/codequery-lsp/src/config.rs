//! Language server configuration and registry.
//!
//! Maps languages to their default language server binaries and arguments. The
//! registry provides built-in defaults for Tier 1 languages (except Java, which
//! has complex setup requirements).

use codequery_core::Language;

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
}
