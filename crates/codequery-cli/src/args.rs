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
}
