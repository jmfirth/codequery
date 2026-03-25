#![warn(clippy::pedantic)]

mod args;

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

#[allow(clippy::needless_pass_by_value)]
// Command dispatch will consume args fields (file, symbol) once Tasks 008/009 implement them
#[allow(clippy::unnecessary_wraps)]
// Stub always returns Ok; real command dispatch (Tasks 008/009) will produce errors
fn run(args: CqArgs) -> anyhow::Result<ExitCode> {
    match args.command {
        Command::Outline { file: _ } => {
            // TODO: Task 008 implements this
            eprintln!("outline not yet implemented");
            Ok(ExitCode::Success)
        }
        Command::Def { symbol: _ } => {
            // TODO: Task 009 implements this
            eprintln!("def not yet implemented");
            Ok(ExitCode::Success)
        }
    }
}
