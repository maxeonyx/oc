use assert_cmd::cargo::cargo_bin;
use help_test::HelpTest;
use std::process::Command;

#[test]
fn help_examples() {
    panic!("pending: wire oc help pages into workspace help-test coverage");
}

fn help_output(command_path: &[&str]) -> String {
    let output = Command::new(cargo_bin("oc"))
        .args(command_path)
        .arg("--help")
        .output()
        .expect("oc --help should run");

    assert!(
        output.status.success(),
        "oc {:?} --help should succeed\nstdout:\n{}\nstderr:\n{}",
        command_path,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("help output should be valid UTF-8")
}

fn assert_examples_use_long_flags_only(help: &str) {
    for line in help.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("$ oc ") {
            continue;
        }

        let words = trimmed
            .split_whitespace()
            .take_while(|word| !word.starts_with('#'))
            .collect::<Vec<_>>();
        for word in words.into_iter().skip(2) {
            assert!(
                !is_short_flag(word),
                "help example should use long flags only: {trimmed}"
            );
        }
    }
}

fn is_short_flag(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some('-'), Some(letter), None) if letter.is_ascii_alphabetic()
    )
}

fn tmux_is_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
}
