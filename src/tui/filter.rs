use std::path::{Path, PathBuf};

use super::format::format_memory;
use super::types::{DashboardRow, DashboardSnapshot, DashboardSummary, DisplayRow, InputMode};

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

pub fn build_display_rows(
    snapshot: &DashboardSnapshot,
    filter_text: &str,
    input_mode: InputMode,
    current_directory: Option<PathBuf>,
) -> Vec<DisplayRow> {
    let body = if input_mode == InputMode::Command || filter_text.is_empty() {
        display_rows_without_filter(snapshot, current_directory.as_deref())
    } else {
        display_rows_with_filter(snapshot, filter_text)
    };

    let mut rows = vec![DisplayRow::ColumnHeader];
    rows.extend(body);
    rows.push(DisplayRow::Totals(totals_for_rows(
        &snapshot.summary,
        &rows,
    )));
    rows
}

pub fn totals_for_rows(summary: &DashboardSummary, rows: &[DisplayRow]) -> DashboardSummary {
    let mut totals = summary.clone();
    totals.filtered_sessions = 0;
    totals.filtered_running = 0;
    totals.filtered_memory_bytes = 0;

    for row in rows.iter().filter_map(DisplayRow::session) {
        totals.filtered_sessions += 2;
        if row.is_running() {
            totals.filtered_running += 1;
        }
        totals.filtered_memory_bytes += row.memory_bytes.unwrap_or(0);
    }

    totals
}

pub fn totals_label(summary: &DashboardSummary) -> String {
    format!(
        "{} sessions  {} running  {}",
        summary.filtered_sessions,
        summary.filtered_running,
        format_memory(summary.filtered_memory_bytes)
    )
}

fn display_rows_without_filter(
    snapshot: &DashboardSnapshot,
    current_directory: Option<&Path>,
) -> Vec<DisplayRow> {
    let (matching_rows, remaining_rows) = match current_directory {
        Some(current_directory) => snapshot
            .rows
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|row| row.full_directory == current_directory),
        None => (Vec::new(), snapshot.rows.clone()),
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

fn display_rows_with_filter(snapshot: &DashboardSnapshot, filter_text: &str) -> Vec<DisplayRow> {
    let numeric_filter = is_numeric_filter(filter_text);
    let mut numeric_matches = Vec::new();
    let mut name_matches = Vec::new();
    let mut directory_matches = Vec::new();
    let mut opencode_matches = Vec::new();

    for row in &snapshot.rows {
        let Some((group, strength)) = best_match_for_row(row, filter_text, numeric_filter) else {
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
