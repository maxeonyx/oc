use assert_cmd::cargo::cargo_bin;
use help_test::HelpTest;
use std::process::Command;

#[test]
fn help_examples() {
    if !tmux_is_available() {
        eprintln!("skipping help_examples: tmux not available");
        return;
    }

    let top_level_help = help_output(&[]);

    assert!(
        top_level_help.contains("[TARGET] is a session name or alias to attach to."),
        "top-level help should explain [TARGET]\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("Use bare `oc` to open the TUI session manager."),
        "top-level help should explain the bare oc workflow\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("Use `oc <name>` to attach to a tracked session."),
        "top-level help should explain the attach workflow\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("$ oc                           # Open the TUI session manager"),
        "top-level help should include the bare oc example\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("$ oc my-project                # Attach to session 'my-project'"),
        "top-level help should include the attach example\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("$ oc new my-project ~/src/foo  # Create a new session"),
        "top-level help should include the new-session example\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("$ oc list                      # List all tracked sessions"),
        "top-level help should include the list example\n{top_level_help}"
    );
    assert!(
        top_level_help.contains("$ oc list --json               # List as JSON (for scripting)"),
        "top-level help should include the json list example\n{top_level_help}"
    );
    assert_examples_use_long_flags_only(&top_level_help);

    let list_output = Command::new(cargo_bin("oc"))
        .args(["list"])
        .output()
        .expect("oc list should run");
    assert!(
        list_output.status.success(),
        "oc list should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&list_output.stdout),
        String::from_utf8_lossy(&list_output.stderr)
    );

    let list_json_output = Command::new(cargo_bin("oc"))
        .args(["list", "--json"])
        .output()
        .expect("oc list --json should run");
    assert!(
        list_json_output.status.success(),
        "oc list --json should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&list_json_output.stdout),
        String::from_utf8_lossy(&list_json_output.stderr)
    );

    HelpTest::new("oc")
        .display_command(&["__help_test_ignores_non_executable_root_examples__"])
        .page(&[], |_fixture| {})
        .page(&["completion"], |_fixture| {})
        .page(&["new"], |_fixture| {})
        .page(&["alias"], |_fixture| {})
        .page(&["unalias"], |_fixture| {})
        .page(&["rm"], |_fixture| {})
        .page(&["stop"], |_fixture| {})
        .page(&["restart"], |_fixture| {})
        .page(&["mv"], |_fixture| {})
        .page(&["migrate"], |_fixture| {})
        .page(&["list"], |_fixture| {})
        .page(&["db-path"], |_fixture| {})
        .run();
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
