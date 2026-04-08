use clap::Parser;
use oc::{app, cli};
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
