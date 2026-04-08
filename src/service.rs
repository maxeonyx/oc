use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::RuntimeConfig;
use crate::session::{NewSessionAlias, SavedSession, SessionListEntry, SessionRef};
use crate::session_list::merge_saved_and_runtime_sessions_with_prefix;
use crate::storage::SessionStore;
use crate::tmux::Tmux;

#[derive(Debug, Clone)]
pub struct SessionService {
    config: RuntimeConfig,
}

impl SessionService {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn create_session(
        &self,
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
    ) -> Result<()> {
        let directory = resolve_new_directory(dir)?;
        let alias = NewSessionAlias::new(name, directory, opencode_args)?;
        let tmux = self.open_tmux();
        let mut store = self.open_session_store()?;
        let saved_session = store.save_alias(alias).context("failed to save session")?;

        self.activate_new_saved_session(&tmux, &mut store, saved_session)
    }

    pub fn save_alias(
        &self,
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
    ) -> Result<()> {
        let directory = resolve_alias_directory(dir)?;
        let alias = NewSessionAlias::new(name, directory, opencode_args)?;
        let mut store = self.open_session_store()?;

        store.save_alias(alias).context("failed to save session")?;

        Ok(())
    }

    pub fn remove_alias(&self, name: &str) -> Result<()> {
        let mut store = self.open_session_store()?;

        store
            .remove_alias(name)
            .with_context(|| format!("failed to remove session '{name}'"))
    }

    pub fn list_dashboard_sessions(&self) -> Result<Vec<SessionListEntry>> {
        let store = self.open_session_store()?;
        let saved_sessions = store.list_saved_sessions()?;
        let tmux = self.open_tmux();
        let runtimes = tmux.list_managed_sessions()?;

        merge_saved_and_runtime_sessions_with_prefix(
            saved_sessions,
            runtimes,
            tmux.managed_session_prefix(),
        )
    }

    pub fn resolve_session_ref(&self, target: &str) -> Result<SavedSession> {
        let store = self.open_session_store()?;
        let session_ref = SessionRef::parse(target)?;

        store
            .resolve_session_ref(&session_ref)
            .with_context(|| format!("failed to resolve session '{target}'"))
    }

    pub fn activate_target(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        self.activate_session(&saved_session)
    }

    pub fn activate_session(&self, saved_session: &SavedSession) -> Result<()> {
        let tmux = self.open_tmux();
        self.ensure_tmux_session_running(&tmux, saved_session)?;
        self.attach_to_session(&tmux, saved_session)
    }

    pub fn stop_session(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        let tmux = self.open_tmux();
        let tmux_session_name = tmux.managed_session_name(&saved_session.name);

        tmux.graceful_stop(&tmux_session_name)
            .with_context(|| format!("failed to stop running session '{}'", saved_session.name))
    }

    pub fn remove_session(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        let tmux = self.open_tmux();
        let tmux_session_name = tmux.managed_session_name(&saved_session.name);

        tmux.kill_session_if_exists(&tmux_session_name)
            .with_context(|| {
                format!(
                    "failed to remove tmux session for session '{}'",
                    saved_session.name
                )
            })?;

        let mut store = self.open_session_store()?;
        store.remove_alias(&saved_session.name).with_context(|| {
            format!(
                "failed to remove saved session '{}' after tmux cleanup",
                saved_session.name
            )
        })
    }

    pub fn auto_attach_directory_match(&self) -> Result<bool> {
        match self.current_directory_matches()?.as_slice() {
            [saved_session] => {
                self.activate_session(saved_session)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn current_directory_matches(&self) -> Result<Vec<SavedSession>> {
        let current_directory =
            env::current_dir().context("failed to determine current working directory")?;
        let store = self.open_session_store()?;
        find_saved_sessions_in_directory(&store, &current_directory)
    }

    fn open_session_store(&self) -> Result<SessionStore> {
        SessionStore::open(self.config.session_db_path())
    }

    fn open_tmux(&self) -> Tmux {
        Tmux::new(self.config.tmux_prefix())
    }

    fn activate_new_saved_session(
        &self,
        tmux: &Tmux,
        store: &mut SessionStore,
        saved_session: SavedSession,
    ) -> Result<()> {
        if let Err(error) = self.ensure_tmux_session_running(tmux, &saved_session) {
            rollback_saved_session(store, &saved_session.name, error)?;
        }

        self.attach_to_session(tmux, &saved_session)
    }

    fn ensure_tmux_session_running(&self, tmux: &Tmux, saved_session: &SavedSession) -> Result<()> {
        let tmux_session_name =
            saved_session.managed_tmux_session_name(tmux.managed_session_prefix());

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

    fn attach_to_session(&self, tmux: &Tmux, saved_session: &SavedSession) -> Result<()> {
        let tmux_session_name =
            saved_session.managed_tmux_session_name(tmux.managed_session_prefix());
        tmux.attach_session(&tmux_session_name)
            .with_context(|| format!("failed to attach to session '{}'", saved_session.name))
    }
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
    directory: &Path,
) -> Result<Vec<SavedSession>> {
    Ok(store
        .list_saved_sessions()?
        .into_iter()
        .filter(|saved_session| saved_session.directory == directory)
        .collect())
}
