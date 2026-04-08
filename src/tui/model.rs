use std::path::{Path, PathBuf};

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
    pub full_directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardAction {
    Attach,
    Stop,
    Remove,
    Restart,
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Filter,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRow {
    GroupHeader { title: String },
    NewSession,
    Session(DashboardRow),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchStrength {
    Exact,
    Prefix,
    Contains,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchGroup {
    NumericId,
    Name,
    Directory,
    OpencodeSessionId,
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
            full_directory: saved_session.directory,
            opencode_session_id: saved_session.opencode_session_id,
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
}

impl DisplayRow {
    pub fn session(&self) -> Option<&DashboardRow> {
        match self {
            Self::Session(row) => Some(row),
            _ => None,
        }
    }
}

impl DashboardSnapshot {
    pub fn display_rows(
        &self,
        filter_text: &str,
        input_mode: InputMode,
        current_directory: Option<PathBuf>,
    ) -> Vec<DisplayRow> {
        if input_mode == InputMode::Command || filter_text.is_empty() {
            return self.display_rows_without_filter(current_directory.as_deref());
        }

        self.display_rows_with_filter(filter_text)
    }

    fn display_rows_without_filter(&self, current_directory: Option<&Path>) -> Vec<DisplayRow> {
        let (matching_rows, remaining_rows) = match current_directory {
            Some(current_directory) => self
                .rows
                .iter()
                .cloned()
                .partition::<Vec<_>, _>(|row| row.full_directory == current_directory),
            None => (Vec::new(), self.rows.clone()),
        };

        let mut display_rows = Vec::new();

        if matching_rows.is_empty() {
            display_rows.push(DisplayRow::NewSession);
            display_rows.extend(remaining_rows.into_iter().map(DisplayRow::Session));
            return display_rows;
        }

        display_rows.extend(matching_rows.into_iter().map(DisplayRow::Session));
        display_rows.push(DisplayRow::NewSession);
        display_rows.extend(remaining_rows.into_iter().map(DisplayRow::Session));
        display_rows
    }

    fn display_rows_with_filter(&self, filter_text: &str) -> Vec<DisplayRow> {
        let numeric_filter = is_numeric_filter(filter_text);
        let mut numeric_matches = Vec::new();
        let mut name_matches = Vec::new();
        let mut directory_matches = Vec::new();
        let mut opencode_matches = Vec::new();

        for row in &self.rows {
            let Some((group, strength)) = best_match_for_row(row, filter_text, numeric_filter)
            else {
                continue;
            };

            match group {
                MatchGroup::NumericId => numeric_matches.push((strength, row.clone())),
                MatchGroup::Name => name_matches.push((strength, row.clone())),
                MatchGroup::Directory => directory_matches.push((strength, row.clone())),
                MatchGroup::OpencodeSessionId => opencode_matches.push((strength, row.clone())),
            }
        }

        let mut display_rows = Vec::new();
        append_group(&mut display_rows, MatchGroup::NumericId, numeric_matches);
        append_group(&mut display_rows, MatchGroup::Name, name_matches);
        append_group(&mut display_rows, MatchGroup::Directory, directory_matches);
        append_group(
            &mut display_rows,
            MatchGroup::OpencodeSessionId,
            opencode_matches,
        );

        display_rows
    }
}

fn append_group(
    display_rows: &mut Vec<DisplayRow>,
    group: MatchGroup,
    mut matches: Vec<(MatchStrength, DashboardRow)>,
) {
    if matches.is_empty() {
        return;
    }

    matches.sort_by_key(|(strength, _)| *strength);
    display_rows.push(DisplayRow::GroupHeader {
        title: String::from(group.title()),
    });
    display_rows.extend(matches.into_iter().map(|(_, row)| DisplayRow::Session(row)));
}

fn classify_match(value: &str, filter_text: &str) -> Option<MatchStrength> {
    if value == filter_text {
        return Some(MatchStrength::Exact);
    }

    if value.starts_with(filter_text) {
        return Some(MatchStrength::Prefix);
    }

    value
        .contains(filter_text)
        .then_some(MatchStrength::Contains)
}

fn classify_numeric_id_match(session_id: i64, filter_text: &str) -> Option<MatchStrength> {
    let id_text = session_id.to_string();
    if id_text == filter_text {
        return Some(MatchStrength::Exact);
    }

    if id_text.starts_with(filter_text) {
        return Some(MatchStrength::Prefix);
    }

    (id_text.len() > filter_text.len() && id_text[1..].contains(filter_text))
        .then_some(MatchStrength::Contains)
}

fn best_match_for_row(
    row: &DashboardRow,
    filter_text: &str,
    numeric_filter: bool,
) -> Option<(MatchGroup, MatchStrength)> {
    let mut candidates = Vec::new();

    if numeric_filter {
        if let Some(strength) = classify_numeric_id_match(row.session_id, filter_text) {
            candidates.push((strength, MatchGroup::NumericId));
        }
    }

    if let Some(strength) = classify_match(&row.name, filter_text) {
        candidates.push((strength, MatchGroup::Name));
    }

    if let Some(strength) = classify_match(&row.full_directory.display().to_string(), filter_text) {
        candidates.push((strength, MatchGroup::Directory));
    }

    if let Some(opencode_session_id) = &row.opencode_session_id {
        if let Some(strength) = classify_match(opencode_session_id, filter_text) {
            candidates.push((strength, MatchGroup::OpencodeSessionId));
        }
    }

    candidates
        .into_iter()
        .min_by_key(|(strength, group)| (*strength, *group))
        .map(|(strength, group)| (group, strength))
}

fn is_numeric_filter(filter_text: &str) -> bool {
    !filter_text.is_empty()
        && filter_text
            .chars()
            .all(|character| character.is_ascii_digit())
}

impl MatchGroup {
    fn title(self) -> &'static str {
        match self {
            Self::NumericId => "numeric id",
            Self::Name => "name",
            Self::Directory => "directory",
            Self::OpencodeSessionId => "opencode session id",
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
