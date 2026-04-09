mod common;

use common::{
    capture_tmux_pane, create_tmux_session_in_dir, detach_tmux_client_from_session,
    read_saved_sessions, saved_session_row, send_keys_to_tmux_session, spawn_tmux_attach_client,
    tmux_session_attached_count, wait_for_file_exists, wait_for_tmux_client_detach_window,
    wait_for_tmux_pane_contains, wait_for_tmux_session_attached, FakeOpenCode, SavedSessionRow,
    TestEnv,
};
use predicates::prelude::*;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

const EMPTY_ARGS_JSON: &str = "[]";

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
        .stdout(Stdio::piped())
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
    let child = spawn_interactive_oc(env, fake_opencode, args);
    detach_after_attach(session_name);

    let output = child
        .wait_with_output()
        .expect("interactive oc command should exit after detach");
    assert!(
        output.status.success(),
        "Expected interactive oc command to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_interactive_oc_command_launches_and_succeeds_after_detach(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
    session_name: &str,
) {
    let child = spawn_interactive_oc(env, fake_opencode, args);
    env.wait_for_tmux_session_exists(session_name);
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
    detach_after_attach(session_name);

    let output = child
        .wait_with_output()
        .expect("interactive oc command should exit after detach");
    assert!(
        output.status.success(),
        "Expected interactive oc command to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
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

#[test]
fn no_arg_auto_attach_falls_back_to_dashboard_when_attach_fails() {
    let env = TestEnv::new("auto-attach-fallback-on-attach-failure");
    let fake_opencode = env.install_fake_opencode();
    let parent_session_name = format!("{}parent", env.tmux_prefix());

    create_saved_alias(&env, "dc", Some(env.root_dir()));

    create_tmux_session_in_dir(&parent_session_name, env.root_dir());

    let parent_command = format!(
        "PATH=\"{}:{}\" OC_FAKE_OPENCODE_LOG_DIR=\"{}\" OC_ALIASES_FILE=\"{}\" OC_TMUX_PREFIX=\"{}\" OC_OPENCODE_DB=\"{}\" TMUX=nested-test {}",
        fake_opencode.bin_dir().display(),
        std::env::var("PATH").expect("PATH should exist for test"),
        fake_opencode.log_dir().display(),
        env.aliases_file().display(),
        env.tmux_prefix(),
        env.opencode_db().display(),
        assert_cmd::cargo::cargo_bin("oc").display()
    );
    send_keys_to_tmux_session(&parent_session_name, &[&parent_command, "Enter"]);

    let pane =
        wait_for_tmux_pane_contains(&parent_session_name, "filter>", Duration::from_secs(10));

    assert!(
        pane.contains("Auto-attach failed for dc:"),
        "Expected dashboard status message after attach failure\npane:\n{pane}"
    );
    assert!(
        capture_tmux_pane(&parent_session_name).contains("filter>"),
        "Expected dashboard output after attach failure\npane:\n{}",
        capture_tmux_pane(&parent_session_name)
    );
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
}

#[test]
fn bare_target_attach_falls_back_to_dashboard_when_attach_fails() {
    let env = TestEnv::new("bare-target-fallback-on-attach-failure");
    let fake_opencode = env.install_fake_opencode();
    let parent_session_name = format!("{}parent", env.tmux_prefix());

    create_saved_alias(&env, "dc", Some(env.root_dir()));
    create_tmux_session_in_dir(&parent_session_name, env.root_dir());

    let parent_command = format!(
        "PATH=\"{}:{}\" OC_FAKE_OPENCODE_LOG_DIR=\"{}\" OC_ALIASES_FILE=\"{}\" OC_TMUX_PREFIX=\"{}\" OC_OPENCODE_DB=\"{}\" TMUX=nested-test {} dc",
        fake_opencode.bin_dir().display(),
        std::env::var("PATH").expect("PATH should exist for test"),
        fake_opencode.log_dir().display(),
        env.aliases_file().display(),
        env.tmux_prefix(),
        env.opencode_db().display(),
        assert_cmd::cargo::cargo_bin("oc").display()
    );
    send_keys_to_tmux_session(&parent_session_name, &[&parent_command, "Enter"]);

    let pane =
        wait_for_tmux_pane_contains(&parent_session_name, "filter>", Duration::from_secs(10));

    assert!(
        pane.contains("Attach failed for dc:"),
        "Expected dashboard fallback after target attach failure\npane:\n{pane}"
    );
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
}

#[test]
fn new_session_falls_back_to_dashboard_when_attach_fails() {
    let env = TestEnv::new("new-session-fallback-on-attach-failure");
    let fake_opencode = env.install_fake_opencode();
    let parent_session_name = format!("{}parent", env.tmux_prefix());

    create_tmux_session_in_dir(&parent_session_name, env.root_dir());

    let parent_command = format!(
        "PATH=\"{}:{}\" OC_FAKE_OPENCODE_LOG_DIR=\"{}\" OC_ALIASES_FILE=\"{}\" OC_TMUX_PREFIX=\"{}\" OC_OPENCODE_DB=\"{}\" TMUX=nested-test {} new dc",
        fake_opencode.bin_dir().display(),
        std::env::var("PATH").expect("PATH should exist for test"),
        fake_opencode.log_dir().display(),
        env.aliases_file().display(),
        env.tmux_prefix(),
        env.opencode_db().display(),
        assert_cmd::cargo::cargo_bin("oc").display()
    );
    send_keys_to_tmux_session(&parent_session_name, &[&parent_command, "Enter"]);

    let pane =
        wait_for_tmux_pane_contains(&parent_session_name, "filter>", Duration::from_secs(10));

    assert!(
        pane.contains("Attach failed for dc:"),
        "Expected dashboard fallback after new-session attach failure\npane:\n{pane}"
    );
    wait_for_file_exists(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
    assert_saved_sessions(
        &env,
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
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
