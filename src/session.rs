use anyhow::{bail, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSession {
    pub id: i64,
    pub name: String,
    pub directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub opencode_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSessionAlias {
    pub name: String,
    pub directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub opencode_args: Vec<String>,
}

impl NewSessionAlias {
    pub fn new(name: String, directory: PathBuf, opencode_args: Vec<String>) -> Result<Self> {
        validate_session_name(&name)?;

        Ok(Self {
            name,
            directory,
            opencode_session_id: None,
            opencode_args,
        })
    }

    pub fn with_opencode_session_id(mut self, opencode_session_id: Option<String>) -> Self {
        self.opencode_session_id = opencode_session_id;
        self
    }
}

impl SavedSession {
    pub fn managed_tmux_session_name(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRef {
    NumericId(i64),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    RunningAttached,
    RunningDetached,
    Saved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSessionRuntime {
    pub tmux_session_name: String,
    pub attached_count: usize,
    pub pane_pid: Option<u32>,
    pub memory_bytes: Option<u64>,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunningAttached => "running_attached",
            Self::RunningDetached => "running_detached",
            Self::Saved => "saved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntry {
    pub saved_session: SavedSession,
    pub status: SessionStatus,
    pub runtime: Option<ManagedSessionRuntime>,
}

impl SessionListEntry {
    pub fn from_saved_session(
        saved_session: SavedSession,
        runtime: Option<&ManagedSessionRuntime>,
    ) -> Self {
        let status = match runtime {
            Some(runtime) if runtime.attached_count > 0 => SessionStatus::RunningAttached,
            Some(_) => SessionStatus::RunningDetached,
            None => SessionStatus::Saved,
        };

        Self {
            saved_session,
            status,
            runtime: runtime.cloned(),
        }
    }

    pub fn debug_dump_line(&self) -> String {
        format!(
            "id={} name={} dir={} status={}",
            self.saved_session.id,
            self.saved_session.name,
            self.saved_session.directory.display(),
            self.status.as_str()
        )
    }

    pub fn runtime_memory_bytes(&self) -> Option<u64> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.memory_bytes)
    }
}

impl SessionRef {
    pub fn parse(input: &str) -> Result<Self> {
        match input.parse::<i64>() {
            Ok(id) if id > 0 => Ok(Self::NumericId(id)),
            Ok(_) => bail!("Session ID '{input}' must be a positive integer"),
            Err(_) => Ok(Self::Name(String::from(input))),
        }
    }
}

fn validate_session_name(name: &str) -> Result<()> {
    if name.parse::<u64>().is_ok() {
        bail!(
            "Session name '{name}' cannot be a plain number because numeric IDs and names must stay unambiguous"
        );
    }

    Ok(())
}
