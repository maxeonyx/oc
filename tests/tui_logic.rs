use oc::cli::RequestedAction;
use oc::session::{SavedSession, SessionListEntry, SessionStatus};
use oc::tui::command::{parse_command, CommandParseError};
use oc::tui::filter::{build_view, totals_for_rows};
use oc::tui::selection::{preferred_action_for_row, select_index};
use oc::tui::types::{DashboardAction, DashboardRow, DashboardSnapshot, DashboardView, InputMode};

use std::path::PathBuf;

#[test]
fn filter_groups_by_priority_then_match_strength() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(
            1,
            "job-fix-abc",
            "/tmp/job-fix-abc",
            None,
            SessionStatus::Saved,
        ),
        session_entry(
            12,
            "thingymyob",
            "/tmp/thingymyob",
            None,
            SessionStatus::Saved,
        ),
        session_entry(31, "tofu-x", "/tmp/tofu-x", None, SessionStatus::Saved),
        session_entry(16, "1abc", "/tmp/1abc", None, SessionStatus::Saved),
        session_entry(
            54,
            "aid-11000",
            "/tmp/aid-11000",
            None,
            SessionStatus::Saved,
        ),
        session_entry(43, "fix-1", "/tmp/fix-1", None, SessionStatus::Saved),
    ]);

    let view = build_view(&snapshot, "1", InputMode::Filter, None);

    assert_view(
        &view,
        &[
            "header",
            "group:numeric id",
            "session:1",
            "session:12",
            "session:16",
            "session:31",
            "group:name",
            "session:54",
            "session:43",
            "totals:6:0",
        ],
    );
}

#[test]
fn filter_uses_highest_priority_match_only_once() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![session_entry(
        312,
        "12",
        "/tmp/12",
        Some("ses_12abc"),
        SessionStatus::Saved,
    )]);

    let view = build_view(&snapshot, "12", InputMode::Filter, None);

    assert_view(
        &view,
        &["header", "group:numeric id", "session:312", "totals:1:0"],
    );
}

#[test]
fn empty_filter_shows_new_row_then_sessions_when_no_directory_match() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);

    let view = build_view(&snapshot, "", InputMode::Filter, None);

    assert_view(&view, &["header", "session:1", "session:2", "totals:2:0"]);
}

#[test]
fn empty_filter_places_new_row_after_directory_matches() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/work/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
        session_entry(
            3,
            "alpha-two",
            "/work/alpha-two",
            None,
            SessionStatus::Saved,
        ),
    ]);

    let view = build_view(
        &snapshot,
        "",
        InputMode::Filter,
        Some(PathBuf::from("/work/alpha")),
    );

    assert_view(
        &view,
        &[
            "header",
            "session:1",
            "session:2",
            "session:3",
            "totals:3:0",
        ],
    );
}

#[test]
fn available_actions_depend_on_row_status() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(
            1,
            "running",
            "/tmp/running",
            Some("ses_running"),
            SessionStatus::RunningDetached,
        ),
        session_entry(2, "saved", "/tmp/saved", None, SessionStatus::Saved),
    ]);

    let view = build_view(&snapshot, "", InputMode::Filter, None);
    let sessions = view.sessions().collect::<Vec<_>>();
    let running = sessions[0];
    let saved = sessions[1];

    assert_eq!(
        running.available_actions(),
        vec![
            DashboardAction::Attach,
            DashboardAction::Stop,
            DashboardAction::Remove,
            DashboardAction::Restart
        ]
    );
    assert_eq!(
        saved.available_actions(),
        vec![DashboardAction::Attach, DashboardAction::Remove]
    );
}

#[test]
fn command_parser_supports_dashboard_commands() {
    assert_eq!(
        parse_command("new dc").unwrap(),
        RequestedAction::New {
            name: String::from("dc"),
            dir: None,
            opencode_args: Vec::new()
        }
    );
    assert_eq!(
        parse_command("rm 1").unwrap(),
        RequestedAction::Rm {
            target: String::from("1")
        }
    );
    assert_eq!(
        parse_command("stop dc").unwrap(),
        RequestedAction::Stop {
            target: String::from("dc")
        }
    );
    assert_eq!(
        parse_command("restart dc").unwrap(),
        RequestedAction::Restart {
            target: String::from("dc")
        }
    );
    assert_eq!(
        parse_command("mv dc /tmp/project").unwrap(),
        RequestedAction::Move {
            target: String::from("dc"),
            new_dir: PathBuf::from("/tmp/project"),
        }
    );
}

#[test]
fn command_parser_rejects_invalid_input() {
    assert_eq!(
        parse_command("wat").unwrap_err(),
        CommandParseError::UnknownCommand(String::from("wat"))
    );
    assert_eq!(
        parse_command("new").unwrap_err(),
        CommandParseError::MissingArgument(String::from("new"))
    );
    assert_eq!(
        parse_command("mv dc").unwrap_err(),
        CommandParseError::MissingArgument(String::from("mv"))
    );
}

#[test]
fn totals_only_count_session_rows() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(
            2,
            "beta",
            "/tmp/beta",
            Some("ses_beta"),
            SessionStatus::RunningDetached,
        ),
    ]);

    let view = build_view(&snapshot, "", InputMode::Filter, None);
    let totals = totals_for_rows(&snapshot.summary, view.sessions());

    assert_eq!(totals.filtered_sessions, 2);
    assert_eq!(totals.filtered_running, 1);
}

#[test]
fn default_selection_prefers_new_session_without_directory_match() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);
    let view = build_view(&snapshot, "", InputMode::Filter, None);

    assert_eq!(select_index(&view, None, None), 0);
}

#[test]
fn default_selection_prefers_directory_match_before_new_session() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/work/project", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);
    let view = build_view(
        &snapshot,
        "",
        InputMode::Filter,
        Some(PathBuf::from("/work/project")),
    );

    assert_eq!(
        select_index(&view, None, Some(PathBuf::from("/work/project").as_path())),
        0
    );
}

#[test]
fn preferred_action_falls_back_when_row_cannot_support_requested_action() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(
            2,
            "beta",
            "/tmp/beta",
            Some("ses_beta"),
            SessionStatus::RunningDetached,
        ),
    ]);
    let view = build_view(&snapshot, "", InputMode::Filter, None);

    let saved_row = view
        .sessions()
        .find(|session| session.session_id == 1)
        .expect("saved row should exist");
    let running_row = view
        .sessions()
        .find(|session| session.session_id == 2)
        .expect("running row should exist");

    assert_eq!(
        preferred_action_for_row(saved_row, DashboardAction::Restart),
        DashboardAction::Attach
    );
    assert_eq!(
        preferred_action_for_row(running_row, DashboardAction::Restart),
        DashboardAction::Restart
    );
}

#[test]
fn filter_enters_top_result_after_refreshing_from_previous_selection() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(12, "beta", "/tmp/beta", None, SessionStatus::Saved),
        session_entry(2, "1-match", "/tmp/1-match", None, SessionStatus::Saved),
    ]);

    let unfiltered_view = build_view(&snapshot, "", InputMode::Filter, None);
    let previous_selection = Some(oc::tui::selection::SelectedSession(2));

    assert_eq!(select_index(&unfiltered_view, previous_selection, None), 1);

    let filtered_view = build_view(&snapshot, "1", InputMode::Filter, None);

    assert_eq!(select_index(&filtered_view, None, None), 0);
}

fn session_entry(
    id: i64,
    name: &str,
    directory: &str,
    opencode_session_id: Option<&str>,
    status: SessionStatus,
) -> SessionListEntry {
    SessionListEntry {
        saved_session: SavedSession {
            id,
            name: String::from(name),
            directory: PathBuf::from(directory),
            opencode_session_id: opencode_session_id.map(String::from),
            opencode_args: Vec::new(),
        },
        status,
        runtime: None,
    }
}

fn assert_view(view: &DashboardView, expected: &[&str]) {
    let mut actual = vec![String::from("header")];

    for group in &view.groups {
        if let Some(title) = &group.title {
            actual.push(format!("group:{title}"));
        }

        actual.extend(group.sessions.iter().map(render_session_row));
    }

    actual.push(format!(
        "totals:{}:{}",
        view.totals.filtered_sessions, view.totals.filtered_running
    ));

    assert_eq!(actual, expected);
}

fn render_session_row(row: &DashboardRow) -> String {
    format!("session:{}", row.session_id)
}
