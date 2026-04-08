mod common;

use common::{TestEnv, read_saved_sessions};
use predicates::prelude::*;
use std::fs;

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
