use std::path::{Path, PathBuf};

use super::types::{
    DashboardGroup, DashboardRow, DashboardSnapshot, DashboardSummary, DashboardView, InputMode,
};

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

pub fn build_view(
    snapshot: &DashboardSnapshot,
    filter_text: &str,
    input_mode: InputMode,
    current_directory: Option<PathBuf>,
) -> DashboardView {
    let groups = if is_active_filter(input_mode, filter_text) {
        groups_with_filter(snapshot, filter_text)
    } else {
        groups_without_filter(snapshot, current_directory.as_deref())
    };

    let totals = totals_for_rows(
        &snapshot.summary,
        groups.iter().flat_map(|group| group.sessions.iter()),
    );

    DashboardView { groups, totals }
}

pub fn totals_for_rows<'a>(
    summary: &DashboardSummary,
    rows: impl IntoIterator<Item = &'a DashboardRow>,
) -> DashboardSummary {
    let mut totals = summary.clone();
    totals.filtered_sessions = 0;
    totals.filtered_running = 0;
    totals.filtered_memory_bytes = 0;

    for row in rows {
        totals.filtered_sessions += 1;
        if row.is_running() {
            totals.filtered_running += 1;
        }
        totals.filtered_memory_bytes += row.memory_bytes.unwrap_or(0);
    }

    totals
}

pub fn totals_scope_label(input_mode: InputMode, input_text: &str) -> &'static str {
    if is_active_filter(input_mode, input_text) {
        "filtered"
    } else {
        "all sessions"
    }
}

pub fn summary_for_view(
    summary: &DashboardSummary,
    view: &DashboardView,
    input_mode: InputMode,
    input_text: &str,
) -> DashboardSummary {
    if is_active_filter(input_mode, input_text) {
        let mut filtered_summary = summary.clone();
        filtered_summary.attached = 0;
        filtered_summary.detached = 0;
        filtered_summary.saved = 0;

        for row in view.sessions() {
            match row.status {
                crate::session::SessionStatus::RunningAttached => filtered_summary.attached += 1,
                crate::session::SessionStatus::RunningDetached => filtered_summary.detached += 1,
                crate::session::SessionStatus::Saved => filtered_summary.saved += 1,
            }
        }

        filtered_summary
    } else {
        summary.clone()
    }
}

fn groups_without_filter(
    snapshot: &DashboardSnapshot,
    current_directory: Option<&Path>,
) -> Vec<DashboardGroup> {
    let (matching_rows, remaining_rows) = match current_directory {
        Some(current_directory) => snapshot
            .rows
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|row| row.full_directory == current_directory),
        None => (Vec::new(), snapshot.rows.clone()),
    };

    if matching_rows.is_empty() {
        return vec![DashboardGroup {
            title: None,
            sessions: remaining_rows,
        }];
    }

    vec![DashboardGroup {
        title: None,
        sessions: matching_rows.into_iter().chain(remaining_rows).collect(),
    }]
}

fn groups_with_filter(snapshot: &DashboardSnapshot, filter_text: &str) -> Vec<DashboardGroup> {
    let normalized_filter = filter_text.to_ascii_lowercase();
    let numeric_filter = is_numeric_filter(filter_text);
    let mut numeric_matches = Vec::new();
    let mut name_matches = Vec::new();
    let mut directory_matches = Vec::new();
    let mut opencode_matches = Vec::new();

    for row in &snapshot.rows {
        let Some((group, strength)) = best_match_for_row(row, &normalized_filter, numeric_filter)
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

    let mut groups = Vec::new();
    append_group(&mut groups, MatchGroup::NumericId, numeric_matches);
    append_group(&mut groups, MatchGroup::Name, name_matches);
    append_group(&mut groups, MatchGroup::Directory, directory_matches);
    append_group(&mut groups, MatchGroup::OpencodeSessionId, opencode_matches);

    groups
}

fn append_group(
    groups: &mut Vec<DashboardGroup>,
    group: MatchGroup,
    mut matches: Vec<(MatchStrength, DashboardRow)>,
) {
    if matches.is_empty() {
        return;
    }

    matches.sort_by_key(|(strength, _)| *strength);
    groups.push(DashboardGroup {
        title: Some(String::from(group.title())),
        sessions: matches.into_iter().map(|(_, row)| row).collect(),
    });
}

fn classify_match(value: &str, filter_text: &str) -> Option<MatchStrength> {
    let normalized_value = value.to_ascii_lowercase();
    if value == filter_text {
        return Some(MatchStrength::Exact);
    }

    if normalized_value == filter_text {
        return Some(MatchStrength::Exact);
    }

    if normalized_value.starts_with(filter_text) {
        return Some(MatchStrength::Prefix);
    }

    normalized_value
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

    (id_text.len() > filter_text.len() && id_text.contains(filter_text))
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
        .min_by_key(|(strength, group)| (*group, *strength))
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

fn is_active_filter(input_mode: InputMode, input_text: &str) -> bool {
    input_mode == InputMode::Filter && !input_text.is_empty()
}
