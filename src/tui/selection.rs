use crate::session::SessionStatus;

use super::types::{DashboardAction, DisplayRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedIdentity {
    NewSession,
    Session(i64),
}

pub fn select_index(
    display_rows: &[DisplayRow],
    selected_identity: Option<SelectedIdentity>,
    current_directory: Option<&std::path::Path>,
) -> usize {
    let target_identity =
        selected_identity.or_else(|| default_selected_identity(display_rows, current_directory));

    match target_identity {
        Some(SelectedIdentity::NewSession) => display_rows
            .iter()
            .position(|row| matches!(row, DisplayRow::NewSession))
            .unwrap_or(0),
        Some(SelectedIdentity::Session(session_id)) => display_rows
            .iter()
            .position(|row| matches!(row, DisplayRow::Session(session) if session.session_id == session_id))
            .unwrap_or_else(|| {
                display_rows
                    .iter()
                    .position(is_selectable_row)
                    .unwrap_or(0)
            }),
        None => 0,
    }
}

pub fn preferred_action_for_row(row: &DisplayRow, current: DashboardAction) -> DashboardAction {
    let available = available_actions(row);
    if available.is_empty() {
        return current;
    }

    if available.contains(&current) {
        return current;
    }

    if available.contains(&DashboardAction::Attach) {
        DashboardAction::Attach
    } else {
        available[0]
    }
}

pub fn available_actions(row: &DisplayRow) -> Vec<DashboardAction> {
    match row {
        DisplayRow::NewSession => vec![DashboardAction::Create],
        DisplayRow::Session(row) => row.available_actions(),
        _ => Vec::new(),
    }
}

pub fn is_selectable_row(row: &DisplayRow) -> bool {
    matches!(row, DisplayRow::NewSession | DisplayRow::Session(_))
}

fn default_selected_identity(
    display_rows: &[DisplayRow],
    current_directory: Option<&std::path::Path>,
) -> Option<SelectedIdentity> {
    if let Some(current_directory) = current_directory {
        for row in display_rows {
            match row {
                DisplayRow::Session(session) if session.full_directory == current_directory => {
                    return Some(SelectedIdentity::Session(session.session_id));
                }
                _ => {}
            }
        }
    }

    display_rows.iter().find_map(|row| match row {
        DisplayRow::NewSession => Some(SelectedIdentity::NewSession),
        DisplayRow::Session(session) => Some(SelectedIdentity::Session(session.session_id)),
        _ => None,
    })
}

pub fn action_label(action: DashboardAction) -> &'static str {
    match action {
        DashboardAction::Attach => "ATTACH",
        DashboardAction::Stop => "STOP",
        DashboardAction::Remove => "RM",
        DashboardAction::Restart => "RESTART",
        DashboardAction::Create => "CREATE",
    }
}

pub fn status_label(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::RunningAttached => "attached",
        SessionStatus::RunningDetached => "detached",
        SessionStatus::Saved => "saved",
    }
}
