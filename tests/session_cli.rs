mod common;

use common::{
    detach_tmux_client_from_session, read_opencode_process_sessions, read_opencode_sessions,
    read_saved_sessions, tmux_pane_current_command, tmux_pane_pid, wait_for_file_contains,
    wait_for_file_exists, wait_for_opencode_process_session,
    wait_for_opencode_process_session_absent, wait_for_opencode_process_session_state,
    wait_for_tmux_client_detach_window, FakeOpenCode, SavedSessionRow, TestEnv,
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

fn read_captured_session_id(fake_opencode: &FakeOpenCode) -> String {
    fs::read_to_string(fake_opencode.session_id_log_path())
        .expect("fake opencode session id log should be readable")
        .trim()
        .to_string()
}

fn saved_session_row_with_id(
    id: i64,
    name: &str,
    directory: &std::path::Path,
    opencode_session_id: &str,
    opencode_args: &str,
) -> SavedSessionRow {
    SavedSessionRow {
        id,
        name: String::from(name),
        directory: directory.to_path_buf(),
        opencode_session_id: Some(String::from(opencode_session_id)),
        opencode_args: String::from(opencode_args),
    }
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

fn pid_logged_by_fake(fake_opencode: &FakeOpenCode) -> u32 {
    fs::read_to_string(fake_opencode.pid_log_path())
        .expect("fake opencode pid log should be readable")
        .trim()
        .parse()
        .expect("fake opencode pid log should contain integer pid")
}

fn spawn_new_command_with_lifecycle_delay(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    delay_ms: u64,
    args: &[&str],
) -> std::process::Child {
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    command
        .env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", delay_ms.to_string())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc new should spawn")
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
    let captured_id = read_captured_session_id(&fake_opencode);

    assert_saved_sessions(
        &env,
        vec![saved_session_row_with_id(
            1,
            "worktree",
            env.root_dir(),
            &captured_id,
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
    let captured_id = read_captured_session_id(&fake_opencode);

    assert_saved_sessions(
        &env,
        vec![saved_session_row_with_id(
            1,
            "dc",
            &project_dir,
            &captured_id,
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
    let captured_id = read_captured_session_id(&fake_opencode);

    assert_saved_sessions(
        &env,
        vec![saved_session_row_with_id(
            1,
            "dc",
            env.root_dir(),
            &captured_id,
            EMPTY_ARGS_JSON,
        )],
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

    let session_one = managed_tmux_session_name(&env, "one");
    run_new_command_and_wait(&env, &fake_opencode, &session_one, &["new", "one"]);
    let first_captured_id = read_captured_session_id(&fake_opencode);

    let session_two = managed_tmux_session_name(&env, "two");
    run_new_command_and_wait(&env, &fake_opencode, &session_two, &["new", "two"]);
    let second_captured_id = read_captured_session_id(&fake_opencode);

    env.oc_cmd().args(["rm", "1"]).assert().success();
    env.wait_for_tmux_session_absent(&session_one);

    assert_saved_sessions(
        &env,
        vec![saved_session_row_with_id(
            2,
            "two",
            env.root_dir(),
            &second_captured_id,
            EMPTY_ARGS_JSON,
        )],
    );
    assert_eq!(env.list_tmux_sessions(), vec![session_two]);
    assert_ne!(first_captured_id, second_captured_id);
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
    let captured_id = read_captured_session_id(&fake_opencode);

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
        vec![saved_session_row_with_id(
            1,
            "dc",
            env.root_dir(),
            &captured_id,
            EMPTY_ARGS_JSON,
        )],
    );
}

#[test]
fn stop_accepts_numeric_id() {
    let env = TestEnv::new("stop-by-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");
    let captured_id = read_captured_session_id(&fake_opencode);

    env.oc_cmd().args(["stop", "1"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    assert_saved_sessions(
        &env,
        vec![saved_session_row_with_id(
            1,
            "dc",
            env.root_dir(),
            &captured_id,
            EMPTY_ARGS_JSON,
        )],
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

    let captured_id = read_captured_session_id(&fake_opencode);

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
fn pane_pid_matches_fake_opencode_process_pid() {
    let env = TestEnv::new("pane-pid-matches-fake-opencode-pid");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    let child = spawn_new_command_with_lifecycle_delay(&env, &fake_opencode, 1500, &["new", "dc"]);
    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_exists(&fake_opencode.pid_log_path(), Duration::from_secs(5));

    let fake_pid = pid_logged_by_fake(&fake_opencode);
    let pane_pid = tmux_pane_pid(&session_name);
    let startup_row =
        wait_for_opencode_process_session(env.opencode_db(), fake_pid, Duration::from_secs(5));

    allow_new_command_to_settle(&session_name);
    let output = child
        .wait_with_output()
        .expect("oc new process should exit after attach handling completes");
    assert!(
        output.status.success(),
        "Expected oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(pane_pid, fake_pid);
    assert_eq!(startup_row.pid, fake_pid);
    assert_eq!(startup_row.reason.as_deref(), Some("startup"));
}

#[test]
fn fake_opencode_new_writes_process_session_lifecycle() {
    let env = TestEnv::new("fake-opencode-new-process-session-lifecycle");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    let child = spawn_new_command_with_lifecycle_delay(&env, &fake_opencode, 1500, &["new", "dc"]);
    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_exists(&fake_opencode.pid_log_path(), Duration::from_secs(5));

    let fake_pid = pid_logged_by_fake(&fake_opencode);
    let startup_row =
        wait_for_opencode_process_session(env.opencode_db(), fake_pid, Duration::from_secs(5));
    assert_eq!(startup_row.pid, fake_pid);
    assert_eq!(startup_row.directory, env.root_dir());
    assert_eq!(startup_row.reason.as_deref(), Some("startup"));
    assert_eq!(startup_row.session_id, None);
    assert!(startup_row.proc_start_ticks > 0);

    let created_row = wait_for_opencode_process_session_state(
        env.opencode_db(),
        fake_pid,
        Duration::from_secs(5),
        "to be created",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );

    allow_new_command_to_settle(&session_name);
    let output = child
        .wait_with_output()
        .expect("oc new process should exit after attach handling completes");
    assert!(
        output.status.success(),
        "Expected oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_file_exists(&fake_opencode.session_id_log_path(), Duration::from_secs(5));
    let captured_id = read_captured_session_id(&fake_opencode);
    assert_eq!(
        created_row.session_id.as_deref(),
        Some(captured_id.as_str())
    );

    env.oc_cmd().args(["stop", "dc"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    wait_for_opencode_process_session_absent(env.opencode_db(), fake_pid, Duration::from_secs(5));

    let process_rows_after = read_opencode_process_sessions(env.opencode_db());
    assert!(process_rows_after.iter().all(|row| row.pid != fake_pid));

    let session_rows = read_opencode_sessions(env.opencode_db());
    assert_eq!(session_rows.len(), 1);
    assert_eq!(session_rows[0].id, captured_id);
}

#[test]
fn fake_opencode_resume_writes_resumed_process_session_then_deletes_on_exit() {
    let env = TestEnv::new("fake-opencode-resume-process-session-lifecycle");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    run_new_command_and_wait(&env, &fake_opencode, &session_name, &["new", "dc"]);
    wait_for_file_exists(&fake_opencode.session_id_log_path(), Duration::from_secs(5));
    let captured_id = read_captured_session_id(&fake_opencode);
    fs::remove_file(fake_opencode.pid_log_path()).expect("old pid log should be removable");

    let mut restart_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut restart_command);
    let child = restart_command
        .args(["restart", "dc"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc restart should spawn");

    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_exists(&fake_opencode.pid_log_path(), Duration::from_secs(5));
    let fake_pid = pid_logged_by_fake(&fake_opencode);

    let resumed_row = wait_for_opencode_process_session_state(
        env.opencode_db(),
        fake_pid,
        Duration::from_secs(5),
        "to be resumed",
        |row| {
            row.reason.as_deref() == Some("resumed")
                && row.session_id.as_deref() == Some(captured_id.as_str())
        },
    );
    assert_eq!(resumed_row.pid, fake_pid);
    assert_eq!(resumed_row.directory, env.root_dir());
    assert_eq!(resumed_row.reason.as_deref(), Some("resumed"));
    assert_eq!(
        resumed_row.session_id.as_deref(),
        Some(captured_id.as_str())
    );
    assert!(resumed_row.proc_start_ticks > 0);

    allow_new_command_to_settle(&session_name);
    let output = child
        .wait_with_output()
        .expect("oc restart process should exit after attach handling completes");
    assert!(
        output.status.success(),
        "Expected oc restart to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    env.oc_cmd().args(["stop", "dc"]).assert().success();
    env.wait_for_tmux_session_absent(&session_name);
    wait_for_opencode_process_session_absent(env.opencode_db(), fake_pid, Duration::from_secs(5));
}

#[test]
fn restart_uses_captured_session_id() {
    let env = TestEnv::new("restart-uses-captured-session-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    run_new_command_and_wait(&env, &fake_opencode, &session_name, &["new", "dc"]);
    wait_for_file_exists(&fake_opencode.session_id_log_path(), Duration::from_secs(5));

    let captured_id = read_captured_session_id(&fake_opencode);

    let mut restart_command = env.oc_cmd();
    fake_opencode.apply_to_assert_cmd(&mut restart_command);
    restart_command.args(["restart", "dc"]).assert().success();
    wait_for_file_exists(&fake_opencode.args_log_path(), Duration::from_secs(5));

    assert_eq!(
        fs::read_to_string(fake_opencode.args_log_path()).expect("args log should be readable"),
        format!("--session\n{captured_id}\n")
    );
}
