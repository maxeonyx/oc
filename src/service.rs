use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::RuntimeConfig;
use crate::opencode_db::{OpenCodeDb, RootSessionIdLookup};
use crate::session::{NewSessionAlias, SavedSession, SessionListEntry, SessionRef};
use crate::session_list::merge_saved_and_runtime_sessions_with_prefix;
use crate::storage::SessionStore;
use crate::tmux::Tmux;

#[derive(Debug, Clone)]
pub struct SessionService {
    config: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub imported: usize,
    pub skipped: usize,
    pub conflicts: Vec<String>,
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
        let mut store = self.open_session_store()?;
        self.catch_up_missing_session_ids(&mut store)?;
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
        let mut store = self.open_session_store()?;
        self.activate_saved_session(&tmux, &mut store, saved_session)
    }

    pub fn stop_session(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        let tmux = self.open_tmux();
        let launch = SessionLaunch::for_saved_session(&tmux, &saved_session);

        tmux.graceful_stop(&launch.tmux_session_name)
            .with_context(|| format!("failed to stop running session '{}'", saved_session.name))
    }

    pub fn remove_session(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        let tmux = self.open_tmux();
        let launch = SessionLaunch::for_saved_session(&tmux, &saved_session);

        tmux.kill_session_if_exists(&launch.tmux_session_name)
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

    pub fn restart_session(&self, target: &str) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        saved_session.opencode_session_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Session '{}' cannot be restarted because it has no saved OpenCode session ID",
                saved_session.name
            )
        })?;

        let tmux = self.open_tmux();
        let launch = SessionLaunch::for_saved_session(&tmux, &saved_session);

        tmux.restart_session(
            &launch.tmux_session_name,
            &launch.directory,
            &launch.opencode_args,
        )
        .with_context(|| format!("failed to restart session '{}'", saved_session.name))
    }

    pub fn move_session(&self, target: &str, new_dir: PathBuf) -> Result<()> {
        let saved_session = self.resolve_session_ref(target)?;
        let new_directory = resolve_new_directory(Some(new_dir))?;
        let tmux = self.open_tmux();
        let launch = SessionLaunch::for_saved_session(&tmux, &saved_session);

        if tmux.session_exists(&launch.tmux_session_name)? {
            anyhow::bail!(
                "Session '{}' must be stopped before moving its directory",
                saved_session.name
            );
        }

        let mut store = self.open_session_store()?;
        store
            .update_directory(&saved_session.name, &new_directory)
            .with_context(|| format!("failed to move session '{}'", saved_session.name))?;

        let updated_session = self.resolve_session_ref(&saved_session.name)?;
        self.activate_session(&updated_session)
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

    pub fn migrate_legacy_aliases(&self) -> Result<MigrationReport> {
        let legacy_entries = read_legacy_aliases_file(self.config.legacy_aliases_path())?;
        let mut store = self.open_session_store()?;
        let mut report = MigrationReport {
            imported: 0,
            skipped: 0,
            conflicts: Vec::new(),
        };

        for alias in legacy_entries {
            match store.save_imported_alias(alias.clone()) {
                Ok(Some(_)) => report.imported += 1,
                Ok(None) => report.skipped += 1,
                Err(_) => report.conflicts.push(alias.name),
            }
        }

        Ok(report)
    }

    fn open_session_store(&self) -> Result<SessionStore> {
        SessionStore::open(self.config.session_db_path())
    }

    fn open_tmux(&self) -> Tmux {
        Tmux::new(self.config.tmux_prefix())
    }

    fn open_opencode_db(&self) -> OpenCodeDb {
        OpenCodeDb::new(self.config.opencode_db_path())
    }

    fn activate_new_saved_session(
        &self,
        tmux: &Tmux,
        store: &mut SessionStore,
        saved_session: SavedSession,
    ) -> Result<()> {
        let launch = SessionLaunch::for_saved_session(tmux, &saved_session);
        let before_launch_ids = self.root_session_ids_before_launch(&saved_session)?;

        if let Err(error) = self.ensure_tmux_session_running(tmux, &launch) {
            rollback_saved_session(store, &saved_session.name, error)?;
        }

        self.attach_to_session(tmux, &launch)?;
        self.capture_session_id_after_attach(store, &saved_session, before_launch_ids)
    }

    fn activate_saved_session(
        &self,
        tmux: &Tmux,
        store: &mut SessionStore,
        saved_session: &SavedSession,
    ) -> Result<()> {
        let launch = SessionLaunch::for_saved_session(tmux, saved_session);
        let before_launch_ids = self.root_session_ids_before_launch(saved_session)?;
        let launched = self.ensure_tmux_session_running(tmux, &launch)?;
        self.attach_to_session(tmux, &launch)?;

        if launched {
            self.capture_session_id_after_attach(store, saved_session, before_launch_ids)?;
        }

        Ok(())
    }

    fn ensure_tmux_session_running(&self, tmux: &Tmux, launch: &SessionLaunch) -> Result<bool> {
        if !tmux.session_exists(&launch.tmux_session_name)? {
            tmux.launch_opencode_session(
                &launch.tmux_session_name,
                &launch.directory,
                &launch.opencode_args,
            )
            .with_context(|| format!("failed to launch session '{}'", launch.session_name))?;

            return Ok(true);
        }

        Ok(false)
    }

    fn attach_to_session(&self, tmux: &Tmux, launch: &SessionLaunch) -> Result<()> {
        tmux.attach_session(&launch.tmux_session_name)
            .with_context(|| format!("failed to attach to session '{}'", launch.session_name))
    }

    fn root_session_ids_before_launch(
        &self,
        saved_session: &SavedSession,
    ) -> Result<Option<BTreeSet<String>>> {
        if saved_session.opencode_session_id.is_some() {
            return Ok(None);
        }

        match self
            .open_opencode_db()
            .root_session_ids_for_directory(&saved_session.directory)?
        {
            RootSessionIdLookup::Available(ids) => Ok(Some(ids)),
            RootSessionIdLookup::Unavailable => Ok(None),
        }
    }

    fn capture_session_id_after_attach(
        &self,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        before_launch_ids: Option<BTreeSet<String>>,
    ) -> Result<()> {
        let Some(before_launch_ids) = before_launch_ids else {
            return Ok(());
        };

        let RootSessionIdLookup::Available(after_launch_ids) = self
            .open_opencode_db()
            .root_session_ids_for_directory(&saved_session.directory)?
        else {
            return Ok(());
        };

        let new_ids = after_launch_ids
            .difference(&before_launch_ids)
            .cloned()
            .collect::<Vec<_>>();

        if let [captured_id] = new_ids.as_slice() {
            store
                .update_opencode_session_id(&saved_session.name, Some(captured_id))
                .with_context(|| {
                    format!(
                        "failed to persist captured OpenCode session ID for session '{}'",
                        saved_session.name
                    )
                })?;
        }

        Ok(())
    }

    fn catch_up_missing_session_ids(&self, store: &mut SessionStore) -> Result<()> {
        let saved_sessions = store.list_saved_sessions()?;
        let opencode_db = self.open_opencode_db();

        for saved_session in saved_sessions {
            if saved_session.opencode_session_id.is_some() {
                continue;
            }

            let RootSessionIdLookup::Available(ids) =
                opencode_db.root_session_ids_for_directory(&saved_session.directory)?
            else {
                continue;
            };

            if let [captured_id] = ids.iter().collect::<Vec<_>>().as_slice() {
                store
                    .update_opencode_session_id(&saved_session.name, Some(captured_id.as_str()))
                    .with_context(|| {
                        format!(
                            "failed to catch up OpenCode session ID for session '{}'",
                            saved_session.name
                        )
                    })?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SessionLaunch {
    session_name: String,
    tmux_session_name: String,
    directory: PathBuf,
    opencode_args: Vec<String>,
}

impl SessionLaunch {
    fn for_saved_session(tmux: &Tmux, saved_session: &SavedSession) -> Self {
        Self {
            session_name: saved_session.name.clone(),
            tmux_session_name: tmux.managed_session_name(&saved_session.name),
            directory: saved_session.directory.clone(),
            opencode_args: launch_opencode_args(saved_session),
        }
    }
}

fn read_legacy_aliases_file(path: &Path) -> Result<Vec<NewSessionAlias>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read legacy aliases file {}", path.display()))?;

    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_legacy_alias_line)
        .collect()
}

fn parse_legacy_alias_line(line: &str) -> Result<NewSessionAlias> {
    let (name, directory) = line
        .split_once('\t')
        .ok_or_else(|| anyhow::anyhow!("Legacy alias line must contain a tab separator: {line}"))?;

    NewSessionAlias::new(String::from(name), PathBuf::from(directory), Vec::new())
}

fn launch_opencode_args(saved_session: &SavedSession) -> Vec<String> {
    let mut args = saved_session.opencode_args.clone();
    if let Some(session_id) = &saved_session.opencode_session_id {
        args.splice(0..0, [String::from("--session"), session_id.clone()]);
    }

    args
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
