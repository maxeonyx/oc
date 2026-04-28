mod common;

use common::{
    capture_tmux_pane, create_tmux_session_in_dir, detach_tmux_client_from_session,
    ensure_opencode_process_session_table, insert_opencode_session, read_saved_sessions,
    send_keys_to_tmux_session, spawn_tmux_attach_client, tmux_session_attached_count,
    update_opencode_process_session_start_ticks, wait_for_file_exists,
    wait_for_file_to_have_non_empty_contents, wait_for_opencode_process_session_state,
    wait_for_saved_session_id, wait_for_tmux_pane_contains, wait_for_tmux_pane_pid_to_be_non_zero,
    wait_for_tmux_session_attached, wait_for_tmux_session_client_ready_for_detach, FakeOpenCode,
    SavedSessionRow, TestEnv,
};
use predicates::prelude::*;
use serde_json::Value;
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

fn read_captured_session_id(fake_opencode: &FakeOpenCode) -> String {
    std::fs::read_to_string(fake_opencode.session_id_log_path())
        .expect("fake opencode session id log should be readable")
        .trim()
        .to_string()
}

fn spawn_interactive_oc(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
) -> std::process::Child {
    fake_opencode.reset_logs_for_launch();
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
    wait_for_tmux_session_client_ready_for_detach(session_name, Duration::from_secs(5));
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
    wait_for_file_to_have_non_empty_contents(&fake_opencode.cwd_log_path(), Duration::from_secs(5));
    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
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

fn spawn_interactive_oc_with_env(
    env: &TestEnv,
    fake_opencode: &FakeOpenCode,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Child {
    fake_opencode.reset_logs_for_launch();
    let mut command = env.std_oc_cmd();
    fake_opencode.apply_to_command(&mut command);
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oc command should spawn")
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
    let captured_id = read_captured_session_id(&fake_opencode);
    assert_saved_sessions(
        &env,
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: env.root_dir().to_path_buf(),
            opencode_session_id: Some(captured_id),
            opencode_args: String::from(EMPTY_ARGS_JSON),
        }],
    );
}

#[test]
fn bare_target_launches_exact_session_when_longer_prefix_match_exists() {
    let env = TestEnv::new("bare-target-launches-exact-session");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");
    let colliding_session_name = format!("{}dc-1163-something", env.tmux_prefix());

    create_saved_alias(&env, "dc", None);
    create_tmux_session_in_dir(&colliding_session_name, env.root_dir());

    assert_interactive_oc_command_launches_and_succeeds_after_detach(
        &env,
        &fake_opencode,
        &["dc"],
        &session_name,
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
    let saved_sessions = read_saved_sessions(env.aliases_file());
    assert_eq!(saved_sessions.len(), 1);
    assert_eq!(saved_sessions[0].id, 1);
    assert_eq!(saved_sessions[0].name, "dc");
    assert_eq!(saved_sessions[0].directory, env.root_dir());
    assert_eq!(saved_sessions[0].opencode_args, EMPTY_ARGS_JSON);
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

#[test]
fn list_human_reports_empty_state() {
    let env = TestEnv::new("list-human-empty-state");

    env.oc_cmd().args(["list"]).assert().success().stdout(
        predicate::str::contains("NAME")
            .and(predicate::str::contains("SESSION ID"))
            .and(predicate::str::contains("(no sessions)"))
            .and(predicate::str::contains(
                "0 sessions: 0 attached, 0 detached, 0 saved",
            )),
    );
}

#[test]
fn list_json_reports_empty_array() {
    let env = TestEnv::new("list-json-empty-state");

    env.oc_cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout("[]\n");
}

#[test]
fn list_human_includes_public_statuses_and_session_id_placeholder() {
    let env = TestEnv::new("list-human-statuses-and-session-ids");
    let fake_opencode = env.install_fake_opencode();
    let attached_session_name = managed_tmux_session_name(&env, "attached");

    create_saved_alias(&env, "saved", None);
    launch_via_new(&env, &fake_opencode, "detached");
    launch_via_new(&env, &fake_opencode, "attached");

    let detached_id =
        wait_for_saved_session_id(env.aliases_file(), "detached", Duration::from_secs(5));
    let attached_id =
        wait_for_saved_session_id(env.aliases_file(), "attached", Duration::from_secs(5));

    let mut attached_client = spawn_tmux_attach_client(&attached_session_name);
    wait_for_tmux_session_attached(&attached_session_name, Duration::from_secs(5));

    let output = env
        .oc_cmd()
        .args(["list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("list output should be valid UTF-8");

    assert!(
        stdout.contains("NAME"),
        "Expected header in list output\n{stdout}"
    );
    assert!(
        stdout.contains("saved") && stdout.contains("(none)"),
        "Expected missing session ID placeholder in list output\n{stdout}"
    );
    assert!(
        stdout.contains("detached") && stdout.contains(&detached_id),
        "Expected detached session details in list output\n{stdout}"
    );
    assert!(
        stdout.contains("attached") && stdout.contains(&attached_id),
        "Expected attached session details in list output\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("3 sessions: 1 attached, 1 detached, 1 saved")),
        "Expected summary footer in list output\n{stdout}"
    );

    detach_tmux_client_from_session(&attached_session_name);
    let status = attached_client
        .wait()
        .expect("attached tmux client should exit after detach");
    assert!(
        status.success(),
        "Expected helper attach client to exit cleanly"
    );
}

#[test]
fn list_json_uses_public_status_values_and_null_session_ids() {
    let env = TestEnv::new("list-json-statuses-and-session-ids");
    let fake_opencode = env.install_fake_opencode();
    let attached_session_name = managed_tmux_session_name(&env, "attached");

    create_saved_alias(&env, "saved", None);
    launch_via_new(&env, &fake_opencode, "detached");
    launch_via_new(&env, &fake_opencode, "attached");

    let detached_id =
        wait_for_saved_session_id(env.aliases_file(), "detached", Duration::from_secs(5));
    let attached_id =
        wait_for_saved_session_id(env.aliases_file(), "attached", Duration::from_secs(5));

    let mut attached_client = spawn_tmux_attach_client(&attached_session_name);
    wait_for_tmux_session_attached(&attached_session_name, Duration::from_secs(5));

    let output = env
        .oc_cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let sessions: Value = serde_json::from_slice(&output).expect("list JSON should parse");

    let sessions = sessions
        .as_array()
        .expect("list JSON output should be an array");
    assert_eq!(sessions.len(), 3, "Expected three sessions in JSON output");

    let saved = sessions
        .iter()
        .find(|session| session["name"] == "saved")
        .expect("saved session should be present");
    assert_eq!(saved["status"], "saved");
    assert_eq!(saved["session_id"], Value::Null);

    let detached = sessions
        .iter()
        .find(|session| session["name"] == "detached")
        .expect("detached session should be present");
    assert_eq!(detached["status"], "detached");
    assert_eq!(detached["session_id"], detached_id);

    let attached = sessions
        .iter()
        .find(|session| session["name"] == "attached")
        .expect("attached session should be present");
    assert_eq!(attached["status"], "attached");
    assert_eq!(attached["session_id"], attached_id);

    detach_tmux_client_from_session(&attached_session_name);
    let status = attached_client
        .wait()
        .expect("attached tmux client should exit after detach");
    assert!(
        status.success(),
        "Expected helper attach client to exit cleanly"
    );
}

#[test]
fn session_list_catchup_fills_null_id_when_opencode_db_has_exactly_one_root_match() {
    // Old-schema compatibility: only the legacy `session` table exists, so catch-up may still use directory matching.
    let env = TestEnv::new("catchup-single-root-match");
    let keep_dir = env.root_dir().join("keep");
    let ambiguous_dir = env.root_dir().join("ambiguous");

    std::fs::create_dir_all(&keep_dir).expect("keep directory should be creatable");
    std::fs::create_dir_all(&ambiguous_dir).expect("ambiguous directory should be creatable");

    create_saved_alias(&env, "dc", Some(env.root_dir()));
    create_saved_alias(&env, "keep", Some(&keep_dir));
    create_saved_alias(&env, "amb", Some(&ambiguous_dir));
    common::update_saved_session_opencode_session_id(env.aliases_file(), "keep", "ses_existing");

    insert_opencode_session(env.opencode_db(), "ses_single_match", env.root_dir(), None);
    insert_opencode_session(env.opencode_db(), "ses_keep_new", &keep_dir, None);
    insert_opencode_session(env.opencode_db(), "ses_amb_one", &ambiguous_dir, None);
    insert_opencode_session(env.opencode_db(), "ses_amb_two", &ambiguous_dir, None);

    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success();

    assert_eq!(
        read_saved_sessions(env.aliases_file()),
        vec![
            SavedSessionRow {
                id: 1,
                name: String::from("dc"),
                directory: env.root_dir().to_path_buf(),
                opencode_session_id: Some(String::from("ses_single_match")),
                opencode_args: String::from(EMPTY_ARGS_JSON),
            },
            SavedSessionRow {
                id: 2,
                name: String::from("keep"),
                directory: keep_dir,
                opencode_session_id: Some(String::from("ses_existing")),
                opencode_args: String::from(EMPTY_ARGS_JSON),
            },
            SavedSessionRow {
                id: 3,
                name: String::from("amb"),
                directory: ambiguous_dir,
                opencode_session_id: None,
                opencode_args: String::from(EMPTY_ARGS_JSON),
            },
        ]
    );
}

#[test]
fn session_list_catchup_fills_idle_alias_when_process_session_table_exists_and_directory_has_one_root_match(
) {
    let env = TestEnv::new("catchup-idle-single-root-match");

    create_saved_alias(&env, "dc", Some(env.root_dir()));
    ensure_opencode_process_session_table(env.opencode_db());
    insert_opencode_session(
        env.opencode_db(),
        "ses_idle_single_match",
        env.root_dir(),
        None,
    );

    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success();

    assert_eq!(
        read_saved_sessions(env.aliases_file()),
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: env.root_dir().to_path_buf(),
            opencode_session_id: Some(String::from("ses_idle_single_match")),
            opencode_args: String::from(EMPTY_ARGS_JSON),
        }]
    );
}

#[test]
fn session_list_catchup_matches_tilde_alias_directory_against_expanded_home_path() {
    let env = TestEnv::new("catchup-tilde-directory-match");
    let fake_home = env.root_dir().join("home");
    let project_dir = fake_home.join("project");

    std::fs::create_dir_all(&project_dir)
        .expect("fake tilde project directory should be creatable");

    env.oc_cmd()
        .env("HOME", &fake_home)
        .args(["alias", "dc", "~/project"])
        .assert()
        .success();

    ensure_opencode_process_session_table(env.opencode_db());
    insert_opencode_session(env.opencode_db(), "ses_tilde_match", &project_dir, None);

    env.oc_cmd()
        .env("HOME", &fake_home)
        .args(["__dump-session-list"])
        .assert()
        .success();

    assert_eq!(
        read_saved_sessions(env.aliases_file()),
        vec![SavedSessionRow {
            id: 1,
            name: String::from("dc"),
            directory: Path::new("~/project").to_path_buf(),
            opencode_session_id: Some(String::from("ses_tilde_match")),
            opencode_args: String::from(EMPTY_ARGS_JSON),
        }]
    );
}

#[test]
fn session_list_pid_catchup_fills_null_id_after_early_detach() {
    let env = TestEnv::new("catchup-pid-early-detach");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    let child = spawn_interactive_oc_with_env(
        &env,
        &fake_opencode,
        &["new", "dc"],
        &[("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1800")],
    );
    env.wait_for_tmux_session_exists(&session_name);

    wait_for_opencode_process_session_state(
        env.opencode_db(),
        wait_for_tmux_pane_pid_to_be_non_zero(&session_name, Duration::from_secs(5)),
        Duration::from_secs(5),
        "to remain startup-only before detach",
        |row| row.reason.as_deref() == Some("startup") && row.session_id.is_none(),
    );

    insert_opencode_session(env.opencode_db(), "ses_intruder", env.root_dir(), None);

    detach_after_attach(&session_name);

    let output = child
        .wait_with_output()
        .expect("interactive oc command should exit after detach");
    assert!(
        output.status.success(),
        "Expected interactive oc command to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        read_saved_sessions(env.aliases_file())[0].opencode_session_id,
        None
    );

    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
    let created_id = std::fs::read_to_string(fake_opencode.session_id_log_path())
        .expect("fake opencode session id log should be readable")
        .trim()
        .to_string();

    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success();

    assert_eq!(
        read_saved_sessions(env.aliases_file())[0].opencode_session_id,
        Some(created_id)
    );
}

#[test]
fn session_list_pid_catchup_ignores_stale_process_session_row() {
    let env = TestEnv::new("catchup-pid-ignores-stale-row");
    let fake_opencode = env.install_fake_opencode();
    let session_name = managed_tmux_session_name(&env, "dc");

    let child = spawn_interactive_oc_with_env(
        &env,
        &fake_opencode,
        &["new", "dc"],
        &[("OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS", "1800")],
    );
    env.wait_for_tmux_session_exists(&session_name);

    let pid = wait_for_tmux_pane_pid_to_be_non_zero(&session_name, Duration::from_secs(5));
    wait_for_opencode_process_session_state(
        env.opencode_db(),
        pid,
        Duration::from_secs(5),
        "to remain startup-only before detach",
        |row| row.reason.as_deref() == Some("startup") && row.session_id.is_none(),
    );

    detach_after_attach(&session_name);

    let output = child
        .wait_with_output()
        .expect("interactive oc command should exit after detach");
    assert!(
        output.status.success(),
        "Expected interactive oc command to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_file_to_have_non_empty_contents(
        &fake_opencode.session_id_log_path(),
        Duration::from_secs(5),
    );
    update_opencode_process_session_start_ticks(env.opencode_db(), pid, 1);

    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success();

    assert_eq!(
        read_saved_sessions(env.aliases_file())[0].opencode_session_id,
        None
    );
}
