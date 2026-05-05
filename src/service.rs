use anyhow::{Context, Result, anyhow};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::config::RuntimeConfig;
use crate::opencode_db::{
    OpenCodeDb, ProcessSessionLookup, ProcessSessionRowLookup, ProcessSessionTableLookup,
    RootSessionIdLookup,
};
use crate::session::{NewSessionAlias, SavedSession, SessionListEntry, SessionRef};
use crate::session_list::merge_saved_and_runtime_sessions_with_prefix;
use crate::storage::SessionStore;
use crate::tmux::Tmux;

const ATTACH_SESSION_ID_CAPTURE_POLL_INTERVAL_MS: u64 = 1_000;

enum SessionIdCaptureProbe {
    SessionId(String),
    RetryLater,
    CompatibilityFallbackAllowed,
    Stop,
}

#[derive(Debug, Default)]
struct AttachSessionIdCapture {
    latest_session_id: Option<String>,
    fallback_allowed: bool,
}

impl AttachSessionIdCapture {
    fn observe(&mut self, probe: SessionIdCaptureProbe) -> bool {
        match probe {
            SessionIdCaptureProbe::SessionId(session_id) => {
                self.latest_session_id = Some(session_id);
                false
            }
            SessionIdCaptureProbe::RetryLater => false,
            SessionIdCaptureProbe::CompatibilityFallbackAllowed => {
                self.fallback_allowed = true;
                false
            }
            SessionIdCaptureProbe::Stop => true,
        }
    }
}

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
        self.create_session_with_attach(name, dir, opencode_args, true)
    }

    pub fn create_session_headless(
        &self,
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
    ) -> Result<()> {
        self.create_session_with_attach(name, dir, opencode_args, false)
    }

    fn create_session_with_attach(
        &self,
        name: String,
        dir: Option<PathBuf>,
        opencode_args: Vec<String>,
        attach: bool,
    ) -> Result<()> {
        let directory = resolve_new_directory(dir)?;
        let alias = NewSessionAlias::new(name, directory, opencode_args)?;
        let tmux = self.open_tmux();
        let mut store = self.open_session_store()?;
        let saved_session = store.save_alias(alias).context("failed to save session")?;

        self.activate_new_saved_session(&tmux, &mut store, saved_session, attach)
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
        let tmux = self.open_tmux();
        let saved_sessions = store.list_saved_sessions()?;
        let runtimes = tmux.list_managed_sessions()?;

        merge_saved_and_runtime_sessions_with_prefix(
            saved_sessions,
            runtimes,
            tmux.managed_session_prefix(),
        )
    }

    pub fn reconcile_missing_session_ids_once(&self) -> Result<bool> {
        let tmux = self.open_tmux();
        let opencode_db = self.open_opencode_db();
        let mut store = self.open_session_store()?;

        self.reconcile_missing_session_ids_once_with(&tmux, &opencode_db, &mut store)
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
            .with_context(|| format!("failed to stop running session '{}'", saved_session.name))?;

        let mut store = self.open_session_store()?;
        self.mark_saved_session_used(&mut store, &saved_session)
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
        .with_context(|| format!("failed to restart session '{}'", saved_session.name))?;

        let mut store = self.open_session_store()?;
        self.mark_saved_session_used(&mut store, &saved_session)
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
        attach: bool,
    ) -> Result<()> {
        let launch = SessionLaunch::for_saved_session(tmux, &saved_session);
        let directory_compatibility_fallback_snapshot =
            self.prepare_directory_compatibility_fallback_snapshot(&saved_session)?;

        if let Err(error) = self.ensure_tmux_session_running(tmux, &launch) {
            rollback_saved_session(store, &saved_session.name, error)?;
        }

        if attach {
            let result = self.attach_to_session_and_capture_session_id(
                tmux,
                &launch,
                store,
                &saved_session,
                directory_compatibility_fallback_snapshot,
            );

            if result.is_ok() {
                self.mark_saved_session_used(store, &saved_session)?;
            }

            return result;
        }

        self.capture_session_id_after_launch(
            tmux,
            store,
            &saved_session,
            directory_compatibility_fallback_snapshot,
        )?;

        self.mark_saved_session_used(store, &saved_session)
    }

    fn activate_saved_session(
        &self,
        tmux: &Tmux,
        store: &mut SessionStore,
        saved_session: &SavedSession,
    ) -> Result<()> {
        let launch = SessionLaunch::for_saved_session(tmux, saved_session);
        let directory_compatibility_fallback_snapshot =
            self.prepare_directory_compatibility_fallback_snapshot(saved_session)?;
        self.ensure_tmux_session_running(tmux, &launch)?;

        if saved_session.opencode_session_id.is_none() {
            self.attach_to_session_and_capture_session_id(
                tmux,
                &launch,
                store,
                saved_session,
                directory_compatibility_fallback_snapshot,
            )?;
        } else {
            self.attach_to_session(tmux, &launch)?;
        }

        self.mark_saved_session_used(store, saved_session)
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

    fn prepare_directory_compatibility_fallback_snapshot(
        &self,
        saved_session: &SavedSession,
    ) -> Result<Option<BTreeSet<String>>> {
        if saved_session.opencode_session_id.is_some() {
            return Ok(None);
        }

        match self.open_opencode_db().process_session_table_lookup()? {
            ProcessSessionTableLookup::Available => return Ok(None),
            ProcessSessionTableLookup::Missing => {}
            ProcessSessionTableLookup::Unavailable => return Ok(None),
        }

        match self
            .open_opencode_db()
            .root_session_ids_for_directory(&saved_session.directory)?
        {
            RootSessionIdLookup::Available(ids) => Ok(Some(ids)),
            RootSessionIdLookup::Unavailable => Ok(None),
        }
    }

    fn attach_to_session_and_capture_session_id(
        &self,
        tmux: &Tmux,
        launch: &SessionLaunch,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        directory_compatibility_fallback_snapshot: Option<BTreeSet<String>>,
    ) -> Result<()> {
        let stop_flag = AtomicBool::new(false);
        let opencode_db = self.open_opencode_db();
        let capture = std::thread::scope(|scope| -> Result<AttachSessionIdCapture> {
            let capture_handle = scope.spawn(|| {
                self.poll_session_id_during_attach(
                    tmux,
                    &opencode_db,
                    &launch.tmux_session_name,
                    &stop_flag,
                )
            });

            let attach_result = self.attach_to_session(tmux, launch);
            stop_flag.store(true, Ordering::Relaxed);

            let final_probe = self.capture_session_id_probe_for_tmux_session(
                tmux,
                &opencode_db,
                &launch.tmux_session_name,
            )?;
            let mut capture = capture_handle
                .join()
                .map_err(|_| anyhow!("session ID capture thread panicked during tmux attach"))??;

            if let Some(probe) = final_probe {
                capture.observe(probe);
            }

            attach_result?;
            Ok(capture)
        })?;

        self.persist_attach_session_id_capture(
            store,
            saved_session,
            directory_compatibility_fallback_snapshot,
            capture,
        )
    }

    fn capture_session_id_after_launch(
        &self,
        tmux: &Tmux,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        directory_compatibility_fallback_snapshot: Option<BTreeSet<String>>,
    ) -> Result<()> {
        let tmux_session_name = tmux.managed_session_name(&saved_session.name);
        let opencode_db = self.open_opencode_db();
        let Some(pid) = tmux.pane_pid(&tmux_session_name)? else {
            if let Some(directory_compatibility_fallback_snapshot) =
                directory_compatibility_fallback_snapshot
            {
                self.capture_session_id_via_directory_compatibility_fallback(
                    store,
                    saved_session,
                    &directory_compatibility_fallback_snapshot,
                )?;
            }
            return Ok(());
        };

        match self.capture_session_id_probe_via_pid(&opencode_db, pid)? {
            SessionIdCaptureProbe::SessionId(session_id) => {
                self.persist_captured_session_id(store, saved_session, &session_id)
            }
            SessionIdCaptureProbe::CompatibilityFallbackAllowed => {
                let Some(directory_compatibility_fallback_snapshot) =
                    directory_compatibility_fallback_snapshot
                else {
                    return Ok(());
                };

                self.capture_session_id_via_directory_compatibility_fallback(
                    store,
                    saved_session,
                    &directory_compatibility_fallback_snapshot,
                )
            }
            SessionIdCaptureProbe::RetryLater | SessionIdCaptureProbe::Stop => Ok(()),
        }
    }

    fn poll_session_id_during_attach(
        &self,
        tmux: &Tmux,
        opencode_db: &OpenCodeDb,
        tmux_session_name: &str,
        stop_flag: &AtomicBool,
    ) -> Result<AttachSessionIdCapture> {
        let mut capture = AttachSessionIdCapture::default();

        loop {
            if let Some(pid) = tmux.pane_pid(tmux_session_name)? {
                let should_stop =
                    capture.observe(self.capture_session_id_probe_via_pid(opencode_db, pid)?);
                if should_stop {
                    return Ok(capture);
                }
            }

            if stop_flag.load(Ordering::Relaxed) {
                return Ok(capture);
            }

            std::thread::sleep(Duration::from_millis(
                ATTACH_SESSION_ID_CAPTURE_POLL_INTERVAL_MS,
            ));
        }
    }

    fn capture_session_id_probe_for_tmux_session(
        &self,
        tmux: &Tmux,
        opencode_db: &OpenCodeDb,
        tmux_session_name: &str,
    ) -> Result<Option<SessionIdCaptureProbe>> {
        let Some(pid) = tmux.pane_pid(tmux_session_name)? else {
            return Ok(None);
        };

        Ok(Some(
            self.capture_session_id_probe_via_pid(opencode_db, pid)?,
        ))
    }

    fn persist_attach_session_id_capture(
        &self,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        directory_compatibility_fallback_snapshot: Option<BTreeSet<String>>,
        capture: AttachSessionIdCapture,
    ) -> Result<()> {
        if let Some(captured_id) = capture.latest_session_id.as_deref() {
            return self.persist_captured_session_id(store, saved_session, captured_id);
        }

        if !capture.fallback_allowed {
            return Ok(());
        }

        let Some(directory_compatibility_fallback_snapshot) =
            directory_compatibility_fallback_snapshot
        else {
            return Ok(());
        };

        self.capture_session_id_via_directory_compatibility_fallback(
            store,
            saved_session,
            &directory_compatibility_fallback_snapshot,
        )
    }

    fn reconcile_missing_session_ids_once_with(
        &self,
        tmux: &Tmux,
        opencode_db: &OpenCodeDb,
        store: &mut SessionStore,
    ) -> Result<bool> {
        let saved_sessions = store.list_saved_sessions()?;
        let mut changed = false;

        for saved_session in saved_sessions {
            if saved_session.opencode_session_id.is_some() {
                continue;
            }

            let tmux_session_name =
                saved_session.managed_tmux_session_name(tmux.managed_session_prefix());
            match tmux.pane_pid(&tmux_session_name)? {
                Some(pid) => match self.capture_session_id_probe_via_pid(opencode_db, pid)? {
                    SessionIdCaptureProbe::SessionId(session_id) => {
                        self.persist_captured_session_id(store, &saved_session, &session_id)?;
                        changed = true;
                    }
                    SessionIdCaptureProbe::RetryLater
                    | SessionIdCaptureProbe::CompatibilityFallbackAllowed
                    | SessionIdCaptureProbe::Stop => {}
                },
                None => {
                    changed |= self.adopt_session_id_via_directory_if_unambiguous(
                        opencode_db,
                        store,
                        &saved_session,
                    )?;
                }
            }
        }

        Ok(changed)
    }

    fn capture_session_id_probe_via_pid(
        &self,
        opencode_db: &OpenCodeDb,
        pid: u32,
    ) -> Result<SessionIdCaptureProbe> {
        match opencode_db.process_session_id_for_pid(pid)? {
            ProcessSessionLookup::Available(ProcessSessionRowLookup::SessionId(session_id)) => {
                Ok(SessionIdCaptureProbe::SessionId(session_id))
            }
            // Only a missing table permits the old directory-based compatibility fallback.
            ProcessSessionLookup::Available(ProcessSessionRowLookup::TableMissing) => {
                Ok(SessionIdCaptureProbe::CompatibilityFallbackAllowed)
            }
            ProcessSessionLookup::Available(
                ProcessSessionRowLookup::RowMissing | ProcessSessionRowLookup::SessionIdMissing,
            ) => Ok(SessionIdCaptureProbe::RetryLater),
            ProcessSessionLookup::Available(ProcessSessionRowLookup::Stale) => {
                Ok(SessionIdCaptureProbe::Stop)
            }
            ProcessSessionLookup::Unavailable => Ok(SessionIdCaptureProbe::RetryLater),
        }
    }

    fn capture_session_id_via_directory_compatibility_fallback(
        &self,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        directory_compatibility_fallback_snapshot: &BTreeSet<String>,
    ) -> Result<()> {
        let Some(captured_id) = self.new_root_session_id_since_snapshot(
            &self.open_opencode_db(),
            &saved_session.directory,
            directory_compatibility_fallback_snapshot,
        )?
        else {
            return Ok(());
        };

        self.persist_captured_session_id(store, saved_session, &captured_id)
    }

    fn adopt_session_id_via_directory_if_unambiguous(
        &self,
        opencode_db: &OpenCodeDb,
        store: &mut SessionStore,
        saved_session: &SavedSession,
    ) -> Result<bool> {
        let Some(captured_id) =
            self.unambiguous_root_session_id_for_directory(opencode_db, &saved_session.directory)?
        else {
            return Ok(false);
        };

        self.persist_captured_session_id(store, saved_session, &captured_id)?;
        Ok(true)
    }

    fn new_root_session_id_since_snapshot(
        &self,
        opencode_db: &OpenCodeDb,
        directory: &Path,
        before_launch_ids: &BTreeSet<String>,
    ) -> Result<Option<String>> {
        let RootSessionIdLookup::Available(after_launch_ids) =
            opencode_db.root_session_ids_for_directory(directory)?
        else {
            return Ok(None);
        };

        Ok(single_session_id(
            after_launch_ids.difference(before_launch_ids).cloned(),
        ))
    }

    fn unambiguous_root_session_id_for_directory(
        &self,
        opencode_db: &OpenCodeDb,
        directory: &Path,
    ) -> Result<Option<String>> {
        let RootSessionIdLookup::Available(ids) =
            opencode_db.root_session_ids_for_directory(directory)?
        else {
            return Ok(None);
        };

        Ok(single_session_id(ids.into_iter()))
    }

    fn persist_captured_session_id(
        &self,
        store: &mut SessionStore,
        saved_session: &SavedSession,
        captured_id: &str,
    ) -> Result<()> {
        store
            .update_opencode_session_id(&saved_session.name, Some(captured_id))
            .with_context(|| {
                format!(
                    "failed to persist captured OpenCode session ID for session '{}'",
                    saved_session.name
                )
            })
    }

    fn mark_saved_session_used(
        &self,
        store: &mut SessionStore,
        saved_session: &SavedSession,
    ) -> Result<()> {
        store
            .mark_session_used_now(&saved_session.name)
            .with_context(|| {
                format!(
                    "failed to update recency for session '{}'",
                    saved_session.name
                )
            })?;

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
    let mut fields = line.splitn(3, '\t');
    let name = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("Legacy alias line must contain a name: {line}"))?;
    let directory = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("Legacy alias line must contain a tab separator: {line}"))?;
    let raw_args = fields.next().unwrap_or("");
    let (opencode_session_id, opencode_args) = parse_legacy_opencode_args(raw_args);

    NewSessionAlias::new(String::from(name), PathBuf::from(directory), opencode_args)
        .map(|alias| alias.with_opencode_session_id(opencode_session_id))
}

fn parse_legacy_opencode_args(raw_args: &str) -> (Option<String>, Vec<String>) {
    let parts = raw_args
        .split_whitespace()
        .map(String::from)
        .collect::<Vec<_>>();
    let mut opencode_session_id = None;
    let mut filtered_args = Vec::with_capacity(parts.len());
    let mut index = 0;

    while index < parts.len() {
        if parts[index] == "--session" {
            if let Some(session_id) = parts.get(index + 1) {
                opencode_session_id = Some(session_id.clone());
                index += 2;
                continue;
            }
        }

        filtered_args.push(parts[index].clone());
        index += 1;
    }

    (opencode_session_id, filtered_args)
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

fn single_session_id<I>(mut ids: I) -> Option<String>
where
    I: Iterator<Item = String>,
{
    let first = ids.next()?;
    if ids.next().is_some() {
        None
    } else {
        Some(first)
    }
}
