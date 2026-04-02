#![allow(dead_code)]

//! Shared test utilities for black-box E2E tests.
//!
//! Helpers here always use real tmux sessions named `oc-<name>` and clean up on
//! drop so future session-management tests can run safely in parallel.

use assert_cmd::Command;
use rand::Rng;
use std::process::Command as StdCommand;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const TEST_NAME_PREFIX: &str = "octest-";
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct WaitStatus<T> {
    value: Option<T>,
    observed: String,
}

impl<T> WaitStatus<T> {
    pub fn ready(value: T, observed: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            observed: observed.into(),
        }
    }

    pub fn pending(observed: impl Into<String>) -> Self {
        Self {
            value: None,
            observed: observed.into(),
        }
    }
}

pub fn wait_until<T, F>(
    description: &str,
    timeout: Duration,
    poll_interval: Duration,
    mut probe: F,
) -> T
where
    F: FnMut() -> WaitStatus<T>,
{
    let deadline = Instant::now() + timeout;
    let mut last_observed = String::from("<nothing observed>");

    loop {
        let status = probe();
        last_observed = status.observed;

        if let Some(value) = status.value {
            return value;
        }

        if Instant::now() >= deadline {
            panic!(
                "Timed out waiting for {} after {:?}\nlast observed:\n{}",
                description, timeout, last_observed
            );
        }

        thread::sleep(poll_interval);
    }
}

fn unique_token() -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut rng = rand::thread_rng();
    format!(
        "{}-{}-{:08x}",
        std::process::id(),
        counter,
        rng.r#gen::<u32>()
    )
}

pub fn unique_session_name() -> String {
    format!("{}{}", TEST_NAME_PREFIX, unique_token())
}

pub fn tmux_session_name(name: &str) -> String {
    format!("oc-{}", name)
}

pub fn oc_cmd() -> Command {
    Command::cargo_bin("oc").expect("oc binary should build for tests")
}

fn run_tmux_output(args: &[&str], description: &str) -> Output {
    StdCommand::new("tmux")
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("Failed to {}: {}", description, error))
}

pub fn session_exists(name: &str) -> bool {
    StdCommand::new("tmux")
        .args(["has-session", "-t", &tmux_session_name(name)])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn capture_pane_content(target: &str) -> String {
    let output = run_tmux_output(&["capture-pane", "-t", target, "-p"], "capture tmux pane");

    if !output.status.success() {
        panic!(
            "Failed to capture tmux pane {}\nstdout:\n{}\nstderr:\n{}",
            target,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn wait_for_session_exists(name: &str, timeout: Duration) {
    let tmux_name = tmux_session_name(name);
    let description = format!("tmux session {} to exist", tmux_name);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let output = run_tmux_output(&["has-session", "-t", &tmux_name], "check tmux session");
        let observed = format!(
            "tmux has-session exit status: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        if output.status.success() {
            WaitStatus::ready((), observed)
        } else {
            WaitStatus::pending(observed)
        }
    });
}

pub fn wait_for_pane_content<F>(name: &str, description: &str, mut predicate: F) -> String
where
    F: FnMut(&str) -> bool,
{
    let target = tmux_session_name(name);

    wait_until(
        description,
        DEFAULT_WAIT_TIMEOUT,
        DEFAULT_POLL_INTERVAL,
        || {
            let content = capture_pane_content(&target);
            if predicate(&content) {
                WaitStatus::ready(content.clone(), content)
            } else {
                WaitStatus::pending(content)
            }
        },
    )
}

pub struct TestSession {
    pub name: String,
}

impl TestSession {
    pub fn new() -> Self {
        let name = unique_session_name();
        let tmux_name = tmux_session_name(&name);

        let status = StdCommand::new("tmux")
            .args(["new-session", "-d", "-s", &tmux_name])
            .status()
            .expect("Failed to create tmux session");

        assert!(
            status.success(),
            "Failed to create tmux session {tmux_name}"
        );

        Self { name }
    }

    pub fn tmux_name(&self) -> String {
        tmux_session_name(&self.name)
    }

    pub fn wait_for_exists(&self) {
        wait_for_session_exists(&self.name, DEFAULT_WAIT_TIMEOUT);
    }
}

impl Drop for TestSession {
    fn drop(&mut self) {
        let _ = StdCommand::new("tmux")
            .args(["kill-session", "-t", &self.tmux_name()])
            .output();
    }
}
