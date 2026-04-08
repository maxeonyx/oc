mod common;

use common::TestEnv;
use predicates::prelude::*;
use rusqlite::{Connection, OpenFlags, params};
use std::fs;
use std::path::{Path, PathBuf};

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
        vec![saved_session(
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
        vec![saved_session(
            1,
            "dc",
            &project_dir,
            "[\"--model\",\"gpt-5.4\",\"--sandbox\",\"read-only\"]",
        )],
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
        vec![saved_session(1, "dc", env.root_dir(), EMPTY_ARGS_JSON)],
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
            saved_session(1, "one", env.root_dir(), EMPTY_ARGS_JSON),
            saved_session(2, "four", env.root_dir(), EMPTY_ARGS_JSON),
            saved_session(3, "three", env.root_dir(), EMPTY_ARGS_JSON),
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
