mod common;

use predicates::prelude::*;

#[test]
fn version_flag_prints_package_version() {
    common::oc_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("oc 0.1.1\n"));
}
