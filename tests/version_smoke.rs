mod common;

use predicates::prelude::*;
use serde_json::Value;

const EXPECTED_VERSION_OUTPUT: &str = concat!("oc ", env!("CARGO_PKG_VERSION"), "\n");

#[test]
fn version_flag_prints_package_version() {
    common::oc_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_VERSION_OUTPUT));
}

#[test]
fn version_json_flag_prints_machine_readable_version() {
    let assert = common::oc_cmd()
        .args(["--version", "--json"])
        .assert()
        .success();

    let value: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("stdout should be valid JSON");
    assert_eq!(value["package"], "oc");
    assert_eq!(value["binary"], "oc");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
}
