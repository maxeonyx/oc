mod common;

use common::{
    FakeOpenCode, TestEnv, detach_tmux_client_from_session, tmux_pane_current_command,
    tmux_pane_pid, wait_for_file_contains, wait_for_file_exists,
    wait_for_tmux_client_detach_window,
};
use predicates::prelude::*;
use rusqlite::{Connection, OpenFlags, params};
use std::fs;
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

fn spawn_new_command(
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
        .expect("oc new should spawn")
}

fn allow_new_command_to_settle(session_name: &str) {
    wait_for_tmux_client_detach_window();
    detach_tmux_client_from_session(session_name);
}

fn run_new_command_and_wait(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    session_name: &str,
    args: &[&str],
) {
    let mut child = spawn_new_command(env, fake_opencode, args);

    env.wait_for_tmux_session_exists(session_name);
    allow_new_command_to_settle(session_name);

    let status = child
        .wait()
        .expect("oc new process should exit after attach handling completes");
    assert!(status.success(), "Expected oc new to exit successfully");
}

fn launch_saved_session(env: &TestEnv, fake_opencode: &FakeOpenCode, name: &str) -> String {
    let session_name = managed_tmux_session_name(env, name);
    run_new_command_and_wait(env, fake_opencode, &session_name, &["new", name]);
    session_name
}

#[test]
fn new_creates_alias_launches_tmux_session_and_attaches() {
    let env = TestEnv::new("new-creates-and-attaches");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "worktree");

    run_new_command_and_wait(&env, &fake_opencode, &session_name, &["new", "worktree"]);
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));

    assert_saved_sessions(
        &env,
        vec![saved_session(
            1,
            "worktree",
            env.root_dir(),
            EMPTY_ARGS_JSON,
        )],
    );
    assert!(
        tmux_pane_current_command(&session_name).contains("opencode"),
        "Expected tmux pane command to include opencode"
    );
    assert!(tmux_pane_pid(&session_name) > 0);
}

#[test]
fn new_uses_explicit_dir_and_args_when_launching_tmux_session() {
    let env = TestEnv::new("new-explicit-dir-and-args");
    let fake_opencode = env.install_fake_opencode();
    let project_dir = env.root_dir().join("project");
    fs::create_dir_all(&project_dir).expect("test should create explicit project directory");
    let session_name = managed_tmux_session_name(&env, "dc");

    run_new_command_and_wait(
        &env,
        &fake_opencode,
        &session_name,
        &[
            "new",
            "dc",
            project_dir
                .to_str()
                .expect("project dir should be valid UTF-8 for test"),
            "--",
            "--model",
            "gpt-5.4",
        ],
    );

    wait_for_file_exists(&fake_opencode.args_log_path(), Duration::from_secs(5));

    assert_saved_sessions(
        &env,
        vec![saved_session(
            1,
            "dc",
            &project_dir,
            "[\"--model\",\"gpt-5.4\"]",
        )],
    );
    assert_eq!(
        fs::read_to_string(fake_opencode.cwd_log_path()).expect("cwd log should be readable"),
        format!("{}\n", project_dir.display())
    );
    assert_eq!(
        fs::read_to_string(fake_opencode.args_log_path()).expect("args log should be readable"),
        "--model\ngpt-5.4\n"
    );
}

#[test]
fn new_rejects_missing_directory() {
    let env = TestEnv::new("new-rejects-missing-directory");
    let missing_dir = env.root_dir().join("missing");

    env.oc_cmd()
        .args([
            "new",
            "worktree",
            missing_dir
                .to_str()
                .expect("missing dir should be valid UTF-8 for test"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("directory"));
}

#[test]
fn new_rejects_duplicate_name_without_creating_second_tmux_session() {
    let env = TestEnv::new("new-rejects-duplicate-name");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");

    let mut duplicate_command = env.oc_cmd();
    fake_opencode.apply_to_assert_cmd(&mut duplicate_command);
    duplicate_command
        .current_dir(env.root_dir())
        .args(["new", "dc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    assert_saved_sessions(
        &env,
        vec![saved_session(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
    assert_eq!(env.list_tmux_sessions(), vec![session_name]);
}

#[test]
fn rm_removes_alias_and_kills_running_tmux_session_by_name() {
    let env = TestEnv::new("rm-by-name");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");

    env.oc_cmd().args(["rm", "dc"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    assert_saved_sessions(&env, Vec::new());
}

#[test]
fn rm_removes_alias_and_kills_running_tmux_session_by_numeric_id() {
    let env = TestEnv::new("rm-by-id");
    let fake_opencode = env.install_fake_opencode();

    let session_one = launch_saved_session(&env, &fake_opencode, "one");
    let session_two = launch_saved_session(&env, &fake_opencode, "two");

    env.oc_cmd().args(["rm", "1"]).assert().success();
    env.wait_for_tmux_session_absent(&session_one);

    assert_saved_sessions(
        &env,
        vec![saved_session(2, "two", env.root_dir(), EMPTY_ARGS_JSON)],
    );
    assert_eq!(env.list_tmux_sessions(), vec![session_two]);
}

#[test]
fn rm_fails_cleanly_when_target_not_found() {
    let env = TestEnv::new("rm-missing-target");

    env.oc_cmd()
        .args(["rm", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn stop_sends_ctrl_c_then_ctrl_d_and_keeps_alias() {
    let env = TestEnv::new("stop-graceful-shutdown");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");

    env.oc_cmd().args(["stop", "dc"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    let events = wait_for_file_contains(
        &fake_opencode.events_log_path(),
        "EOF",
        Duration::from_secs(5),
    );

    assert!(
        events.contains("INT"),
        "Expected fake opencode to receive ctrl-c"
    );
    assert!(
        events.contains("EOF"),
        "Expected fake opencode stdin to close after ctrl-d"
    );
    assert_saved_sessions(
        &env,
        vec![saved_session(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
}

#[test]
fn stop_accepts_numeric_id() {
    let env = TestEnv::new("stop-by-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");

    env.oc_cmd().args(["stop", "1"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    assert_saved_sessions(
        &env,
        vec![saved_session(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
}

#[test]
fn stop_fails_cleanly_when_target_not_found() {
    let env = TestEnv::new("stop-missing-target");

    env.oc_cmd()
        .args(["stop", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
