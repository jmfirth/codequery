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
#[command(
    name = "cq",
    version,
    about,
    long_about = "Semantic code query tool for AI agents and humans.\n\
                   75 languages. Three-tier precision: tree-sitter, stack graphs, and LSP.\n\
                   Languages auto-install on first use. Works on broken code.",
    after_help = "\x1b[1mExamples:\x1b[0m\n  \
                   cq def handle_request          Find where handle_request is defined\n  \
                   cq body Router::add_route       Extract the full source of a method\n  \
                   cq refs Config --semantic       Find all references using LSP precision\n  \
                   cq outline src/main.rs          List all symbols in a file\n  \
                   cq search '(function_item) @f'   Find Rust functions via S-expression"
)]
#[allow(clippy::struct_excessive_bools)]
// CLI flag structs naturally use booleans for each flag; refactoring would hurt clarity
pub struct CqArgs {
    /// Explicit project root (overrides auto-detection)
    #[arg(
        long,
        global = true,
        long_help = "Set the project root directory explicitly, overriding auto-detection.\n\
                     By default, cq walks up from the current directory looking for VCS roots\n\
                     (.git, .hg) or language markers (Cargo.toml, package.json, go.mod, etc.)."
    )]
    pub project: Option<PathBuf>,

    /// Narrow file discovery to a directory or file
    #[arg(
        long = "in",
        global = true,
        long_help = "Restrict file discovery to a subdirectory or single file.\n\
                     Accepts relative or absolute paths. Useful for focusing a wide command\n\
                     like `refs` or `symbols` on a specific part of the codebase."
    )]
    pub scope: Option<PathBuf>,

    /// JSON output for programmatic use
    #[arg(
        long,
        global = true,
        conflicts_with = "raw",
        long_help = "Emit structured JSON output instead of the default framed text format.\n\
                     Useful for piping into jq or consuming from scripts and AI agents.\n\
                     Use --pretty for human-readable indented JSON."
    )]
    pub json: bool,

    /// Raw output, no @@ framing delimiters
    #[arg(
        long,
        global = true,
        conflicts_with = "json",
        long_help = "Emit raw output without @@ framing delimiters.\n\
                     Produces plain source text suitable for piping into other tools.\n\
                     For the `search` command, --raw emits raw matched text\n\
                     without framing delimiters."
    )]
    pub raw: bool,

    /// Force pretty-printed JSON (default when TTY)
    #[arg(
        long,
        global = true,
        long_help = "Pretty-print JSON output with indentation and newlines.\n\
                     This is the default when outputting to a terminal; use this flag\n\
                     to force pretty-printing when piping to a file or another command."
    )]
    pub pretty: bool,

    /// Filter results by symbol kind (e.g., function, struct, class)
    #[arg(
        long,
        global = true,
        long_help = "Filter results to only include symbols of the specified kind.\n\
                     Common kinds: function, struct, class, trait, interface, method,\n\
                     enum, constant, variable, type_alias, module. The exact set of\n\
                     available kinds depends on the language."
    )]
    pub kind: Option<String>,

    /// Force language detection (e.g., rust, typescript, python, go)
    #[arg(
        long,
        global = true,
        long_help = "Override automatic language detection for the target files.\n\
                     Useful when file extensions are non-standard or ambiguous.\n\
                     Supported: rust, typescript, javascript, python, go, c, cpp,\n\
                     java, ruby, php, c_sharp, kotlin, scala, swift, hcl, zig."
    )]
    pub lang: Option<String>,

    /// Use language server for compiler-level semantic precision
    #[arg(
        long,
        global = true,
        conflicts_with = "no_semantic",
        long_help = "Enable LSP-backed resolution for compiler-level precision.\n\
                     Slower than the default syntactic analysis, but resolves through\n\
                     type aliases, trait impls, and cross-module re-exports.\n\
                     Much faster when the daemon is running (`cq daemon start`).\n\
                     Can also be enabled via CQ_SEMANTIC=1 env var."
    )]
    pub semantic: bool,

    /// Disable semantic resolution even if daemon is running
    #[arg(
        long,
        global = true,
        conflicts_with = "semantic",
        long_help = "Force semantic resolution off, even if the daemon is running\n\
                     or CQ_SEMANTIC=1 is set. Useful when you want fast syntactic\n\
                     results and don't need compiler-level precision."
    )]
    pub no_semantic: bool,

    /// Cache symbol indexes to disk for faster repeated queries
    #[arg(
        long,
        global = true,
        conflicts_with = "no_cache",
        long_help = "Enable disk caching of symbol indexes and scan results.\n\
                     Cached data is stored in a project-local .cq-cache directory\n\
                     and is invalidated automatically when files change.\n\
                     Can also be enabled via CQ_CACHE=1 env var."
    )]
    pub cache: bool,

    /// Disable disk caching (overrides `CQ_CACHE` env var)
    #[arg(
        long,
        global = true,
        conflicts_with = "cache",
        long_help = "Force disk caching off, overriding the CQ_CACHE=1 env var.\n\
                     Ensures every query re-scans from scratch with no stale data."
    )]
    pub no_cache: bool,

    /// Lines of surrounding context, like grep -C
    #[arg(
        long,
        global = true,
        default_value = "0",
        long_help = "Show N lines of surrounding source context around each match,\n\
                     similar to grep -C. Applies to refs, callers, and other commands\n\
                     that return source locations."
    )]
    pub context: usize,

    /// Limit nesting depth (for tree, context)
    #[arg(
        long,
        global = true,
        long_help = "Limit nesting depth in commands that produce hierarchical output.\n\
                     For `tree`: controls directory depth. For `context`: controls how\n\
                     many enclosing scopes to show. For `outline`: limits symbol nesting."
    )]
    pub depth: Option<usize>,

    /// Maximum number of results to return
    #[arg(
        long,
        global = true,
        long_help = "Cap the number of results returned. Applies to commands that can\n\
                     produce many matches (refs, callers, symbols, search). Useful for\n\
                     getting a quick sample without scanning the entire project."
    )]
    pub limit: Option<usize>,

    #[command(subcommand)]
    pub command: Command,
}

impl CqArgs {
    /// Derive the output mode from `--json` and `--raw` flags.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    /// List all symbols in a file with their kinds and nesting
    #[command(
        long_about = "Parse a file and list every symbol (functions, types, constants, etc.)\n\
                      with kind, line number, and nesting structure. Uses tree-sitter for\n\
                      language-aware parsing -- works even on files with syntax errors.",
        after_help = "Examples:\n  cq outline src/main.rs\n  cq outline lib.py --json"
    )]
    Outline {
        /// File to outline (relative or absolute path)
        file: PathBuf,
    },
    /// Find where a symbol is defined across the project
    #[command(
        long_about = "Search the entire project for a symbol's definition site.\n\
                      Uses a fast text pre-filter (memchr) to skip files that don't contain\n\
                      the symbol name, then parses only candidate files with tree-sitter.\n\
                      Supports qualified names like `Router::add_route`.",
        after_help = "Examples:\n  cq def handle_request\n  cq def Router::add_route --lang rust"
    )]
    Def {
        /// Symbol name to find (supports qualified names like `Struct::method`)
        symbol: String,
    },
    /// Extract the full source body of a symbol definition
    #[command(
        long_about = "Find a symbol's definition and return its complete source code,\n\
                      including the body of functions, struct/class definitions, etc.\n\
                      Uses the same fast pre-filter as `def` to locate the symbol.",
        after_help = "Examples:\n  cq body handle_request\n  cq body Config --json"
    )]
    Body {
        /// Symbol name to extract (supports qualified names like `Struct::method`)
        symbol: String,
    },
    /// Extract the signature of a symbol (without body)
    #[command(
        long_about = "Find a symbol's definition and return only its signature -- the\n\
                      function prototype, struct declaration, or class header without\n\
                      the implementation body. Useful for quick API inspection.",
        after_help = "Examples:\n  cq sig handle_request\n  cq sig MyClass::__init__"
    )]
    Sig {
        /// Symbol name to extract signature for (supports qualified names)
        symbol: String,
    },
    /// Find all references to a symbol across the project
    #[command(
        long_about = "Find every occurrence of a symbol throughout the codebase.\n\
                      Uses a three-tier resolution cascade:\n\
                      1. Syntactic: fast text + AST matching (default)\n\
                      2. Resolved: stack graph analysis for same-file precision\n\
                      3. Semantic: LSP-backed, compiler-level accuracy (--semantic flag)\n\
                      Each tier is progressively slower but more precise.",
        after_help = "Examples:\n  cq refs Config\n  cq refs handle_request --semantic --context 3"
    )]
    Refs {
        /// Symbol name to find references for (supports qualified names)
        symbol: String,
    },
    /// Find all callers of a function across the project
    #[command(
        long_about = "Find every call site for a function or method across the codebase.\n\
                      Like `refs` but filtered to only call expressions. Supports the same\n\
                      three-tier resolution cascade (syntactic, resolved, semantic).",
        after_help = "Examples:\n  cq callers handle_request\n  cq callers send --semantic"
    )]
    Callers {
        /// Function name to find callers for (supports qualified names)
        symbol: String,
    },
    /// Show dependency relationships for a symbol
    #[command(
        long_about = "Analyze and display the dependency graph for a symbol -- what it\n\
                      depends on and what depends on it. Combines import analysis with\n\
                      reference tracking to map symbol relationships.",
        after_help = "Examples:\n  cq deps Config\n  cq deps Router --json"
    )]
    Deps {
        /// Symbol name to show dependencies for (supports qualified names)
        symbol: String,
    },
    /// List all symbols in the project
    #[command(
        long_about = "Scan the entire project and list every symbol found across all files.\n\
                      Parses all source files in parallel using tree-sitter. Use --kind to\n\
                      filter by symbol type, --in to narrow scope, or --limit to cap output.",
        after_help = "Examples:\n  cq symbols\n  cq symbols --kind function --json\n  cq symbols --in src/api/ --limit 50"
    )]
    Symbols,
    /// List imports and use statements in a file
    #[command(
        long_about = "Parse a file and extract all import/use/require statements.\n\
                      Identifies both the imported names and their source modules.\n\
                      Works across all 16 supported languages.",
        after_help = "Examples:\n  cq imports src/main.rs\n  cq imports lib.py --json"
    )]
    Imports {
        /// File to list imports for (relative or absolute path)
        file: PathBuf,
    },
    /// Show enclosing code context around a file location
    #[command(
        long_about = "Given a file:line location, show the enclosing symbol context --\n\
                      the function, class, or block that contains that line.\n\
                      Use --depth to control how many nesting levels to show.",
        after_help = "Examples:\n  cq context src/main.rs:42\n  cq context lib.py:100 --depth 2"
    )]
    Context {
        /// Location as `file:line` (e.g., `src/main.rs:42`)
        location: String,
    },
    /// Show project file and directory structure
    #[command(
        long_about = "Display the project's file and directory tree, respecting .gitignore\n\
                      and other VCS ignore rules. Use --depth to limit directory depth.\n\
                      Optionally pass a path to root the tree at a subdirectory.",
        after_help = "Examples:\n  cq tree\n  cq tree src/ --depth 2"
    )]
    Tree {
        /// Root path to display (defaults to project root)
        path: Option<PathBuf>,
    },
    /// Structural search using tree-sitter S-expression queries
    #[command(
        long_about = "Search for code matching a tree-sitter S-expression query.\n\
                      S-expressions match against the AST — use `cq tree <file>` to\n\
                      explore node types for a language. Captures (@name) control\n\
                      which part of the match is returned.",
        after_help = "Examples:\n  \
                     cq search '(function_item name: (identifier) @name)'\n  \
                     cq search '(function_item name: (identifier) @name (#eq? @name \"main\"))'\n  \
                     cq search '(class_declaration name: (identifier) @name)' --lang typescript\n  \
                     cq search '(if_statement condition: (_) @cond) @match'"
    )]
    Search {
        /// Tree-sitter S-expression query pattern
        pattern: String,
    },
    /// Find unreferenced (dead) symbols in the project
    #[command(
        long_about = "Find symbols with zero references across the project.\n\
                      Scans all files for symbols and references, then reports symbols\n\
                      whose name never appears as a reference. Useful for finding dead\n\
                      code during refactoring. Focuses on private symbols by default;\n\
                      public symbols are flagged with a warning since they may have\n\
                      external callers.",
        after_help = "Examples:\n  cq dead\n  \
                     cq dead --kind function\n  \
                     cq dead --in src/legacy/"
    )]
    Dead,
    /// Show syntax errors and language server diagnostics
    #[command(
        long_about = "Show syntax errors and semantic diagnostics for a file or project.\n\
                      Always shows tree-sitter parse errors (syntax layer). When the daemon\n\
                      is running or --semantic is used, also shows language server diagnostics.",
        after_help = "Examples:\n  cq diagnostics src/main.rs\n  \
                     cq diagnostics\n  \
                     cq diagnostics --in src/"
    )]
    Diagnostics {
        /// File to check (omit for whole project)
        file: Option<PathBuf>,
    },
    /// Show type info, docs, and signature at a source location
    #[command(
        long_about = "Show type information, documentation, and signature for the symbol\n\
                      at a given source location. Uses AST analysis by default; with\n\
                      --semantic or a running daemon, uses the language server for\n\
                      precise type resolution.",
        after_help = "Examples:\n  cq hover src/main.rs:42:8\n  \
                     cq hover src/lib.rs:10"
    )]
    Hover {
        /// Location as `file:line[:column]` (e.g. `src/main.rs:42:8`)
        location: String,
    },
    /// Rename a symbol across the project
    #[command(
        long_about = "Rename a symbol across the project. Finds all references and\n\
                      replaces them with the new name. Applies immediately when using\n\
                      semantic or resolved precision; shows a preview diff when using\n\
                      syntactic precision. Use --apply to force write at any tier,\n\
                      or --dry-run to force preview.",
        after_help = "Examples:\n  cq rename OldName NewName\n  \
                     cq rename foo bar --apply\n  \
                     cq rename Handler Router --dry-run"
    )]
    Rename {
        /// Current symbol name
        old: String,
        /// New symbol name
        new: String,
        /// Force apply changes regardless of precision tier
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Force preview mode (don't apply changes)
        #[arg(long, conflicts_with = "apply")]
        dry_run: bool,
    },
    /// Trace multi-level call hierarchy for a symbol
    #[command(
        long_about = "Trace the call hierarchy for a symbol, recursively finding\n\
                      callers up to a configurable depth. Shows who calls the target\n\
                      symbol, who calls those callers, and so on.",
        after_help = "Examples:\n  cq callchain handle_request\n  \
                     cq callchain process --depth 5"
    )]
    Callchain {
        /// Symbol name to trace call hierarchy for
        symbol: String,
    },
    /// Show type hierarchy (supertypes and subtypes)
    #[command(
        long_about = "Show the type hierarchy for a type — what it extends/implements\n\
                      and what extends/implements it. Uses structural AST matching\n\
                      by default; with --semantic, uses the language server.",
        after_help = "Examples:\n  cq hierarchy Iterator\n  \
                     cq hierarchy Animal --lang typescript"
    )]
    Hierarchy {
        /// Type name to show hierarchy for
        symbol: String,
    },
    /// Manage the disk cache
    #[command(
        long_about = "Manage the .cq-cache directory used for disk caching of symbol\n\
                      indexes and scan results. Caching is opt-in via --cache or CQ_CACHE=1."
    )]
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Manage the LSP daemon for fast semantic queries
    #[command(
        long_about = "Control the background LSP daemon that keeps language servers warm.\n\
                      A running daemon makes --semantic queries much faster by reusing\n\
                      an already-initialized language server instead of cold-starting one\n\
                      for each query.",
        after_help = "Examples:\n  cq daemon start\n  cq daemon status\n  cq daemon stop"
    )]
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Internal: run the daemon in the foreground (used by `daemon start`)
    #[command(name = "_daemon-run", hide = true)]
    DaemonRun {
        /// Project root for this daemon instance.
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Manage language grammar packages
    #[command(
        long_about = "Install, remove, and inspect language grammar packages.\n\
                      cq ships with 16 built-in languages. Additional languages can be\n\
                      installed as grammar packages stored in ~/.local/share/cq/languages/.",
        after_help = "Examples:\n  cq grammar list\n  cq grammar install elixir\n  cq grammar info haskell"
    )]
    Grammar {
        #[command(subcommand)]
        action: GrammarAction,
    },
    /// Check for and install a newer version of cq
    #[command(
        long_about = "Check the GitHub releases API for a newer version of cq and print\n\
                      upgrade instructions. Does not perform the upgrade automatically."
    )]
    Upgrade,
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

/// Grammar package management sub-subcommands.
#[derive(Debug, Subcommand)]
pub enum GrammarAction {
    /// List installed and available language packages
    List,
    /// Install a language package
    Install {
        /// Language name (e.g., elixir, haskell, dart). Ignored when --all is set.
        language: Option<String>,
        /// Install all available packages from the registry
        #[arg(long)]
        all: bool,
    },
    /// Update all installed language packages to current version
    Update,
    /// Remove an installed language package
    Remove {
        /// Language name to remove
        language: String,
    },
    /// Show details about a language package
    Info {
        /// Language name
        language: String,
    },
    /// Validate extract.toml queries compile against the grammar
    Validate {
        /// Language name (omit for --all)
        language: Option<String>,
        /// Validate all installed grammars
        #[arg(long)]
        all: bool,
    },
}

/// Process exit codes following SPECIFICATION.md section 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Success (results found, or query valid with no matches)
    Success = 0,
    /// Usage error (bad arguments, unknown command)
    UsageError = 2,
    /// Project error (no project root, no source files, grammar unavailable)
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
        let args = CqArgs::parse_from(["cq", "search", "(function_item name: (identifier) @name)"]);
        match args.command {
            Command::Search { pattern } => {
                assert_eq!(pattern, "(function_item name: (identifier) @name)");
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

    // All CQ_CACHE env var tests in one function to prevent parallel races.
    // set_var/remove_var is process-global — separate #[test] functions race.
    #[test]
    fn test_use_cache_behavior() {
        // Default: false (clean env)
        std::env::remove_var("CQ_CACHE");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        assert!(!args.use_cache());

        // --cache flag: true
        let args = CqArgs::parse_from(["cq", "--cache", "def", "foo"]);
        assert!(args.use_cache());

        // --no-cache wins over env
        std::env::set_var("CQ_CACHE", "1");
        let args = CqArgs::parse_from(["cq", "--no-cache", "def", "foo"]);
        assert!(!args.use_cache());
        std::env::remove_var("CQ_CACHE");

        // CQ_CACHE=1 enables caching
        std::env::set_var("CQ_CACHE", "1");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        assert!(args.use_cache());
        std::env::remove_var("CQ_CACHE");

        // CQ_CACHE=yes is not "1", so no caching
        std::env::set_var("CQ_CACHE", "yes");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
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

    // All CQ_SEMANTIC env var tests in one function to prevent parallel races.
    #[test]
    fn test_use_semantic_behavior() {
        // Default: false (clean env)
        std::env::remove_var("CQ_SEMANTIC");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        assert!(!args.use_semantic());

        // --semantic flag: true
        let args = CqArgs::parse_from(["cq", "--semantic", "def", "foo"]);
        assert!(args.use_semantic());

        // --no-semantic wins over env
        std::env::set_var("CQ_SEMANTIC", "1");
        let args = CqArgs::parse_from(["cq", "--no-semantic", "def", "foo"]);
        assert!(!args.use_semantic());
        std::env::remove_var("CQ_SEMANTIC");

        // CQ_SEMANTIC=1 enables semantic
        std::env::set_var("CQ_SEMANTIC", "1");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
        assert!(args.use_semantic());
        std::env::remove_var("CQ_SEMANTIC");

        // CQ_SEMANTIC=yes is not "1", so no semantic
        std::env::set_var("CQ_SEMANTIC", "yes");
        let args = CqArgs::parse_from(["cq", "def", "foo"]);
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
        assert!(matches!(args.command, Command::DaemonRun { .. }));
    }

    // -----------------------------------------------------------------------
    // Grammar subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_grammar_list_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "list"]);
        match args.command {
            Command::Grammar { action } => {
                assert!(matches!(action, GrammarAction::List));
            }
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_install_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "install", "elixir"]);
        match args.command {
            Command::Grammar { action } => match action {
                GrammarAction::Install { language, all } => {
                    assert_eq!(language, Some("elixir".to_string()));
                    assert!(!all);
                }
                _ => panic!("expected Install action"),
            },
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_install_all_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "install", "--all"]);
        match args.command {
            Command::Grammar { action } => match action {
                GrammarAction::Install { language, all } => {
                    assert!(all);
                    assert!(language.is_none());
                }
                _ => panic!("expected Install action"),
            },
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_update_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "update"]);
        match args.command {
            Command::Grammar { action } => {
                assert!(matches!(action, GrammarAction::Update));
            }
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_remove_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "remove", "elixir"]);
        match args.command {
            Command::Grammar { action } => match action {
                GrammarAction::Remove { language } => assert_eq!(language, "elixir"),
                _ => panic!("expected Remove action"),
            },
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_info_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "grammar", "info", "haskell"]);
        match args.command {
            Command::Grammar { action } => match action {
                GrammarAction::Info { language } => assert_eq!(language, "haskell"),
                _ => panic!("expected Info action"),
            },
            _ => panic!("expected Grammar command"),
        }
    }

    #[test]
    fn test_grammar_subcommand_without_action_fails() {
        let result = CqArgs::try_parse_from(["cq", "grammar"]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Upgrade subcommand
    // -----------------------------------------------------------------------

    #[test]
    fn test_upgrade_subcommand_parsed() {
        let args = CqArgs::parse_from(["cq", "upgrade"]);
        assert!(matches!(args.command, Command::Upgrade));
    }
}
