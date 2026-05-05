mod common;

use common::{read_saved_sessions, TestEnv};
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

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
fn migrate_preserves_legacy_directory_and_session_fields() {
    let env = TestEnv::new("migrate-legacy-fields");
    let legacy_aliases = env.root_dir().join("legacy-aliases");
    fs::write(
        &legacy_aliases,
        concat!(
            "two\t/tmp/two\n",
            "three\t/tmp/three\t--session ses_123 extra --flag\n",
            "trailing\t/tmp/trailing\t\n",
        ),
    )
    .expect("should write legacy aliases file");

    env.oc_cmd()
        .env("OC_LEGACY_ALIASES_FILE", &legacy_aliases)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("imported 3"));

    env.oc_cmd()
        .env("OC_LEGACY_ALIASES_FILE", &legacy_aliases)
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped 3"));

    let saved = read_saved_sessions(env.aliases_file());
    assert_eq!(saved.len(), 3);

    assert_eq!(saved[0].name, "two");
    assert_eq!(saved[0].directory, PathBuf::from("/tmp/two"));
    assert_eq!(saved[0].opencode_session_id, None);
    assert_eq!(saved[0].opencode_args, "[]");

    assert_eq!(saved[1].name, "three");
    assert_eq!(saved[1].directory, PathBuf::from("/tmp/three"));
    assert_eq!(saved[1].opencode_session_id.as_deref(), Some("ses_123"));
    assert_eq!(saved[1].opencode_args, r#"["extra","--flag"]"#);

    assert_eq!(saved[2].name, "trailing");
    assert_eq!(saved[2].directory, PathBuf::from("/tmp/trailing"));
    assert_eq!(saved[2].opencode_session_id, None);
    assert_eq!(saved[2].opencode_args, "[]");
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
fn opening_existing_db_migrates_tilde_directories_to_absolute_paths() {
    let env = TestEnv::new("migrate-existing-tilde-directories");
    let fake_home = env.root_dir().join("home");
    let project_dir = fake_home.join("project");

    fs::create_dir_all(&project_dir).expect("test should create fake home project directory");

    common::create_legacy_sessions_db(
        env.aliases_file(),
        &[
            common::SavedSessionRow {
                id: 1,
                name: String::from("meta"),
                directory: PathBuf::from("~"),
                opencode_session_id: None,
                opencode_args: String::from("[]"),
                last_used_at: 0,
            },
            common::SavedSessionRow {
                id: 2,
                name: String::from("dc"),
                directory: PathBuf::from("~/project"),
                opencode_session_id: None,
                opencode_args: String::from("[]"),
                last_used_at: 0,
            },
        ],
    );

    env.oc_cmd()
        .env("HOME", &fake_home)
        .args(["list"])
        .assert()
        .success();

    let saved = read_saved_sessions(env.aliases_file());
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].directory, fake_home);
    assert_eq!(saved[1].directory, project_dir);
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
