use crate::session::SessionStatus;

use super::types::{DashboardAction, DashboardRow, DashboardView, InputMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedSession(pub i64);

pub fn select_index(
    view: &DashboardView,
    selected_identity: Option<SelectedSession>,
    current_directory: Option<&std::path::Path>,
) -> usize {
    let target_identity =
        selected_identity.or_else(|| default_selected_identity(view, current_directory));

    let sessions = view.sessions().collect::<Vec<_>>();

    match target_identity {
        Some(SelectedSession(session_id)) => sessions
            .iter()
            .position(|session| session.session_id == session_id)
            .unwrap_or(0),
        None => 0,
    }
}

pub fn select_index_for_input(
    view: &DashboardView,
    selected_identity: Option<SelectedSession>,
    current_directory: Option<&std::path::Path>,
    input_mode: InputMode,
    input_text: &str,
) -> usize {
    let preserve_selection = input_mode == InputMode::Command || input_text.is_empty();
    select_index(
        view,
        selected_identity.filter(|_| preserve_selection),
        current_directory,
    )
}

pub fn preferred_action_for_row(row: &DashboardRow, current: DashboardAction) -> DashboardAction {
    let available = available_actions(row);
    if available.is_empty() {
        return current;
    }

    if available.contains(&current) {
        return current;
    }

    let ordered_actions = DashboardAction::ALL;
    let current_index = ordered_actions
        .iter()
        .position(|action| *action == current)
        .unwrap_or(0);

    for offset in 1..=ordered_actions.len() {
        let candidate = ordered_actions[(current_index + offset) % ordered_actions.len()];
        if available.contains(&candidate) {
            return candidate;
        }
    }

    available[0]
}

pub fn cycle_action_for_row(
    row: &DashboardRow,
    current: DashboardAction,
    delta: isize,
) -> DashboardAction {
    let actions = available_actions(row);
    if actions.is_empty() {
        return current;
    }

    let current = preferred_action_for_row(row, current);
    let current_index = actions
        .iter()
        .position(|action| *action == current)
        .unwrap_or(0) as isize;
    let len = actions.len() as isize;
    actions[(current_index + delta).rem_euclid(len) as usize]
}

pub fn available_actions(row: &DashboardRow) -> Vec<DashboardAction> {
    row.available_actions()
}

fn default_selected_identity(
    view: &DashboardView,
    current_directory: Option<&std::path::Path>,
) -> Option<SelectedSession> {
    if let Some(current_directory) = current_directory {
        for session in view.sessions() {
            if session.full_directory == current_directory {
                return Some(SelectedSession(session.session_id));
            }
        }
    }

    view.sessions()
        .next()
        .map(|session| SelectedSession(session.session_id))
}

pub fn action_label(action: DashboardAction) -> &'static str {
    match action {
        DashboardAction::Attach => "ATTACH",
        DashboardAction::Stop => "STOP",
        DashboardAction::Remove => "RM",
        DashboardAction::Restart => "RESTART",
    }
}

pub fn status_label(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::RunningAttached => "attached",
        SessionStatus::RunningDetached => "detached",
        SessionStatus::Saved => "saved",
    }
}
