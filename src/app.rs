use anyhow::{Context, Result};

use crate::cli::{Cli, Command};
use crate::config::RuntimeConfig;

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::DumpRuntimeConfig) => RuntimeConfig::from_env()
            .context("failed to resolve runtime configuration")?
            .write_debug_dump(),
        None => {}
    }

    Ok(())
}
