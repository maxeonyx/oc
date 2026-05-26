mod common;

use common::TestEnv;
use predicates::prelude::*;

#[test]
fn completion_fish_prints_non_empty_script() {
    common::oc_cmd()
        .args(["completion", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c oc"));
}

#[test]
fn completion_fish_uses_hand_written_top_level_ordering() {
    let env = TestEnv::new("completion_fish_uses_hand_written_top_level_ordering");

    env.oc_cmd()
        .args([
            "alias",
            "beta",
            env.root_dir().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    env.oc_cmd()
        .args([
            "alias",
            "alpha",
            env.root_dir().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let output = String::from_utf8(
        env.oc_cmd()
            .args(["completion", "fish"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("fish completion should be utf-8");

    assert!(output.contains("complete -c oc -f\n"));
    assert!(output.contains(
        "complete -c oc -n '__fish_oc_needs_command' -k -a \"(__fish_oc_session_names)\""
    ));
    assert!(output.contains("command oc __dump-session-list 2>/dev/null"));
    assert!(output.contains("complete -c oc -n '__fish_oc_using_subcommand mv; and __fish_is_nth_token 3' -r -a '(__fish_complete_directories)'"));

    let sessions_pos = output
        .find("complete -c oc -n '__fish_oc_needs_command' -k -a \"(__fish_oc_session_names)\"")
        .expect("session completion should exist");
    let subcommands_pos = output
        .find("complete -c oc -n '__fish_oc_needs_command' -k -a '")
        .expect("subcommand completion should exist");
    assert!(
        sessions_pos < subcommands_pos,
        "sessions must be defined before subcommands for fish -k ordering"
    );
    assert!(!output.contains("__dump-runtime-config"));
    assert!(!output.contains("__parse-memory-status"));
}

#[test]
fn dump_session_list_outputs_names_only() {
    let env = TestEnv::new("dump_session_list_outputs_names_only");

    env.oc_cmd()
        .args([
            "alias",
            "beta",
            env.root_dir().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    env.oc_cmd()
        .args([
            "alias",
            "alpha",
            env.root_dir().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    env.oc_cmd()
        .args(["__dump-session-list"])
        .assert()
        .success()
        .stdout("alpha\nbeta\n");
}
