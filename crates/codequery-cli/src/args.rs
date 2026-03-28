use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// The output format for command results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Default: `@@ file:line:column kind name @@` headers with content.
    Framed,
    /// Structured JSON output for programmatic use.
    Json,
    /// Content only, no `@@` framing delimiters.
    Raw,
}

/// Semantic code query tool for the command line.
#[derive(Debug, Parser)]
#[command(name = "cq", version, about)]
#[allow(clippy::struct_excessive_bools)]
// CLI flag structs naturally use booleans for each flag; refactoring would hurt clarity
pub struct CqArgs {
    /// Explicit project root (overrides auto-detection)
    #[arg(long, global = true)]
    pub project: Option<PathBuf>,

    /// Narrow search scope to a directory or file
    #[arg(long = "in", global = true)]
    pub scope: Option<PathBuf>,

    /// JSON output for programmatic use
    #[arg(long, global = true, conflicts_with = "raw")]
    pub json: bool,

    /// Raw output, no framing
    #[arg(long, global = true, conflicts_with = "json")]
    pub raw: bool,

    /// Force pretty-printed JSON (default when TTY)
    #[arg(long, global = true)]
    pub pretty: bool,

    /// Filter results by symbol kind
    #[arg(long, global = true)]
    pub kind: Option<String>,

    /// Force language detection
    #[arg(long, global = true)]
    pub lang: Option<String>,

    /// Use language server for semantic precision (slow without daemon)
    #[arg(long, global = true, conflicts_with = "no_semantic")]
    pub semantic: bool,

    /// Disable semantic resolution even if daemon is running
    #[arg(long, global = true, conflicts_with = "semantic")]
    pub no_semantic: bool,

    /// Enable disk caching for faster repeated queries
    #[arg(long, global = true, conflicts_with = "no_cache")]
    pub cache: bool,

    /// Disable disk caching (overrides `CQ_CACHE` env var)
    #[arg(long, global = true, conflicts_with = "cache")]
    pub no_cache: bool,

    /// Lines of context around matches
    #[arg(long, global = true, default_value = "0")]
    pub context: usize,

    /// Limit nesting depth (for tree, context)
    #[arg(long, global = true)]
    pub depth: Option<usize>,

    /// Maximum number of results
    #[arg(long, global = true)]
    pub limit: Option<usize>,

    #[command(subcommand)]
    pub command: Command,
}

impl CqArgs {
    /// Derive the output mode from `--json` and `--raw` flags.
    pub fn output_mode(&self) -> OutputMode {
        if self.json {
            OutputMode::Json
        } else if self.raw {
            OutputMode::Raw
        } else {
            OutputMode::Framed
        }
    }

    /// Determine whether semantic (LSP-backed) resolution is enabled.
    ///
    /// Precedence: `--no-semantic` (force off) > `--semantic` (force on) > `CQ_SEMANTIC=1` env var.
    pub fn use_semantic(&self) -> bool {
        if self.no_semantic {
            return false;
        }
        if self.semantic {
            return true;
        }
        std::env::var("CQ_SEMANTIC")
            .map(|v| v == "1")
            .unwrap_or(false)
    }

    /// Determine whether disk caching is enabled.
    ///
    /// Precedence: `--no-cache` (force off) > `--cache` (force on) > `CQ_CACHE=1` env var.
    pub fn use_cache(&self) -> bool {
        if self.no_cache {
            return false;
        }
        if self.cache {
            return true;
        }
        std::env::var("CQ_CACHE").map(|v| v == "1").unwrap_or(false)
    }
}

/// Available cq subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List all symbols in a file
    Outline {
        /// Path to the file to outline
        file: PathBuf,
    },
    /// Find where a symbol is defined
    Def {
        /// Symbol name to search for
        symbol: String,
    },
    /// Extract the full body of a symbol
    Body {
        /// Symbol name to extract body for
        symbol: String,
    },
    /// Extract the signature of a symbol
    Sig {
        /// Symbol name to extract signature for
        symbol: String,
    },
    /// Find all references to a symbol
    Refs {
        /// Symbol name to find references for
        symbol: String,
    },
    /// Find all callers of a function
    Callers {
        /// Function name to find callers for
        symbol: String,
    },
    /// Show dependency relationships
    Deps {
        /// Symbol name to show dependencies for
        symbol: String,
    },
    /// List all symbols in scope
    Symbols,
    /// List imports in a file
    Imports {
        /// Path to the file to list imports for
        file: PathBuf,
    },
    /// Show code context around a location
    Context {
        /// Location in `file:line` format
        location: String,
    },
    /// Show project structure tree
    Tree {
        /// Path to root (defaults to project root)
        path: Option<PathBuf>,
    },
    /// Structural search using AST patterns
    Search {
        /// Pattern to search for (structural pattern or S-expression with --raw)
        pattern: String,
    },
    /// Manage the disk cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Manage the LSP daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Internal: run the daemon in the foreground (used by `daemon start`)
    #[command(name = "_daemon-run", hide = true)]
    DaemonRun,
}

/// Cache management sub-subcommands.
#[derive(Debug, Subcommand)]
pub enum CacheAction {
    /// Clear all cached data
    Clear,
}

/// Daemon management sub-subcommands.
#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// Start the daemon in the background
    Start,
    /// Stop a running daemon
    Stop,
    /// Show daemon status
    Status,
}

/// Process exit codes following SPECIFICATION.md section 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Success, results found
    Success = 0,
    /// No results found (query valid but matched nothing)
    NoResults = 1,
    /// Usage error (bad arguments, unknown command)
    UsageError = 2,
    /// Project error (no project root, no source files)
    ProjectError = 3,
    /// Parse warning (tree-sitter error, but results still returned from other files)
    ParseWarning = 4,
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        Self::from(code as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outline_command_captures_file() {
        let args = CqArgs::parse_from(["cq", "outline", "src/main.rs"]);
        match args.command {
            Command::Outline { file } => assert_eq!(file, PathBuf::from("src/main.rs")),
            _ => panic!("expected Outline command"),
        }
    }

    #[test]
    fn test_def_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "def", "handle_request"]);
        match args.command {
            Command::Def { symbol } => assert_eq!(symbol, "handle_request"),
            _ => panic!("expected Def command"),
        }
    }

    #[test]
    fn test_project_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--project", "/path", "outline", "f.rs"]);
        assert_eq!(args.project, Some(PathBuf::from("/path")));
        match args.command {
            Command::Outline { file } => assert_eq!(file, PathBuf::from("f.rs")),
            _ => panic!("expected Outline command"),
        }
    }

    #[test]
    fn test_scope_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--in", "src/", "def", "foo"]);
        assert_eq!(args.scope, Some(PathBuf::from("src/")));
        match args.command {
            Command::Def { symbol } => assert_eq!(symbol, "foo"),
            _ => panic!("expected Def command"),
        }
    }

    #[test]
    fn test_no_subcommand_fails() {
        let result = CqArgs::try_parse_from(["cq"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_exit_code_success_is_zero() {
        let code: std::process::ExitCode = ExitCode::Success.into();
        // std::process::ExitCode doesn't expose its value directly,
        // so we verify via the From<u8> path
        assert_eq!(ExitCode::Success as u8, 0);
        let _ = code; // ensure conversion compiles
    }

    #[test]
    fn test_exit_code_no_results_is_one() {
        assert_eq!(ExitCode::NoResults as u8, 1);
        let _code: std::process::ExitCode = ExitCode::NoResults.into();
    }

    #[test]
    fn test_exit_code_project_error_is_three() {
        assert_eq!(ExitCode::ProjectError as u8, 3);
        let _code: std::process::ExitCode = ExitCode::ProjectError.into();
    }

    #[test]
    fn test_exit_code_usage_error_is_two() {
        assert_eq!(ExitCode::UsageError as u8, 2);
    }

    #[test]
    fn test_exit_code_parse_warning_is_four() {
        assert_eq!(ExitCode::ParseWarning as u8, 4);
    }

    #[test]
    fn test_json_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--json", "def", "foo"]);
        assert!(args.json);
        assert!(!args.raw);
        assert_eq!(args.output_mode(), OutputMode::Json);
    }

    #[test]
    fn test_raw_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--raw", "def", "foo"]);
        assert!(args.raw);
        assert!(!args.json);
        assert_eq!(args.output_mode(), OutputMode::Raw);
    }

    #[test]
    fn test_json_and_raw_together_produce_error() {
        let result = CqArgs::try_parse_from(["cq", "--json", "--raw", "def", "foo"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_pretty_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--json", "--pretty", "def", "foo"]);
        assert!(args.json);
        assert!(args.pretty);
    }

    #[test]
    fn test_no_output_flags_defaults_to_framed() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        assert!(!args.json);
        assert!(!args.raw);
        assert_eq!(args.output_mode(), OutputMode::Framed);
    }

    #[test]
    fn test_kind_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--kind", "function", "def", "foo"]);
        assert_eq!(args.kind, Some("function".to_string()));
    }

    #[test]
    fn test_lang_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--lang", "rust", "def", "foo"]);
        assert_eq!(args.lang, Some("rust".to_string()));
    }

    #[test]
    fn test_context_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--context", "3", "def", "foo"]);
        assert_eq!(args.context, 3);
    }

    #[test]
    fn test_depth_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--depth", "2", "def", "foo"]);
        assert_eq!(args.depth, Some(2));
    }

    #[test]
    fn test_limit_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--limit", "10", "def", "foo"]);
        assert_eq!(args.limit, Some(10));
    }

    #[test]
    fn test_body_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "body", "my_func"]);
        match args.command {
            Command::Body { symbol } => assert_eq!(symbol, "my_func"),
            _ => panic!("expected Body command"),
        }
    }

    #[test]
    fn test_sig_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "sig", "my_func"]);
        match args.command {
            Command::Sig { symbol } => assert_eq!(symbol, "my_func"),
            _ => panic!("expected Sig command"),
        }
    }

    #[test]
    fn test_refs_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "refs", "my_func"]);
        match args.command {
            Command::Refs { symbol } => assert_eq!(symbol, "my_func"),
            _ => panic!("expected Refs command"),
        }
    }

    #[test]
    fn test_callers_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "callers", "my_func"]);
        match args.command {
            Command::Callers { symbol } => assert_eq!(symbol, "my_func"),
            _ => panic!("expected Callers command"),
        }
    }

    #[test]
    fn test_deps_command_captures_symbol() {
        let args = CqArgs::parse_from(["cq", "deps", "my_module"]);
        match args.command {
            Command::Deps { symbol } => assert_eq!(symbol, "my_module"),
            _ => panic!("expected Deps command"),
        }
    }

    #[test]
    fn test_symbols_command_parses() {
        let args = CqArgs::parse_from(["cq", "symbols"]);
        assert!(matches!(args.command, Command::Symbols));
    }

    #[test]
    fn test_imports_command_captures_file() {
        let args = CqArgs::parse_from(["cq", "imports", "src/main.rs"]);
        match args.command {
            Command::Imports { file } => assert_eq!(file, PathBuf::from("src/main.rs")),
            _ => panic!("expected Imports command"),
        }
    }

    #[test]
    fn test_context_command_captures_location() {
        let args = CqArgs::parse_from(["cq", "context", "src/main.rs:42"]);
        match args.command {
            Command::Context { location } => assert_eq!(location, "src/main.rs:42"),
            _ => panic!("expected Context command"),
        }
    }

    #[test]
    fn test_tree_command_parses_with_path() {
        let args = CqArgs::parse_from(["cq", "tree", "src/"]);
        match args.command {
            Command::Tree { path } => assert_eq!(path, Some(PathBuf::from("src/"))),
            _ => panic!("expected Tree command"),
        }
    }

    #[test]
    fn test_tree_command_parses_without_path() {
        let args = CqArgs::parse_from(["cq", "tree"]);
        match args.command {
            Command::Tree { path } => assert!(path.is_none()),
            _ => panic!("expected Tree command"),
        }
    }

    #[test]
    fn test_search_command_captures_pattern() {
        let args = CqArgs::parse_from(["cq", "search", "fn $NAME() {}"]);
        match args.command {
            Command::Search { pattern } => assert_eq!(pattern, "fn $NAME() {}"),
            _ => panic!("expected Search command"),
        }
    }

    #[test]
    fn test_search_command_with_raw_flag() {
        let args = CqArgs::parse_from(["cq", "--raw", "search", "(function_item) @func"]);
        assert!(args.raw);
        match args.command {
            Command::Search { pattern } => {
                assert_eq!(pattern, "(function_item) @func");
            }
            _ => panic!("expected Search command"),
        }
    }

    // -----------------------------------------------------------------------
    // Cache flags
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--cache", "def", "foo"]);
        assert!(args.cache);
        assert!(!args.no_cache);
    }

    #[test]
    fn test_no_cache_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--no-cache", "def", "foo"]);
        assert!(!args.cache);
        assert!(args.no_cache);
    }

    #[test]
    fn test_cache_and_no_cache_together_produce_error() {
        let result = CqArgs::try_parse_from(["cq", "--cache", "--no-cache", "def", "foo"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_use_cache_returns_false_by_default() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        // Remove CQ_CACHE env if set
        std::env::remove_var("CQ_CACHE");
        assert!(!args.use_cache());
    }

    #[test]
    fn test_use_cache_returns_true_with_cache_flag() {
        let args = CqArgs::parse_from(["cq", "--cache", "def", "foo"]);
        assert!(args.use_cache());
    }

    #[test]
    fn test_use_cache_returns_false_with_no_cache_flag() {
        let args = CqArgs::parse_from(["cq", "--no-cache", "def", "foo"]);
        // Even if env is set, --no-cache wins
        std::env::set_var("CQ_CACHE", "1");
        assert!(!args.use_cache());
        std::env::remove_var("CQ_CACHE");
    }

    #[test]
    fn test_use_cache_respects_cq_cache_env_var() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        std::env::set_var("CQ_CACHE", "1");
        assert!(args.use_cache());
        std::env::remove_var("CQ_CACHE");
    }

    #[test]
    fn test_use_cache_ignores_non_one_env_var() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        std::env::set_var("CQ_CACHE", "yes");
        assert!(!args.use_cache());
        std::env::remove_var("CQ_CACHE");
    }

    // -----------------------------------------------------------------------
    // Cache subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_clear_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "cache", "clear"]);
        match args.command {
            Command::Cache { action } => {
                assert!(matches!(action, CacheAction::Clear));
            }
            _ => panic!("expected Cache command"),
        }
    }

    #[test]
    fn test_cache_subcommand_without_action_fails() {
        let result = CqArgs::try_parse_from(["cq", "cache"]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Semantic flags
    // -----------------------------------------------------------------------

    #[test]
    fn test_semantic_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--semantic", "def", "foo"]);
        assert!(args.semantic);
        assert!(!args.no_semantic);
    }

    #[test]
    fn test_no_semantic_flag_parsed() {
        let args = CqArgs::parse_from(["cq", "--no-semantic", "def", "foo"]);
        assert!(!args.semantic);
        assert!(args.no_semantic);
    }

    #[test]
    fn test_semantic_and_no_semantic_together_produce_error() {
        let result = CqArgs::try_parse_from(["cq", "--semantic", "--no-semantic", "def", "foo"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_use_semantic_returns_false_by_default() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        std::env::remove_var("CQ_SEMANTIC");
        assert!(!args.use_semantic());
    }

    #[test]
    fn test_use_semantic_returns_true_with_semantic_flag() {
        let args = CqArgs::parse_from(["cq", "--semantic", "def", "foo"]);
        assert!(args.use_semantic());
    }

    #[test]
    fn test_use_semantic_returns_false_with_no_semantic_flag() {
        let args = CqArgs::parse_from(["cq", "--no-semantic", "def", "foo"]);
        // Even if env is set, --no-semantic wins
        std::env::set_var("CQ_SEMANTIC", "1");
        assert!(!args.use_semantic());
        std::env::remove_var("CQ_SEMANTIC");
    }

    #[test]
    fn test_use_semantic_respects_cq_semantic_env_var() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        std::env::set_var("CQ_SEMANTIC", "1");
        assert!(args.use_semantic());
        std::env::remove_var("CQ_SEMANTIC");
    }

    #[test]
    fn test_use_semantic_ignores_non_one_env_var() {
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        std::env::set_var("CQ_SEMANTIC", "yes");
        assert!(!args.use_semantic());
        std::env::remove_var("CQ_SEMANTIC");
    }

    // -----------------------------------------------------------------------
    // Daemon subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_daemon_start_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "daemon", "start"]);
        match args.command {
            Command::Daemon { action } => {
                assert!(matches!(action, DaemonAction::Start));
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn test_daemon_stop_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "daemon", "stop"]);
        match args.command {
            Command::Daemon { action } => {
                assert!(matches!(action, DaemonAction::Stop));
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn test_daemon_status_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "daemon", "status"]);
        match args.command {
            Command::Daemon { action } => {
                assert!(matches!(action, DaemonAction::Status));
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn test_daemon_subcommand_without_action_fails() {
        let result = CqArgs::try_parse_from(["cq", "daemon"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_daemon_run_hidden_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "_daemon-run"]);
        assert!(matches!(args.command, Command::DaemonRun));
    }
}
