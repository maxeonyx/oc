use std::path::PathBuf;

use crate::session::{SessionListEntry, SessionStatus};

use super::format::{abbreviate_directory, format_memory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub rows: Vec<DashboardRow>,
    pub summary: DashboardSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardView {
    pub groups: Vec<DashboardGroup>,
    pub totals: DashboardSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardGroup {
    pub title: Option<String>,
    pub sessions: Vec<DashboardRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardSummary {
    pub attached: usize,
    pub detached: usize,
    pub saved: usize,
    pub filtered_sessions: usize,
    pub filtered_running: usize,
    pub filtered_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardRow {
    pub session_id: i64,
    pub name: String,
    pub directory: String,
    pub full_directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub memory_bytes: Option<u64>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardAction {
    Attach,
    Stop,
    Remove,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionState {
    pub action: DashboardAction,
    pub enabled: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Filter,
    Command,
}

impl DashboardSnapshot {
    pub fn from_session_entries(entries: Vec<SessionListEntry>) -> Self {
        let mut summary = DashboardSummary {
            attached: 0,
            detached: 0,
            saved: 0,
            filtered_sessions: 0,
            filtered_running: 0,
            filtered_memory_bytes: 0,
        };

        let rows = entries
            .into_iter()
            .map(|entry| {
                match entry.status {
                    SessionStatus::RunningAttached => summary.attached += 1,
                    SessionStatus::RunningDetached => summary.detached += 1,
                    SessionStatus::Saved => summary.saved += 1,
                }

                DashboardRow::from_session_entry(entry)
            })
            .collect();

        Self { rows, summary }
    }
}

impl DashboardRow {
    fn from_session_entry(entry: SessionListEntry) -> Self {
        let memory_bytes = entry.runtime_memory_bytes();
        let saved_session = entry.saved_session;

        Self {
            session_id: saved_session.id,
            name: saved_session.name.clone(),
            directory: abbreviate_directory(&saved_session),
            full_directory: saved_session.directory,
            opencode_session_id: saved_session.opencode_session_id,
            memory_bytes,
            status: entry.status,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.status {
            SessionStatus::RunningAttached => "attached",
            SessionStatus::RunningDetached => "detached",
            SessionStatus::Saved => "saved",
        }
    }

    pub fn available_actions(&self) -> Vec<DashboardAction> {
        match self.status {
            SessionStatus::RunningAttached | SessionStatus::RunningDetached => {
                let mut actions = vec![
                    DashboardAction::Attach,
                    DashboardAction::Stop,
                    DashboardAction::Remove,
                ];

                if self.opencode_session_id.is_some() {
                    actions.push(DashboardAction::Restart);
                }

                actions
            }
            SessionStatus::Saved => vec![DashboardAction::Attach, DashboardAction::Remove],
        }
    }

    pub fn memory_label(&self) -> String {
        match self.memory_bytes {
            Some(bytes) => format_memory(bytes),
            None => String::from("-"),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::RunningAttached | SessionStatus::RunningDetached
        )
    }
}

impl DashboardAction {
    pub const ALL: [Self; 4] = [Self::Attach, Self::Stop, Self::Remove, Self::Restart];
}

impl DashboardView {
    pub fn sessions(&self) -> impl Iterator<Item = &DashboardRow> {
        self.groups.iter().flat_map(|group| group.sessions.iter())
    }
}
