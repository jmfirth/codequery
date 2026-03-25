#![warn(clippy::pedantic)]

mod args;
mod commands;
#[allow(dead_code)]
// format_def_results and format_frame_header are consumed by Task 009 (def command)
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
        Command::Def { symbol: _ } => {
            // TODO: Task 009 implements this
            eprintln!("def not yet implemented");
            Ok(ExitCode::Success)
        }
    }
}
