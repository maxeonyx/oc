use anyhow::{Context, Result};

use crate::cli::{Cli, RequestedAction};
use crate::commands;
use crate::config::RuntimeConfig;
use crate::service::SessionService;

pub fn run(cli: Cli) -> Result<()> {
    let action = cli.requested_action();

    if let RequestedAction::Completion { shell } = action {
        crate::cli::print_completion(shell);
        return Ok(());
    }

    let config = RuntimeConfig::from_env().context("failed to resolve runtime configuration")?;
    let service = SessionService::new(config);

    commands::run(&service, action)
}
