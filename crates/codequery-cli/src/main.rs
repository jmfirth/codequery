#![warn(clippy::pedantic)]

mod args;
mod commands;
mod output;

use args::{CacheAction, Command, CqArgs, ExitCode};
use clap::Parser;
use codequery_core::Language;

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

/// Parse the `--lang` flag into a `Language`, returning a usage error on invalid values.
fn parse_lang_filter(lang: Option<&String>) -> anyhow::Result<Option<Language>> {
    match lang {
        None => Ok(None),
        Some(s) => Language::from_name(s).map(Some).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown language: {s}. valid languages: rust, typescript, ts, javascript, js, \
                 python, py, go, c, cpp, c++, java"
            )
        }),
    }
}

fn run(args: CqArgs) -> anyhow::Result<ExitCode> {
    let mode = args.output_mode();
    let pretty = args.pretty;
    let use_cache = args.use_cache();
    let lang_filter = parse_lang_filter(args.lang.as_ref())?;
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
            lang_filter,
        ),
        Command::Body { symbol } => commands::body::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            lang_filter,
        ),
        Command::Sig { symbol } => commands::sig::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            lang_filter,
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
            use_cache,
        ),
        Command::Tree { path } => commands::tree::run(
            path.as_deref(),
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            args.depth,
            use_cache,
        ),
        Command::Refs { symbol } => commands::refs::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            args.context,
            use_cache,
        ),
        Command::Deps { symbol } => commands::deps::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            lang_filter,
            use_cache,
        ),
        Command::Callers { symbol } => commands::callers::run(
            &symbol,
            args.project.as_deref(),
            args.scope.as_deref(),
            mode,
            pretty,
            args.context,
            use_cache,
        ),
        Command::Cache { action } => match action {
            CacheAction::Clear => match codequery_index::clear_all_caches() {
                Ok(true) => {
                    eprintln!("cache cleared");
                    Ok(ExitCode::Success)
                }
                Ok(false) => {
                    eprintln!("no cache to clear");
                    Ok(ExitCode::Success)
                }
                Err(e) => Err(anyhow::anyhow!("failed to clear cache: {e}")),
            },
        },
    }
}
