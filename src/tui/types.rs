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
pub struct CursorPosition {
    pub x: u16,
    pub y: u16,
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

        let mut rows = entries
            .into_iter()
            .map(|entry| {
                match entry.status {
                    SessionStatus::RunningAttached => summary.attached += 1,
                    SessionStatus::RunningDetached => summary.detached += 1,
                    SessionStatus::Saved => summary.saved += 1,
                }

                DashboardRow::from_session_entry(entry)
            })
            .collect::<Vec<_>>();

        rows.sort_by_key(|row| (status_rank(row.status), row.session_id));

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
        DashboardAction::DISPLAY_ORDER
            .into_iter()
            .filter(|action| self.supports_action(*action))
            .collect()
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

    fn supports_action(&self, action: DashboardAction) -> bool {
        match action {
            DashboardAction::Attach => true,
            DashboardAction::Remove => true,
            DashboardAction::Stop => self.is_running(),
            DashboardAction::Restart => self.is_running() && self.opencode_session_id.is_some(),
        }
    }
}

impl DashboardAction {
    pub const DISPLAY_ORDER: [Self; 4] = [Self::Attach, Self::Remove, Self::Stop, Self::Restart];

    pub fn label(self) -> &'static str {
        match self {
            Self::Attach => "ATTACH",
            Self::Stop => "STOP",
            Self::Remove => "RM",
            Self::Restart => "RESTART",
        }
    }
}

impl DashboardView {
    pub fn sessions(&self) -> impl Iterator<Item = &DashboardRow> {
        self.groups.iter().flat_map(|group| group.sessions.iter())
    }
}

fn status_rank(status: SessionStatus) -> u8 {
    match status {
        SessionStatus::RunningAttached => 0,
        SessionStatus::RunningDetached => 1,
        SessionStatus::Saved => 2,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::session::{SavedSession, SessionListEntry};

    use super::*;

    #[test]
    fn snapshot_rows_sort_by_status_then_id() {
        let rows = DashboardSnapshot::from_session_entries(vec![
            entry(4, SessionStatus::Saved),
            entry(3, SessionStatus::RunningDetached),
            entry(2, SessionStatus::RunningAttached),
            entry(1, SessionStatus::RunningDetached),
            entry(5, SessionStatus::RunningAttached),
        ])
        .rows;

        assert_eq!(
            rows.iter().map(|row| row.session_id).collect::<Vec<_>>(),
            vec![2, 5, 1, 3, 4]
        );
    }

    fn entry(id: i64, status: SessionStatus) -> SessionListEntry {
        SessionListEntry {
            saved_session: SavedSession {
                id,
                name: format!("session-{id}"),
                directory: PathBuf::from(format!("/tmp/session-{id}")),
                opencode_session_id: None,
                opencode_args: Vec::new(),
            },
            status,
            runtime: None,
        }
    }
}
