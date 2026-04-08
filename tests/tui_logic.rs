use oc::session::{SavedSession, SessionListEntry, SessionStatus};
use oc::tui::command::{CommandParseError, ParsedCommand, parse_command};
use oc::tui::model::{DashboardAction, DashboardSnapshot, DisplayRow, InputMode};

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

    let rows = snapshot.display_rows("1", InputMode::Filter, None);

    assert_display_rows(
        &rows,
        &[
            "group:numeric id",
            "session:1",
            "session:12",
            "session:16",
            "session:31",
            "group:name",
            "session:54",
            "session:43",
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

    let rows = snapshot.display_rows("12", InputMode::Filter, None);

    assert_display_rows(&rows, &["group:numeric id", "session:12"]);
}

#[test]
fn empty_filter_shows_new_row_then_sessions_when_no_directory_match() {
    let snapshot = DashboardSnapshot::from_session_entries(vec![
        session_entry(1, "alpha", "/tmp/alpha", None, SessionStatus::Saved),
        session_entry(2, "beta", "/tmp/beta", None, SessionStatus::Saved),
    ]);

    let rows = snapshot.display_rows("", InputMode::Filter, None);

    assert_display_rows(&rows, &["new", "session:1", "session:2"]);
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

    let rows = snapshot.display_rows("", InputMode::Filter, Some(PathBuf::from("/work/alpha")));

    assert_display_rows(&rows, &["session:1", "new", "session:2", "session:3"]);
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

    let rows = snapshot.display_rows("", InputMode::Filter, None);
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
    }
}

fn assert_display_rows(rows: &[DisplayRow], expected: &[&str]) {
    let actual = rows
        .iter()
        .map(|row| match row {
            DisplayRow::GroupHeader { title } => format!("group:{title}"),
            DisplayRow::NewSession => String::from("new"),
            DisplayRow::Session(row) => format!("session:{}", row.session_id),
        })
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}
