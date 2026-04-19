mod common;

use common::{
    FakeOpenCode, SavedSessionRow, TestEnv, detach_tmux_client_from_session,
    read_opencode_sessions, read_saved_sessions, saved_session_row, tmux_pane_current_command,
    tmux_pane_pid, wait_for_file_contains, wait_for_file_exists,
    wait_for_tmux_client_detach_window,
};
use predicates::prelude::*;
use std::fs;
use std::process::Stdio;
use std::time::Duration;

const EMPTY_ARGS_JSON: &str = "[]";

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
        .stdout(Stdio::piped())
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
    let child = spawn_new_command(env, fake_opencode, args);

    env.wait_for_tmux_session_exists(session_name);
    allow_new_command_to_settle(session_name);

    let output = child
        .wait_with_output()
        .expect("oc new process should exit after attach handling completes");
    assert!(
        output.status.success(),
        "Expected oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
        vec![saved_session_row(
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
        vec![saved_session_row(
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
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
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
        vec![saved_session_row(2, "two", env.root_dir(), EMPTY_ARGS_JSON)],
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
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
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
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
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

#[test]
fn launch_detach_captures_session_id() {
    let env = TestEnv::new("capture-session-id-on-launch");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    run_new_command_and_wait(&env, &fake_opencode, &session_name, &["new", "dc"]);
    wait_for_file_exists(&fake_opencode.session_id_log_path(), Duration::from_secs(5));

    let captured_id = fs::read_to_string(fake_opencode.session_id_log_path())
        .expect("fake opencode session id log should be readable")
        .trim()
        .to_string();

    assert_saved_sessions(
        &env,
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: env.root_dir().to_path_buf(),
            opencode_session_id: Some(captured_id.clone()),
            opencode_args: String::from(EMPTY_ARGS_JSON),
        }],
    );

    let opencode_sessions = read_opencode_sessions(env.opencode_db());
    assert_eq!(opencode_sessions.len(), 1);
    assert_eq!(opencode_sessions[0].id, captured_id);
}

#[test]
fn restart_uses_captured_session_id() {
    let env = TestEnv::new("restart-uses-captured-session-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    run_new_command_and_wait(&env, &fake_opencode, &session_name, &["new", "dc"]);
    wait_for_file_exists(&fake_opencode.session_id_log_path(), Duration::from_secs(5));

    let captured_id = fs::read_to_string(fake_opencode.session_id_log_path())
        .expect("fake opencode session id log should be readable")
        .trim()
        .to_string();

    env.oc_cmd().args(["restart", "dc"]).assert().success();
    wait_for_file_exists(&fake_opencode.args_log_path(), Duration::from_secs(5));

    assert_eq!(
        fs::read_to_string(fake_opencode.args_log_path()).expect("args log should be readable"),
        format!("--session\n{captured_id}\ncontinue\n")
    );
}
