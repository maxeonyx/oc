mod common;

use common::{
    TestEnv, create_tmux_session_in_dir, create_tmux_session_in_dir_with_size,
    send_keys_to_tmux_session, update_saved_session_opencode_session_id,
    wait_for_tmux_pane_contains,
};
use std::fs;
use std::time::Duration;

fn managed_tmux_session_name(env: &TestEnv, name: &str) -> String {
    format!("{}{}", env.tmux_prefix(), name)
}

fn create_saved_alias(env: &TestEnv, name: &str, directory: &std::path::Path) {
    env.oc_cmd()
        .args([
            "alias",
            name,
            directory
                .to_str()
                .expect("directory should be valid UTF-8 for test"),
        ])
        .assert()
        .success();
}

fn launch_dashboard(env: &TestEnv, parent_session_name: &str) -> String {
    let command = format!(
        "OC_THEME=dark OC_ALIASES_FILE=\"{}\" OC_TMUX_PREFIX=\"{}\" OC_OPENCODE_DB=\"{}\" {}",
        env.aliases_file().display(),
        env.tmux_prefix(),
        env.opencode_db().display(),
        assert_cmd::cargo::cargo_bin("oc").display(),
    );
    send_keys_to_tmux_session(parent_session_name, &[&command, "Enter"]);
    wait_for_tmux_pane_contains(parent_session_name, "filter>", Duration::from_secs(10))
}

#[test]
fn dashboard_removes_enter_hint_and_shows_restart_for_running_session_with_saved_id() {
    let env = TestEnv::new("dashboard-actions");
    create_saved_alias(&env, "dc", env.root_dir());
    update_saved_session_opencode_session_id(env.aliases_file(), "dc", "ses_demo_restart");
    create_tmux_session_in_dir(&managed_tmux_session_name(&env, "dc"), env.root_dir());

    let parent_session_name = format!("{}dashboard", env.tmux_prefix());
    create_tmux_session_in_dir_with_size(&parent_session_name, env.root_dir(), 120, 20);
    let pane = launch_dashboard(&env, &parent_session_name);

    for label in ["ATTACH", "RM", "STOP", "RESTART"] {
        assert!(pane.contains(label), "pane:\n{pane}");
    }
    assert!(!pane.contains("Enter runs"), "pane:\n{pane}");
}

#[test]
fn dashboard_shows_all_seven_sessions_in_fixture_sized_terminal() {
    let env = TestEnv::new("dashboard-seven-sessions");

    let names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"];
    for name in names {
        let dir = env.root_dir().join(name);
        fs::create_dir_all(&dir).expect("test should create session dir");
        create_saved_alias(&env, name, &dir);
    }

    let parent_session_name = format!("{}dashboard", env.tmux_prefix());
    create_tmux_session_in_dir_with_size(&parent_session_name, env.root_dir(), 120, 60);
    launch_dashboard(&env, &parent_session_name);

    let pane = wait_for_tmux_pane_contains(
        &parent_session_name,
        "total sessions",
        Duration::from_secs(10),
    );
    assert!(pane.contains("7"), "pane:\n{pane}");
    assert!(pane.contains("total sessions"), "pane:\n{pane}");
}
