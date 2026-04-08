use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::cli::{Cli, Command};
use crate::config::RuntimeConfig;
use crate::session::{NewSessionAlias, SessionRef};
use crate::storage::SessionStore;
use crate::tmux::Tmux;

pub fn run(cli: Cli) -> Result<()> {
    let config = RuntimeConfig::from_env().context("failed to resolve runtime configuration")?;

    match cli.command {
        Some(Command::New {
            name,
            dir,
            opencode_args,
        }) => run_new(&config, name, dir, opencode_args)?,
        Some(Command::Alias {
            name,
            dir,
            opencode_args,
        }) => run_alias(&config, name, dir, opencode_args)?,
        Some(Command::Unalias { name }) => run_unalias(&config, &name)?,
        Some(Command::Rm { target }) => run_rm(&config, &target)?,
        Some(Command::Stop { target }) => run_stop(&config, &target)?,
        Some(Command::DumpRuntimeConfig) => config.write_debug_dump(),
        None => {}
    }

    Ok(())
}

fn run_new(
    config: &RuntimeConfig,
    name: String,
    dir: Option<PathBuf>,
    opencode_args: Vec<String>,
) -> Result<()> {
    let directory = resolve_new_directory(dir)?;
    let alias = NewSessionAlias::new(name, directory, opencode_args)?;
    let tmux = open_tmux(config);
    let mut store = open_session_store(config)?;
    let saved_session = store
        .save_alias(alias)
        .context("failed to save session alias")?;
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    if let Err(error) = tmux.create_detached_session(
        &tmux_session_name,
        &saved_session.directory,
        &saved_session.opencode_args,
    ) {
        rollback_saved_alias(&mut store, &saved_session.name, error)?;
    }

    tmux.attach_session(&tmux_session_name)?;

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

fn run_rm(config: &RuntimeConfig, target: &str) -> Result<()> {
    let session_ref = SessionRef::parse(target)?;
    let mut store = open_session_store(config)?;
    let saved_session = store
        .resolve_session_ref(&session_ref)
        .with_context(|| format!("failed to resolve session reference '{target}'"))?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    tmux.kill_session_if_exists(&tmux_session_name)?;
    store.remove_alias(&saved_session.name)?;

    Ok(())
}

fn run_stop(config: &RuntimeConfig, target: &str) -> Result<()> {
    let session_ref = SessionRef::parse(target)?;
    let store = open_session_store(config)?;
    let saved_session = store
        .resolve_session_ref(&session_ref)
        .with_context(|| format!("failed to resolve session reference '{target}'"))?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    if !tmux.session_exists(&tmux_session_name)? {
        anyhow::bail!("Session alias '{}' is not running", saved_session.name);
    }

    tmux.graceful_stop(&tmux_session_name)
        .with_context(|| format!("failed to stop session alias '{}'", saved_session.name))?;

    Ok(())
}

fn resolve_alias_directory(dir: Option<PathBuf>) -> Result<PathBuf> {
    match dir {
        Some(path) => Ok(path),
        None => env::current_dir().context("failed to determine current working directory"),
    }
}

fn resolve_new_directory(dir: Option<PathBuf>) -> Result<PathBuf> {
    let directory = resolve_alias_directory(dir)?;
    let metadata = fs::metadata(&directory)
        .with_context(|| format!("Directory {} does not exist", directory.display()))?;

    if !metadata.is_dir() {
        anyhow::bail!("Path {} is not a directory", directory.display());
    }

    Ok(directory)
}

fn open_session_store(config: &RuntimeConfig) -> Result<SessionStore> {
    SessionStore::open(config.session_db_path())
}

fn open_tmux(config: &RuntimeConfig) -> Tmux {
    Tmux::new(config.tmux_prefix())
}

fn rollback_saved_alias(
    store: &mut SessionStore,
    alias_name: &str,
    launch_error: anyhow::Error,
) -> Result<()> {
    match store.remove_alias(alias_name) {
        Ok(()) => Err(launch_error).with_context(|| {
            format!("failed to launch tmux session for saved alias '{alias_name}'")
        }),
        Err(rollback_error) => Err(rollback_error).with_context(|| {
            format!(
                "failed to roll back saved alias '{alias_name}' after tmux launch failure: {launch_error:#}"
            )
        }),
    }
}
