mod common;

use common::{TestEnv, wait_for_file_contains, wait_for_tmux_session_absent};
use predicates::prelude::*;
use std::fs;
use std::time::Duration;

#[test]
fn test_env_injects_runtime_overrides_into_oc() {
    let env = TestEnv::new("runtime-config-injection");

    env.oc_cmd()
        .arg("__dump-runtime-config")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "aliases_file={}\n",
            env.aliases_file().display()
        )))
        .stdout(predicate::str::contains(format!(
            "tmux_prefix={}\n",
            env.tmux_prefix()
        )))
        .stdout(predicate::str::contains(format!(
            "opencode_db={}\n",
            env.opencode_db().display()
        )));
}

#[test]
fn test_env_cleans_up_stale_tmux_sessions_before_use() {
    let scope_name = "pre-cleanup";
    let stale_session = format!("{}stale", TestEnv::scope_tmux_prefix(scope_name));

    let bootstrap = TestEnv::new("pre-cleanup-bootstrap");
    let _ = bootstrap.create_tmux_session("placeholder");

    let output = std::process::Command::new("tmux")
        .args(["new-session", "-d", "-s", &stale_session])
        .output()
        .expect("Failed to create stale tmux session");
    assert!(
        output.status.success(),
        "Failed to create stale tmux session\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let env = TestEnv::new(scope_name);

    env.wait_for_tmux_session_absent(&stale_session);
    assert!(
        env.list_tmux_sessions().is_empty(),
        "Expected stale tmux sessions to be removed before test setup"
    );
}

#[test]
fn test_env_drop_cleans_up_tmux_sessions() {
    let created_session = {
        let env = TestEnv::new("drop-cleans-sessions");
        let session_name = env.create_tmux_session("managed");
        env.wait_for_tmux_session_exists(&session_name);
        session_name
    };

    wait_for_tmux_session_absent(&created_session, Duration::from_secs(10));
}

#[test]
fn wait_for_file_contains_observes_eventual_file_content() {
    let env = TestEnv::new("file-observability");
    let file_path = env.root_dir().join("eventual.txt");

    std::thread::spawn({
        let file_path = file_path.clone();
        move || {
            std::thread::sleep(Duration::from_millis(200));
            fs::write(&file_path, "ready\n").expect("writer thread should create eventual file");
        }
    });

    let contents = wait_for_file_contains(&file_path, "ready", Duration::from_secs(5));
    assert!(contents.contains("ready"));
}
