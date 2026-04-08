use oc::session::{SavedSession, SessionListEntry, SessionStatus};
use oc::tui::command::{CommandParseError, ParsedCommand, parse_command};
use oc::tui::filter::{build_display_rows, totals_for_rows};
use oc::tui::selection::{preferred_action_for_row, select_index};
use oc::tui::types::{DashboardAction, DashboardSnapshot, DisplayRow, InputMode};

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

    let rows = build_display_rows(&snapshot, "1", InputMode::Filter, None);

    assert_display_rows(
        &rows,
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
        12,
        "12-dc",
        "/tmp/12-dc",
        Some("ses_12abc"),
        SessionStatus::Saved,
    )]);

    let rows = build_display_rows(&snapshot, "12", InputMode::Filter, None);

    assert_display_rows(
        &rows,
        &["header", "group:numeric id", "session:12", "totals:1:0"],
    );
}

#[test]
fn empty_filter_shows_new_row_then_sessions_when_no_directory_match() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);

    let rows = build_display_rows(&snapshot, "", InputMode::Filter, None);

    assert_display_rows(
        &rows,
        &["header", "new", "session:1", "session:2", "totals:2:0"],
    );
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

    let rows = build_display_rows(
        &snapshot,
        "",
        InputMode::Filter,
        Some(PathBuf::from("/work/alpha")),
    );

    assert_display_rows(
        &rows,
        &[
            "header",
            "session:1",
            "new",
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

    let rows = build_display_rows(&snapshot, "", InputMode::Filter, None);
    let sessions = rows
        .iter()
        .filter_map(DisplayRow::session)
        .collect::<Vec<_>>();
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
        ParsedCommand::New {
            name: String::from("dc")
        }
    );
    assert_eq!(
        parse_command("rm 1").unwrap(),
        ParsedCommand::Remove {
            target: String::from("1")
        }
    );
    assert_eq!(
        parse_command("stop dc").unwrap(),
        ParsedCommand::Stop {
            target: String::from("dc")
        }
    );
    assert_eq!(
        parse_command("restart dc").unwrap(),
        ParsedCommand::Restart {
            target: String::from("dc")
        }
    );
    assert_eq!(
        parse_command("mv dc /tmp/project").unwrap(),
        ParsedCommand::Move {
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

    let rows = build_display_rows(&snapshot, "", InputMode::Filter, None);
    let totals = totals_for_rows(&snapshot.summary, &rows);

    assert_eq!(totals.filtered_sessions, 2);
    assert_eq!(totals.filtered_running, 1);
}

#[test]
fn default_selection_prefers_new_session_without_directory_match() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);
    let rows = build_display_rows(&snapshot, "", InputMode::Filter, None);

    assert_eq!(select_index(&rows, None, None), 1);
}

#[test]
fn default_selection_prefers_directory_match_before_new_session() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/work/project", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);
    let rows = build_display_rows(
        &snapshot,
        "",
        InputMode::Filter,
        Some(PathBuf::from("/work/project")),
    );

    assert_eq!(
        select_index(&rows, None, Some(PathBuf::from("/work/project").as_path())),
        1
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
    let rows = build_display_rows(&snapshot, "", InputMode::Filter, None);

    let saved_row = rows
        .iter()
        .find(|row| matches!(row, DisplayRow::Session(session) if session.session_id == 1))
        .expect("saved row should exist");
    let running_row = rows
        .iter()
        .find(|row| matches!(row, DisplayRow::Session(session) if session.session_id == 2))
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

fn assert_display_rows(rows: &[DisplayRow], expected: &[&str]) {
    let actual = rows
        .iter()
        .map(|row| match row {
            DisplayRow::ColumnHeader => String::from("header"),
            DisplayRow::GroupHeader { title } => format!("group:{title}"),
            DisplayRow::NewSession => String::from("new"),
            DisplayRow::Session(row) => format!("session:{}", row.session_id),
            DisplayRow::Totals(summary) => format!(
                "totals:{}:{}",
                summary.filtered_sessions, summary.filtered_running
            ),
        })
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}
