mod app;
mod cli;
mod commands;
mod config;
mod session;
mod session_list;
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
