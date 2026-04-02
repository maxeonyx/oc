mod common;

use predicates::prelude::*;

const EXPECTED_VERSION_OUTPUT: &str = concat!("oc ", env!("CARGO_PKG_VERSION"), "\n");

#[test]
fn version_flag_prints_package_version() {
    common::oc_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_VERSION_OUTPUT));
}
