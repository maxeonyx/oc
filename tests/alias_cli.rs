mod common;

use common::{SavedSessionRow, TestEnv, read_saved_sessions, saved_session_row};
use predicates::prelude::*;
use std::fs;

const EMPTY_ARGS_JSON: &str = "[]";

fn alias_in_root_dir(env: &TestEnv, name: &str) {
    env.oc_cmd()
        .current_dir(env.root_dir())
        .args(["alias", name])
        .assert()
        .success();
}

fn assert_saved_sessions(env: &TestEnv, expected_rows: Vec<SavedSessionRow>) {
    assert_eq!(read_saved_sessions(env.aliases_file()), expected_rows);
}

#[test]
fn alias_creates_db_and_inserts_session_with_default_dir() {
    let env = TestEnv::new("alias-default-dir");

    env.oc_cmd()
        .current_dir(env.root_dir())
        .args(["alias", "worktree"])
        .assert()
        .success();

    assert!(
        env.aliases_file().exists(),
        "Expected alias command to create SQLite database at {}",
        env.aliases_file().display()
    );

    assert_saved_sessions(
        &env,
        vec![saved_session_row(
            1,
            "worktree",
            env.root_dir(),
            EMPTY_ARGS_JSON,
        )],
    );
}

#[test]
fn alias_uses_explicit_dir_and_captures_opencode_args_after_double_dash() {
    let env = TestEnv::new("alias-explicit-dir-and-args");
    let project_dir = env.root_dir().join("project");
    fs::create_dir_all(&project_dir).expect("test should create explicit project directory");

    env.oc_cmd()
        .args([
            "alias",
            "dc",
            project_dir
                .to_str()
                .expect("project dir should be valid UTF-8 for test"),
            "--",
            "--model",
            "gpt-5.4",
            "--sandbox",
            "read-only",
        ])
        .assert()
        .success();

    assert_saved_sessions(
        &env,
        vec![saved_session_row(
            1,
            "dc",
            &project_dir,
            "[\"--model\",\"gpt-5.4\",\"--sandbox\",\"read-only\"]",
        )],
    );
}

#[test]
fn alias_expands_tilde_directory_before_storing() {
    let env = TestEnv::new("alias-expands-tilde-dir");
    let fake_home = env.root_dir().join("home");
    let project_dir = fake_home.join("project");
    fs::create_dir_all(&project_dir).expect("test should create fake home project directory");

    env.oc_cmd()
        .env("HOME", &fake_home)
        .args(["alias", "dc", "~/project"])
        .assert()
        .success();

    assert_saved_sessions(
        &env,
        vec![saved_session_row(1, "dc", &project_dir, EMPTY_ARGS_JSON)],
    );
}

#[test]
fn alias_rejects_plain_numeric_name() {
    let env = TestEnv::new("alias-rejects-numeric-name");

    env.oc_cmd()
        .args(["alias", "123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("plain number"));
}

#[test]
fn alias_rejects_duplicate_name() {
    let env = TestEnv::new("alias-rejects-duplicate-name");

    alias_in_root_dir(&env, "dc");

    env.oc_cmd()
        .current_dir(env.root_dir())
        .args(["alias", "dc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    assert_saved_sessions(
        &env,
        vec![saved_session_row(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
    );
}

#[test]
fn alias_assigns_dense_gap_filling_ids() {
    let env = TestEnv::new("alias-gap-filling-ids");

    alias_in_root_dir(&env, "one");
    alias_in_root_dir(&env, "two");
    alias_in_root_dir(&env, "three");
    env.oc_cmd().args(["unalias", "two"]).assert().success();
    alias_in_root_dir(&env, "four");

    assert_saved_sessions(
        &env,
        vec![
            saved_session_row(1, "one", env.root_dir(), EMPTY_ARGS_JSON),
            saved_session_row(2, "four", env.root_dir(), EMPTY_ARGS_JSON),
            saved_session_row(3, "three", env.root_dir(), EMPTY_ARGS_JSON),
        ],
    );
}

#[test]
fn unalias_removes_mapping_by_name() {
    let env = TestEnv::new("unalias-removes-mapping");

    alias_in_root_dir(&env, "dc");

    env.oc_cmd().args(["unalias", "dc"]).assert().success();

    assert!(
        read_saved_sessions(env.aliases_file()).is_empty(),
        "Expected unalias to remove saved session row"
    );
}

#[test]
fn unalias_missing_name_fails_cleanly() {
    let env = TestEnv::new("unalias-missing-name");

    env.oc_cmd()
        .args(["unalias", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
