use anyhow::{Context, Result};

use crate::cli::Cli;
use crate::commands;
use crate::config::RuntimeConfig;
use crate::service::SessionService;

pub fn run(cli: Cli) -> Result<()> {
    let config = RuntimeConfig::from_env().context("failed to resolve runtime configuration")?;
    let action = cli.requested_action();
    let service = SessionService::new(config);

    commands::run(&service, action)
}
