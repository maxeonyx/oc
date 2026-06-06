use clap::Parser;
use oc::{app, cli};
use serde_json::json;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Some(exit_code) = try_handle_version_request() {
        return exit_code;
    }

    match app::run(cli::Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn try_handle_version_request() -> Option<ExitCode> {
    let args: Vec<String> = env::args().skip(1).collect();

    if is_version_json_request(&args) {
        println!(
            "{}",
            json!({
                "package": "oc",
                "binary": "oc",
                "version": env!("CARGO_PKG_VERSION"),
            })
        );
        return Some(ExitCode::SUCCESS);
    }

    if is_version_request(&args) {
        println!("oc {}", env!("CARGO_PKG_VERSION"));
        return Some(ExitCode::SUCCESS);
    }

    None
}

fn is_version_request(args: &[String]) -> bool {
    args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V")
}

fn is_version_json_request(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--version" | "-V"))
        && args.iter().any(|arg| arg == "--json")
        && args
            .iter()
            .all(|arg| matches!(arg.as_str(), "--version" | "-V" | "--json"))
}
