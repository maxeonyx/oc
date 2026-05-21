mod common;

use predicates::prelude::*;

#[test]
fn completion_fish_prints_non_empty_script() {
    common::oc_cmd()
        .args(["completion", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c oc"));
}
