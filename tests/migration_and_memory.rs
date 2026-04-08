mod common;

use common::TestEnv;
use predicates::prelude::*;
use rusqlite::{Connection, OpenFlags, params};
use std::fs;
use std::path::{Path, PathBuf};

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

#[test]
fn migrate_imports_old_aliases_file_idempotently() {
    let env = TestEnv::new("migrate-idempotent");
    let legacy_aliases = env.root_dir().join("legacy-aliases");
    fs::write(&legacy_aliases, "alpha\t/tmp/a\nbeta\t/tmp/b\n")
        .expect("should write legacy aliases file");

    env.oc_cmd()
        .env("OC_LEGACY_ALIASES_FILE", &legacy_aliases)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("imported 2"));

    env.oc_cmd()
        .env("OC_LEGACY_ALIASES_FILE", &legacy_aliases)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped 2"));

    assert_eq!(read_saved_sessions(env.aliases_file()).len(), 2);
}

#[test]
fn migrate_reports_conflicts_without_overwriting() {
    let env = TestEnv::new("migrate-conflicts");
    let legacy_aliases = env.root_dir().join("legacy-aliases");
    fs::write(&legacy_aliases, "alpha\t/tmp/legacy\n").expect("should write legacy aliases file");

    env.oc_cmd()
        .current_dir(env.root_dir())
        .args(["alias", "alpha"])
        .assert()
        .success();

    env.oc_cmd()
        .env("OC_LEGACY_ALIASES_FILE", &legacy_aliases)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("conflicts 1"));

    let saved = read_saved_sessions(env.aliases_file());
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].directory, env.root_dir());
}

#[test]
fn hidden_memory_parser_reads_proc_status_kib() {
    let env = TestEnv::new("memory-parser");
    let status_file = env.root_dir().join("status");
    fs::write(&status_file, "Name:\toc\nVmRSS:\t535552 kB\n")
        .expect("should write fake proc status file");

    env.oc_cmd()
        .args([
            "__parse-memory-status",
            status_file
                .to_str()
                .expect("status file should be valid UTF-8"),
        ])
        .assert()
        .success()
        .stdout("548405248\n");
}
