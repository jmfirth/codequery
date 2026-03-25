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
    match args.command {
        Command::Outline { file } => commands::outline::run(&file, args.project.as_deref()),
        Command::Def { symbol } => {
            commands::def::run(&symbol, args.project.as_deref(), args.scope.as_deref())
        }
    }
}
