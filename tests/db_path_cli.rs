mod common;

use predicates::prelude::*;

use common::TestEnv;

#[test]
fn db_path_prints_resolved_session_db_path() {
    let env = TestEnv::new("db-path-cli");
    let expected = format!("{}\n", env.aliases_file().display());

    env.oc_cmd()
        .arg("db-path")
        .assert()
        .success()
        .stdout(predicate::eq(expected))
        .stderr(predicate::str::is_empty());
}
