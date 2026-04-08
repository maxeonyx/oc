use anyhow::{Context, Result};

use crate::cli::Cli;
use crate::commands;
use crate::config::RuntimeConfig;

pub fn run(cli: Cli) -> Result<()> {
    let config = RuntimeConfig::from_env().context("failed to resolve runtime configuration")?;
    let action = cli.requested_action();

    commands::run(&config, action)
}
