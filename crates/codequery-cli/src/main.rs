#![warn(clippy::pedantic)]

mod args;
mod commands;
mod output;

use args::{Command, CqArgs, ExitCode};
use clap::Parser;

fn main() -> std::process::ExitCode {
    let args = CqArgs::parse();
    match run(args) {
        Ok(code) => code.into(),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::ProjectError.into()
        }
    }
}

fn run(args: CqArgs) -> anyhow::Result<ExitCode> {
    let mode = args.output_mode();
    let pretty = args.pretty;
    match args.command {
        Command::Outline { file } => {
            commands::outline::run(&file, args.project.as_deref(), mode, pretty)
        }
        Command::Def { symbol } => commands::def::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
        ),
        Command::Body { symbol } => commands::body::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
        ),
        Command::Sig { symbol } => commands::sig::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
        ),
        Command::Imports { file } => {
            commands::imports::run(&file, args.project.as_deref(), mode, pretty)
        }
        Command::Context { location } => {
            commands::context::run(&location, args.project.as_deref(), mode, pretty, args.depth)
        }
        Command::Symbols => commands::symbols::run(
            args.project.as_deref(),
            args.scope.as_deref(),
            args.kind.as_deref(),
            args.limit,
            mode,
            pretty,
        ),
        Command::Tree { path } => commands::tree::run(
            path.as_deref(),
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            args.depth,
        ),
        Command::Refs { symbol } => commands::refs::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            args.context,
        ),
        Command::Deps { symbol } => commands::deps::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
        ),
        Command::Callers { .. } => {
            eprintln!("not yet implemented");
            Ok(ExitCode::Success)
        }
    }
}
