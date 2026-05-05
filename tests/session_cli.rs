mod common;

use common::{
    FakeOpenCode, SavedSessionRow, TestEnv, detach_tmux_client_from_session,
    read_opencode_process_sessions, read_opencode_sessions, read_saved_sessions,
    tmux_session_attached_count, update_saved_session_last_used_at, wait_for_file_contains,
    wait_for_file_exists, wait_for_file_to_contain_parseable_u32,
    wait_for_file_to_have_non_empty_contents, wait_for_opencode_process_session,
    wait_for_opencode_process_session_absent, wait_for_opencode_process_session_state,
    wait_for_tmux_pane_current_command_to_contain, wait_for_tmux_pane_pid_to_be_non_zero,
};
use predicates::prelude::*;
use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

const EMPTY_ARGS_JSON: &str = "[]";

fn managed_tmux_session_name(env: &TestEnv, name: &str) -> String {
    format!("{}{}", env.tmux_prefix(), name)
}

fn assert_saved_sessions(env: &TestEnv, expected_rows: Vec<SavedSessionRow>) {
    let actual_rows = read_saved_sessions(env.aliases_file())
        .into_iter()
        .map(|mut row| {
            row.last_used_at = 0;
            row
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_rows, expected_rows);
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
        last_used_at: 0,
    }
}

fn spawn_new_command(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
) -> std::process::Child {
    fake_opencode.reset_logs_for_launch();
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc new should spawn")
}

fn spawn_headless_new_command(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
) -> std::process::Child {
    spawn_headless_new_command_with_lifecycle_delay(env, fake_opencode, None, args)
}

fn spawn_headless_new_command_with_lifecycle_delay(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    delay_ms: Option<u64>,
    args: &[&str],
) -> std::process::Child {
    fake_opencode.reset_logs_for_launch();
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    if let Some(delay_ms) = delay_ms {
        command.env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", delay_ms.to_string());
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("headless oc new should spawn")
}

fn allow_new_command_to_settle(session_name: &str) {
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
    fake_opencode.reset_logs_for_launch();
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
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
    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
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

fn wait_for_command_to_exit_successfully(
    child: std::process::Child,
    description: &str,
    timeout: Duration,
) {
    let start = Instant::now();
    let mut child = child;

    loop {
        if let Some(status) = child
            .try_wait()
            .unwrap_or_else(|error| panic!("Failed to poll {description}: {error}"))
        {
            let output = child.wait_with_output().unwrap_or_else(|error| {
                panic!("Failed to collect output for {description}: {error}")
            });
            assert!(
                status.success(),
                "Expected {description} to exit successfully\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Timed out waiting for {description} to exit after {timeout:?}");
        }

        std::thread::sleep(Duration::from_millis(50));
    }
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
        wait_for_tmux_pane_current_command_to_contain(
            &session_name,
            "opencode",
            Duration::from_secs(5)
        )
        .contains("opencode"),
        "Expected tmux pane command to include opencode"
    );
    assert!(wait_for_tmux_pane_pid_to_be_non_zero(&session_name, Duration::from_secs(5)) > 0);
}

#[test]
fn new_without_tty_creates_alias_and_tmux_session_without_attaching() {
    let env = TestEnv::new("new-without-tty-skips-attach");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "headless");

    let child = spawn_headless_new_command(&env, &fake_opencode, &["new", "headless"]);

    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
    wait_for_command_to_exit_successfully(child, "headless oc new process", Duration::from_secs(5));
    let saved_sessions = read_saved_sessions(env.aliases_file());
    assert_eq!(saved_sessions.len(), 1);
    assert_eq!(saved_sessions[0].name, "headless");
    assert_eq!(saved_sessions[0].directory, env.root_dir());
    assert_eq!(saved_sessions[0].opencode_args, EMPTY_ARGS_JSON);
    assert_eq!(tmux_session_attached_count(&session_name), 0);
}

#[test]
fn new_without_tty_returns_promptly_when_session_id_is_delayed() {
    let env = TestEnv::new("new-without-tty-delayed-session-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "headless");

    let child = spawn_headless_new_command_with_lifecycle_delay(
        &env,
        &fake_opencode,
        Some(1_500),
        &["new", "headless"],
    );

    env.wait_for_tmux_session_exists(&session_name);
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        wait_for_tmux_pane_pid_to_be_non_zero(&session_name, Duration::from_secs(5)),
        Duration::from_secs(5),
        "to remain startup-only before headless command exits",
        |row| row.reason.as_deref() == Some("startup") && row.session_id.is_none(),
    );

    wait_for_command_to_exit_successfully(
        child,
        "headless oc new process with delayed session id",
        Duration::from_secs(3),
    );

    let saved_sessions = read_saved_sessions(env.aliases_file());
    assert_eq!(saved_sessions.len(), 1);
    assert_eq!(saved_sessions[0].name, "headless");
    assert_eq!(saved_sessions[0].opencode_session_id, None);
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

    update_saved_session_last_used_at(env.aliases_file(), "dc", 1);

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
    let stopped_last_used = read_saved_sessions(env.aliases_file())
        .into_iter()
        .find(|row| row.name == "dc")
        .expect("stopped session should still exist")
        .last_used_at;
    assert!(
        stopped_last_used > 1,
        "expected stop to refresh last_used_at"
    );
}

#[test]
fn stop_accepts_numeric_id() {
    let env = TestEnv::new("stop-by-id");
    let fake_opencode = env.install_fake_opencode();
    let session_name = launch_saved_session(&env, &fake_opencode, "dc");
    let captured_id = read_captured_session_id(&fake_opencode);

    update_saved_session_last_used_at(env.aliases_file(), "dc", 1);

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
    let stopped_last_used = read_saved_sessions(env.aliases_file())
        .into_iter()
        .find(|row| row.name == "dc")
        .expect("stopped session should still exist")
        .last_used_at;
    assert!(
        stopped_last_used > 1,
        "expected stop by id to refresh last_used_at"
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
            last_used_at: 0,
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

    fake_opencode.reset_logs_for_launch();
    let mut new_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut new_command);
    let child = new_command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .env("OC_FAKE_OPENCODE_DISABLE_SESSION_TABLE", "1")
        .env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1500")
        .args(["new", "dc"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc new should spawn");
    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_to_contain_parseable_u32(&fake_opencode.pid_log_path(), Duration::from_secs(5));

    let fake_pid = pid_logged_by_fake(&fake_opencode);
    let pane_pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_name, Duration::from_secs(5));
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
    wait_for_file_to_contain_parseable_u32(&fake_opencode.pid_log_path(), Duration::from_secs(5));

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

    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
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
    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
    let captured_id = read_captured_session_id(&fake_opencode);
    fs::remove_file(fake_opencode.pid_log_path()).expect("old pid log should be removable");

    fake_opencode.reset_logs_for_launch();
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
    wait_for_file_to_contain_parseable_u32(&fake_opencode.pid_log_path(), Duration::from_secs(5));
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
    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );

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

#[test]
fn launch_detach_captures_session_id_by_pid_when_session_table_is_unavailable() {
    let env = TestEnv::new("capture-session-id-by-pid-without-session-table");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    let child = spawn_new_command_with_lifecycle_delay(&env, &fake_opencode, 1500, &["new", "dc"]);
    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_to_contain_parseable_u32(&fake_opencode.pid_log_path(), Duration::from_secs(5));

    let fake_pid = pid_logged_by_fake(&fake_opencode);
    let created_row = wait_for_opencode_process_session_state(
        env.opencode_db(),
        fake_pid,
        Duration::from_secs(5),
        "to capture created session id without session table rows",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );
    assert_eq!(created_row.pid, fake_pid);

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

    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
    let captured_id = read_captured_session_id(&fake_opencode);

    assert_saved_sessions(
        &env,
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: env.root_dir().to_path_buf(),
            opencode_session_id: Some(captured_id.clone()),
            opencode_args: String::from(EMPTY_ARGS_JSON),
            last_used_at: 0,
        }],
    );

    let process_rows = read_opencode_process_sessions(env.opencode_db());
    assert_eq!(process_rows.len(), 1);
    assert_eq!(process_rows[0].pid, fake_pid);
    assert_eq!(
        process_rows[0].session_id.as_deref(),
        Some(captured_id.as_str())
    );
}

#[test]
fn launch_detach_falls_back_to_directory_diff_when_process_session_table_is_missing() {
    let env = TestEnv::new("capture-session-id-without-process-session-table");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    fake_opencode.reset_logs_for_launch();
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    let child = command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1500")
        .env("OC_FAKE_OPENCODE_DISABLE_PROCESS_SESSION_TABLE", "1")
        .args(["new", "dc"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc new should spawn");
    env.wait_for_tmux_session_exists(&session_name);
    wait_for_file_to_contain_parseable_u32(&fake_opencode.pid_log_path(), Duration::from_secs(5));
    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
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

    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
    let captured_id = read_captured_session_id(&fake_opencode);

    assert_saved_sessions(
        &env,
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: env.root_dir().to_path_buf(),
            opencode_session_id: Some(captured_id.clone()),
            opencode_args: String::from(EMPTY_ARGS_JSON),
            last_used_at: 0,
        }],
    );

    assert_eq!(
        read_opencode_process_sessions(env.opencode_db()),
        Vec::new()
    );
    let opencode_sessions = read_opencode_sessions(env.opencode_db());
    assert_eq!(opencode_sessions.len(), 1);
    assert_eq!(opencode_sessions[0].id, captured_id);
}

#[test]
fn restart_uses_captured_session_id_when_only_process_session_support_exists() {
    let env = TestEnv::new("restart-uses-captured-id-without-session-table");
    let fake_opencode = env.install_fake_opencode();
    let session_one = managed_tmux_session_name(&env, "one");
    let session_two = managed_tmux_session_name(&env, "two");

    fake_opencode.reset_logs_for_launch();
    let mut first_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut first_command);
    let first_child = first_command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1800")
        .args(["new", "one"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("first oc new should spawn");

    env.wait_for_tmux_session_exists(&session_one);
    let first_pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_one, Duration::from_secs(5));
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        first_pid,
        Duration::from_secs(5),
        "to be startup-only before second launch",
        |row| row.reason.as_deref() == Some("startup") && row.session_id.is_none(),
    );

    fake_opencode.reset_logs_for_launch();
    let mut second_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut second_command);
    let second_child = second_command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .args(["new", "two"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("second oc new should spawn");

    env.wait_for_tmux_session_exists(&session_two);
    let second_pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_two, Duration::from_secs(5));
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        second_pid,
        Duration::from_secs(5),
        "to capture the second same-directory session id",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );

    allow_new_command_to_settle(&session_two);
    let second_output = second_child
        .wait_with_output()
        .expect("second oc new process should exit after attach handling completes");
    assert!(
        second_output.status.success(),
        "Expected second oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );

    wait_for_opencode_process_session_state(
        env.opencode_db(),
        first_pid,
        Duration::from_secs(5),
        "to capture the first same-directory session id",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );

    allow_new_command_to_settle(&session_one);
    let first_output = first_child
        .wait_with_output()
        .expect("first oc new process should exit after attach handling completes");
    assert!(
        first_output.status.success(),
        "Expected first oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr)
    );

    let saved_rows = read_saved_sessions(env.aliases_file());
    let first_saved = saved_rows
        .iter()
        .find(|row| row.name == "one")
        .expect("saved row for first session should exist");
    let process_rows = read_opencode_process_sessions(env.opencode_db());
    let first_process = process_rows
        .iter()
        .find(|row| row.pid == first_pid)
        .expect("first process row should exist");
    let captured_id = first_saved
        .opencode_session_id
        .clone()
        .expect("first session should capture an OpenCode session ID");
    assert_eq!(
        first_process.session_id.as_deref(),
        Some(captured_id.as_str())
    );

    let mut restart_command = env.oc_cmd();
    fake_opencode.apply_to_assert_cmd(&mut restart_command);
    restart_command.args(["restart", "one"]).assert().success();
    wait_for_file_exists(&fake_opencode.args_log_path(), Duration::from_secs(5));

    assert_eq!(
        fs::read_to_string(fake_opencode.args_log_path()).expect("args log should be readable"),
        format!("--session\n{captured_id}\n")
    );
}

#[test]
fn concurrent_same_directory_sessions_capture_distinct_ids_by_pid() {
    let env = TestEnv::new("concurrent-same-dir-captures-distinct-ids-by-pid");
    let fake_opencode = env.install_fake_opencode();

    let session_one = managed_tmux_session_name(&env, "one");
    let session_two = managed_tmux_session_name(&env, "two");

    fake_opencode.reset_logs_for_launch();
    let mut first_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut first_command);
    let first_child = first_command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .env("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1800")
        .args(["new", "one"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("first oc new should spawn");

    env.wait_for_tmux_session_exists(&session_one);
    let first_pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_one, Duration::from_secs(5));
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        first_pid,
        Duration::from_secs(5),
        "to be startup-only before second launch",
        |row| row.reason.as_deref() == Some("startup") && row.session_id.is_none(),
    );

    fake_opencode.reset_logs_for_launch();
    let mut second_command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut second_command);
    let second_child = second_command
        .env("OC_FORCE_ATTACH_FOR_TESTS", "1")
        .args(["new", "two"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("second oc new should spawn");

    env.wait_for_tmux_session_exists(&session_two);
    let second_pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_two, Duration::from_secs(5));
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        second_pid,
        Duration::from_secs(5),
        "to capture the second same-directory session id",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );

    allow_new_command_to_settle(&session_two);
    let second_output = second_child
        .wait_with_output()
        .expect("second oc new process should exit after attach handling completes");
    assert!(
        second_output.status.success(),
        "Expected second oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );

    wait_for_opencode_process_session_state(
        env.opencode_db(),
        first_pid,
        Duration::from_secs(5),
        "to capture the first same-directory session id",
        |row| row.reason.as_deref() == Some("created") && row.session_id.is_some(),
    );

    allow_new_command_to_settle(&session_one);
    let first_output = first_child
        .wait_with_output()
        .expect("first oc new process should exit after attach handling completes");
    assert!(
        first_output.status.success(),
        "Expected first oc new to exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr)
    );

    let saved_rows = read_saved_sessions(env.aliases_file());
    assert_eq!(saved_rows.len(), 2);
    let first_saved = saved_rows
        .iter()
        .find(|row| row.name == "one")
        .expect("saved row for first session should exist");
    let second_saved = saved_rows
        .iter()
        .find(|row| row.name == "two")
        .expect("saved row for second session should exist");

    let process_rows = read_opencode_process_sessions(env.opencode_db());
    assert_eq!(process_rows.len(), 2);
    let first_process = process_rows
        .iter()
        .find(|row| row.pid == first_pid)
        .expect("first process row should exist");
    let second_process = process_rows
        .iter()
        .find(|row| row.pid == second_pid)
        .expect("second process row should exist");

    assert_eq!(first_saved.opencode_session_id, first_process.session_id);
    assert_eq!(second_saved.opencode_session_id, second_process.session_id);
    assert_ne!(
        first_saved.opencode_session_id,
        second_saved.opencode_session_id
    );
    assert_eq!(read_opencode_sessions(env.opencode_db()).len(), 2);
}
