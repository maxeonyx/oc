use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

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
        }) => run_alias(&config, name, dir, opencode_args)?,
        Some(Command::Unalias { name }) => run_unalias(&config, &name)?,
        Some(Command::DumpRuntimeConfig) => config.write_debug_dump(),
        None => {}
    }

    Ok(())
}

fn run_alias(
    config: &RuntimeConfig,
    name: String,
    dir: Option<PathBuf>,
    opencode_args: Vec<String>,
) -> Result<()> {
    let directory = resolve_alias_directory(dir)?;
    let alias = NewSessionAlias::new(name, directory, opencode_args)?;
    let mut store = open_session_store(config)?;

    store
        .save_alias(alias)
        .context("failed to save session alias")?;

    Ok(())
}

fn run_unalias(config: &RuntimeConfig, name: &str) -> Result<()> {
    let mut store = open_session_store(config)?;

    store
        .remove_alias(name)
        .with_context(|| format!("failed to remove session alias '{name}'"))?;

    Ok(())
}

fn resolve_alias_directory(dir: Option<PathBuf>) -> Result<PathBuf> {
    match dir {
        Some(path) => Ok(path),
        None => env::current_dir().context("failed to determine current working directory"),
    }
}

fn open_session_store(config: &RuntimeConfig) -> Result<SessionStore> {
    SessionStore::open(config.session_db_path())
}
