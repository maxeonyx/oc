use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::cli::{Cli, Command};
use crate::config::RuntimeConfig;
use crate::session::{NewSessionAlias, SavedSession, SessionListEntry, SessionRef, SessionStatus};
use crate::storage::SessionStore;
use crate::tmux::{ManagedTmuxSession, Tmux};

pub fn run(config: &RuntimeConfig, cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::New {
            name,
            dir,
            opencode_args,
        }) => run_new(config, name, dir, opencode_args),
        Some(Command::Alias {
            name,
            dir,
            opencode_args,
        }) => run_alias(config, name, dir, opencode_args),
        Some(Command::Unalias { name }) => run_unalias(config, &name),
        Some(Command::Rm { target }) => run_rm(config, &target),
        Some(Command::Stop { target }) => run_stop(config, &target),
        Some(Command::DumpSessionList) => run_dump_session_list(config),
        Some(Command::DumpRuntimeConfig) => {
            config.write_debug_dump();
            Ok(())
        }
        None => match cli.target {
            Some(target) => run_attach_target(config, &target),
            None => run_default(config),
        },
    }
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

    if let Err(error) = tmux.launch_opencode_session(
        &tmux_session_name,
        &saved_session.directory,
        &saved_session.opencode_args,
    ) {
        rollback_saved_alias(&mut store, &saved_session.name, error)?;
    }

    attach_to_saved_session(config, &saved_session)
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
        .with_context(|| format!("failed to remove session alias '{name}'"))
}

fn run_rm(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (mut store, saved_session) = open_store_and_load_saved_session(config, target)?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    tmux.kill_session_if_exists(&tmux_session_name)
        .with_context(|| {
            format!(
                "failed to remove tmux session for alias '{}'",
                saved_session.name
            )
        })?;

    store.remove_alias(&saved_session.name).with_context(|| {
        format!(
            "failed to remove saved alias '{}' after tmux cleanup",
            saved_session.name
        )
    })
}

fn run_stop(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (_, saved_session) = open_store_and_load_saved_session(config, target)?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    tmux.graceful_stop(&tmux_session_name).with_context(|| {
        format!(
            "failed to stop running session alias '{}'",
            saved_session.name
        )
    })
}

fn run_attach_target(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (_, saved_session) = open_store_and_load_saved_session(config, target)?;
    attach_to_saved_session(config, &saved_session)
}

fn run_default(config: &RuntimeConfig) -> Result<()> {
    let current_directory =
        env::current_dir().context("failed to determine current working directory")?;
    let store = open_session_store(config)?;
    let matching_sessions = store
        .list_saved_sessions()?
        .into_iter()
        .filter(|saved_session| saved_session.directory == current_directory)
        .collect::<Vec<_>>();

    match matching_sessions.as_slice() {
        [saved_session] => attach_to_saved_session(config, saved_session),
        _ => Ok(()),
    }
}

fn run_dump_session_list(config: &RuntimeConfig) -> Result<()> {
    for session in list_sessions(config)? {
        println!("{}", session.debug_dump_line());
    }

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

fn open_store_and_load_saved_session(
    config: &RuntimeConfig,
    target: &str,
) -> Result<(SessionStore, SavedSession)> {
    let store = open_session_store(config)?;
    let saved_session = load_saved_session(&store, target)?;

    Ok((store, saved_session))
}

fn load_saved_session(store: &SessionStore, target: &str) -> Result<SavedSession> {
    let session_ref = SessionRef::parse(target)?;

    store
        .resolve_session_ref(&session_ref)
        .with_context(|| format!("failed to resolve session reference '{target}'"))
}

fn attach_to_saved_session(config: &RuntimeConfig, saved_session: &SavedSession) -> Result<()> {
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    if !tmux.session_exists(&tmux_session_name)? {
        tmux.launch_opencode_session(
            &tmux_session_name,
            &saved_session.directory,
            &saved_session.opencode_args,
        )
        .with_context(|| format!("failed to launch session alias '{}'", saved_session.name))?;
    }

    tmux.attach_session(&tmux_session_name)
        .with_context(|| format!("failed to attach to session alias '{}'", saved_session.name))
}

fn list_sessions(config: &RuntimeConfig) -> Result<Vec<SessionListEntry>> {
    let store = open_session_store(config)?;
    let saved_sessions = store.list_saved_sessions()?;
    let tmux = open_tmux(config);
    let tmux_by_session_name = tmux
        .list_managed_sessions()?
        .into_iter()
        .map(|tmux_session| (tmux_session.session_name.clone(), tmux_session))
        .collect::<HashMap<_, _>>();

    Ok(saved_sessions
        .into_iter()
        .map(|saved_session| merge_saved_session(saved_session, &tmux, &tmux_by_session_name))
        .collect())
}

fn merge_saved_session(
    saved_session: SavedSession,
    tmux: &Tmux,
    tmux_by_session_name: &HashMap<String, ManagedTmuxSession>,
) -> SessionListEntry {
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);
    let status = match tmux_by_session_name.get(&tmux_session_name) {
        Some(tmux_session) if tmux_session.attached_count > 0 => SessionStatus::RunningAttached,
        Some(_) => SessionStatus::RunningDetached,
        None => SessionStatus::Saved,
    };

    SessionListEntry {
        saved_session,
        status,
    }
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
