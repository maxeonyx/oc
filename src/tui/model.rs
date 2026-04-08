use std::path::Path;

use crate::session::{SavedSession, SessionListEntry, SessionStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub rows: Vec<DashboardRow>,
    pub summary: DashboardSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardSummary {
    pub attached: usize,
    pub detached: usize,
    pub saved: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardRow {
    pub session_id: i64,
    pub name: String,
    pub directory: String,
    pub status: SessionStatus,
}

impl DashboardSnapshot {
    pub fn from_session_entries(entries: Vec<SessionListEntry>) -> Self {
        let mut summary = DashboardSummary {
            attached: 0,
            detached: 0,
            saved: 0,
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
        let saved_session = entry.saved_session;

        Self {
            session_id: saved_session.id,
            name: saved_session.name.clone(),
            directory: abbreviate_directory(&saved_session),
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
}

fn abbreviate_directory(saved_session: &SavedSession) -> String {
    let directory = saved_session.directory.display().to_string();
    let Some(basename) = basename(&saved_session.directory) else {
        return directory;
    };

    if basename != saved_session.name {
        return directory;
    }

    let prefix = saved_session
        .directory
        .parent()
        .map(|parent| parent.display().to_string())
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| String::from("."));

    format!("{prefix}/…")
}

fn basename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(String::from)
}
