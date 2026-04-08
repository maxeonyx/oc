mod common;

use common::{
    FakeOpenCode, TestEnv, detach_tmux_client_from_session, spawn_tmux_attach_client,
    tmux_session_attached_count, wait_for_file_exists, wait_for_tmux_client_detach_window,
    wait_for_tmux_session_attached,
};
use predicates::prelude::*;
use rusqlite::{Connection, OpenFlags, params};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const EMPTY_ARGS_JSON: &str = "[]";

#[derive(Debug, PartialEq, Eq)]
struct SavedSessionRow {
    id: i64,
    name: String,
    directory: PathBuf,
    opencode_session_id: Option<String>,
    opencode_args: String,
}

fn read_saved_sessions(db_path: &Path) -> Vec<SavedSessionRow> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let mut statement = connection
        .prepare(
            "
            SELECT id, name, directory, opencode_session_id, opencode_args
            FROM sessions
            ORDER BY id
            ",
        )
        .expect("sessions table should be queryable");

    statement
        .query_map(params![], |row| {
            Ok(SavedSessionRow {
                id: row.get(0)?,
                name: row.get(1)?,
                directory: PathBuf::from(row.get::<_, String>(2)?),
                opencode_session_id: row.get(3)?,
                opencode_args: row.get(4)?,
            })
        })
        .expect("session rows should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("session rows should decode")
}

fn saved_session(id: i64, name: &str, directory: &Path, opencode_args: &str) -> SavedSessionRow {
    SavedSessionRow {
        id,
        name: String::from(name),
        directory: directory.to_path_buf(),
        opencode_session_id: None,
        opencode_args: String::from(opencode_args),
    }
}

fn managed_tmux_session_name(env: &TestEnv, name: &str) -> String {
    format!("{}{}", env.tmux_prefix(), name)
}

fn assert_saved_sessions(env: &TestEnv, expected_rows: Vec<SavedSessionRow>) {
    assert_eq!(read_saved_sessions(env.aliases_file()), expected_rows);
}

fn spawn_interactive_oc(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
) -> std::process::Child {
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc command should spawn")
}

fn detach_after_attach(session_name: &str) {
    wait_for_tmux_session_attached(session_name, Duration::from_secs(5));
    wait_for_tmux_client_detach_window();
    detach_tmux_client_from_session(session_name);
}

fn assert_interactive_oc_command_succeeds_after_detach(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
    session_name: &str,
) {
    let mut child = spawn_interactive_oc(env, fake_opencode, args);
    detach_after_attach(session_name);

    let status = child
        .wait()
        .expect("interactive oc command should exit after detach");
    assert!(
        status.success(),
        "Expected interactive oc command to succeed"
    );
}

fn assert_interactive_oc_command_launches_and_succeeds_after_detach(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
    session_name: &str,
) {
    let mut child = spawn_interactive_oc(env, fake_opencode, args);
    env.wait_for_tmux_session_exists(session_name);
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
    detach_after_attach(session_name);

    let status = child
        .wait()
        .expect("interactive oc command should exit after detach");
    assert!(
        status.success(),
        "Expected interactive oc command to succeed"
    );
}

fn create_saved_alias(env: &TestEnv, name: &str, directory: Option<&Path>) {
    let mut command = env.oc_cmd();
    if let Some(directory) = directory {
        command.args([
            "alias",
            name,
            directory
                .to_str()
                .expect("directory should be valid UTF-8 for test"),
        ]);
    } else {
        command.current_dir(env.root_dir()).args(["alias", name]);
    }

    command.assert().success();
}

fn launch_via_new(env: &TestEnv, fake_opencode: &FakeOpenCode, name: &str) {
    let session_name = managed_tmux_session_name(env, name);
    assert_interactive_oc_command_launches_and_succeeds_after_detach(
        env,
        fake_opencode,
        &["new", name],
        &session_name,
    );
}

#[test]
fn bare_target_attaches_running_session_by_name() {
    let env = TestEnv::new("bare-target-attaches-by-name");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    launch_via_new(&env, &fake_opencode, "dc");

    assert_interactive_oc_command_succeeds_after_detach(
        &env,
        &fake_opencode,
        &["dc"],
        &session_name,
    );
}

#[test]
fn bare_target_attaches_running_session_by_numeric_id() {
    let env = TestEnv::new("bare-target-attaches-by-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    launch_via_new(&env, &fake_opencode, "dc");

    assert_interactive_oc_command_succeeds_after_detach(
        &env,
        &fake_opencode,
        &["1"],
        &session_name,
    );
}

#[test]
fn bare_target_launches_saved_alias_by_name_when_tmux_session_is_missing() {
    let env = TestEnv::new("bare-target-launches-by-name");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    create_saved_alias(&env, "dc", None);

    assert_interactive_oc_command_launches_and_succeeds_after_detach(
        &env,
        &fake_opencode,
        &["dc"],
        &session_name,
    );
    assert_saved_sessions(
        &env,
        vec![saved_session(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
}

#[test]
fn bare_target_launches_saved_alias_by_numeric_id_when_tmux_session_is_missing() {
    let env = TestEnv::new("bare-target-launches-by-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    create_saved_alias(&env, "dc", None);

    assert_interactive_oc_command_launches_and_succeeds_after_detach(
        &env,
        &fake_opencode,
        &["1"],
        &session_name,
    );
}

#[test]
fn no_arg_auto_attach_launches_single_directory_match() {
    let env = TestEnv::new("auto-attach-single-match-launches");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    create_saved_alias(&env, "dc", Some(env.root_dir()));

    assert_interactive_oc_command_launches_and_succeeds_after_detach(
        &env,
        &fake_opencode,
        &[],
        &session_name,
    );
}

#[test]
fn no_arg_auto_attach_attaches_single_running_directory_match() {
    let env = TestEnv::new("auto-attach-single-running-match");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    launch_via_new(&env, &fake_opencode, "dc");

    assert_interactive_oc_command_succeeds_after_detach(&env, &fake_opencode, &[], &session_name);
}

fn assert_hidden_session_dump_status(env: &TestEnv, expected_status: &str) {
    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "status={expected_status}"
        )));
}

#[test]
fn hidden_session_dump_reports_saved_only_session() {
    let env = TestEnv::new("session-dump-saved-only");

    create_saved_alias(&env, "dc", None);

    assert_hidden_session_dump_status(&env, "saved");
}

#[test]
fn hidden_session_dump_reports_running_detached_session() {
    let env = TestEnv::new("session-dump-running-detached");
    let fake_opencode = env.install_fake_opencode();

    launch_via_new(&env, &fake_opencode, "dc");

    assert_hidden_session_dump_status(&env, "running_detached");
}

#[test]
fn hidden_session_dump_reports_running_attached_session() {
    let env = TestEnv::new("session-dump-running-attached");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    launch_via_new(&env, &fake_opencode, "dc");

    let mut attached_client = spawn_tmux_attach_client(&session_name);
    wait_for_tmux_session_attached(&session_name, Duration::from_secs(5));

    assert_hidden_session_dump_status(&env, "running_attached");

    detach_tmux_client_from_session(&session_name);
    let status = attached_client
        .wait()
        .expect("attached tmux client should exit after detach");
    assert!(
        status.success(),
        "Expected helper attach client to exit cleanly"
    );
    assert!(tmux_session_attached_count(&session_name) == 0);
}
