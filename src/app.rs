use anyhow::{Context, Result};
use std::env;

use crate::cli::{Cli, Command};
use crate::config::RuntimeConfig;
use crate::session::NewSessionAlias;
use crate::storage::SessionStore;

pub fn run(cli: Cli) -> Result<()> {
    let config = RuntimeConfig::from_env().context("failed to resolve runtime configuration")?;

    match cli.command {
        Some(Command::Alias {
            name,
            dir,
            opencode_args,
        }) => {
            let directory = match dir {
                Some(path) => path,
                None => {
                    env::current_dir().context("failed to determine current working directory")?
                }
            };
            let alias = NewSessionAlias::new(name, directory, opencode_args)?;
            let mut store = SessionStore::open(config.session_db_path())?;
            store.save_alias(alias)?;
        }
        Some(Command::Unalias { name }) => {
            let mut store = SessionStore::open(config.session_db_path())?;
            store.remove_alias(&name)?;
        }
        Some(Command::DumpRuntimeConfig) => config.write_debug_dump(),
        None => {}
    }

    Ok(())
}
