mod app;
mod cli;
mod config;
mod session;
mod storage;
mod tmux;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    match app::run(cli::Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
