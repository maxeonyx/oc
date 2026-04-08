use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::cli::RequestedAction;
use crate::config::RuntimeConfig;
use crate::session::{NewSessionAlias, SavedSession, SessionListEntry, SessionRef};
use crate::session_list::merge_saved_and_runtime_sessions_with_prefix;
use crate::storage::SessionStore;
use crate::tmux::Tmux;

pub fn run(config: &RuntimeConfig, action: RequestedAction) -> Result<()> {
    match action {
        RequestedAction::New {
            name,
            dir,
            opencode_args,
        } => run_new(config, name, dir, opencode_args),
        RequestedAction::Alias {
            name,
            dir,
            opencode_args,
        } => run_alias(config, name, dir, opencode_args),
        RequestedAction::Unalias { name } => run_unalias(config, &name),
        RequestedAction::Rm { target } => run_rm(config, &target),
        RequestedAction::Stop { target } => run_stop(config, &target),
        RequestedAction::AttachTarget { target } => run_attach_target(config, &target),
        RequestedAction::Default => run_default(config),
        RequestedAction::DumpSessionList => run_dump_session_list(config),
        RequestedAction::DumpRuntimeConfig => {
            config.write_debug_dump();
            Ok(())
        }
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
    let saved_session = store.save_alias(alias).context("failed to save session")?;

    activate_new_saved_session(&tmux, &mut store, saved_session)
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

    store.save_alias(alias).context("failed to save session")?;

    Ok(())
}

fn run_unalias(config: &RuntimeConfig, name: &str) -> Result<()> {
    let mut store = open_session_store(config)?;

    store
        .remove_alias(name)
        .with_context(|| format!("failed to remove session '{name}'"))
}

fn run_rm(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (mut store, saved_session) = open_store_and_load_saved_session(config, target)?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    tmux.kill_session_if_exists(&tmux_session_name)
        .with_context(|| {
            format!(
                "failed to remove tmux session for session '{}'",
                saved_session.name
            )
        })?;

    store.remove_alias(&saved_session.name).with_context(|| {
        format!(
            "failed to remove saved session '{}' after tmux cleanup",
            saved_session.name
        )
    })
}

fn run_stop(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (_, saved_session) = open_store_and_load_saved_session(config, target)?;
    let tmux = open_tmux(config);
    let tmux_session_name = tmux.managed_session_name(&saved_session.name);

    tmux.graceful_stop(&tmux_session_name)
        .with_context(|| format!("failed to stop running session '{}'", saved_session.name))
}

fn run_attach_target(config: &RuntimeConfig, target: &str) -> Result<()> {
    let (_, saved_session) = open_store_and_load_saved_session(config, target)?;
    activate_saved_session(config, &saved_session)
}

fn run_default(config: &RuntimeConfig) -> Result<()> {
    let current_directory =
        env::current_dir().context("failed to determine current working directory")?;
    let store = open_session_store(config)?;
    let matching_sessions = find_saved_sessions_in_directory(&store, &current_directory)?;

    match matching_sessions.as_slice() {
        [saved_session] => activate_saved_session(config, saved_session),
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
        .with_context(|| format!("directory '{}' does not exist", directory.display()))?;

    if !metadata.is_dir() {
        anyhow::bail!("path '{}' is not a directory", directory.display());
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
        .with_context(|| format!("failed to resolve session '{target}'"))
}

fn activate_saved_session(config: &RuntimeConfig, saved_session: &SavedSession) -> Result<()> {
    let tmux = open_tmux(config);
    ensure_tmux_session_running(&tmux, saved_session)?;
    attach_to_session(&tmux, saved_session)
}

fn activate_new_saved_session(
    tmux: &Tmux,
    store: &mut SessionStore,
    saved_session: SavedSession,
) -> Result<()> {
    if let Err(error) = ensure_tmux_session_running(tmux, &saved_session) {
        rollback_saved_session(store, &saved_session.name, error)?;
    }

    attach_to_session(tmux, &saved_session)
}

fn ensure_tmux_session_running(tmux: &Tmux, saved_session: &SavedSession) -> Result<()> {
    let tmux_session_name = saved_session.managed_tmux_session_name(tmux.managed_session_prefix());

    if !tmux.session_exists(&tmux_session_name)? {
        tmux.launch_opencode_session(
            &tmux_session_name,
            &saved_session.directory,
            &saved_session.opencode_args,
        )
        .with_context(|| format!("failed to launch session '{}'", saved_session.name))?;
    }

    Ok(())
}

fn attach_to_session(tmux: &Tmux, saved_session: &SavedSession) -> Result<()> {
    let tmux_session_name = saved_session.managed_tmux_session_name(tmux.managed_session_prefix());
    tmux.attach_session(&tmux_session_name)
        .with_context(|| format!("failed to attach to session '{}'", saved_session.name))
}

fn list_sessions(config: &RuntimeConfig) -> Result<Vec<SessionListEntry>> {
    let store = open_session_store(config)?;
    let saved_sessions = store.list_saved_sessions()?;
    let tmux = open_tmux(config);
    let runtimes = tmux.list_managed_sessions()?;

    merge_saved_and_runtime_sessions_with_prefix(
        saved_sessions,
        runtimes,
        tmux.managed_session_prefix(),
    )
}

fn open_session_store(config: &RuntimeConfig) -> Result<SessionStore> {
    SessionStore::open(config.session_db_path())
}

fn open_tmux(config: &RuntimeConfig) -> Tmux {
    Tmux::new(config.tmux_prefix())
}

fn rollback_saved_session(
    store: &mut SessionStore,
    session_name: &str,
    launch_error: anyhow::Error,
) -> Result<()> {
    match store.remove_alias(session_name) {
        Ok(()) => Err(launch_error).with_context(|| {
            format!("failed to launch tmux session for saved session '{session_name}'")
        }),
        Err(rollback_error) => Err(rollback_error).with_context(|| {
            format!(
                "failed to roll back saved session '{session_name}' after tmux launch failure: {launch_error:#}"
            )
        }),
    }
}

fn find_saved_sessions_in_directory(
    store: &SessionStore,
    directory: &std::path::Path,
) -> Result<Vec<SavedSession>> {
    Ok(store
        .list_saved_sessions()?
        .into_iter()
        .filter(|saved_session| saved_session.directory == directory)
        .collect())
}
