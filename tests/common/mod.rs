#![allow(dead_code)]

use assert_cmd::Command;
use rand::Rng;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const TEST_TMUX_FAMILY_PREFIX: &str = "oc-test-";

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

struct WaitStatus<T> {
    value: Option<T>,
    observed: String,
}

impl<T> WaitStatus<T> {
    fn ready(value: T, observed: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            observed: observed.into(),
        }
    }

    fn pending(observed: impl Into<String>) -> Self {
        Self {
            value: None,
            observed: observed.into(),
        }
    }
}

fn wait_until<T, F>(
    description: &str,
    timeout: Duration,
    poll_interval: Duration,
    mut probe: F,
) -> T
where
    F: FnMut() -> WaitStatus<T>,
{
    let deadline = Instant::now() + timeout;

    loop {
        let status = probe();

        if let Some(value) = status.value {
            return value;
        }

        if Instant::now() >= deadline {
            panic!(
                "Timed out waiting for {} after {:?}\nlast observed:\n{}",
                description, timeout, status.observed
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

fn sanitize_scope_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();

    sanitized.trim_matches('-').to_string()
}

fn run_tmux_output(args: &[&str], description: &str) -> Output {
    StdCommand::new("tmux")
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("Failed to {}: {}", description, error))
}

pub fn create_tmux_session(session_name: &str) {
    let output = run_tmux_output(
        &["new-session", "-d", "-s", session_name],
        "create tmux session for test",
    );

    if !output.status.success() {
        panic!(
            "Failed to create tmux session {}\nstdout:\n{}\nstderr:\n{}",
            session_name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn tmux_session_exists(name: &str) -> bool {
    StdCommand::new("tmux")
        .args(["has-session", "-t", name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn list_tmux_sessions_with_prefix(prefix: &str) -> Vec<String> {
    let output = run_tmux_output(
        &["list-sessions", "-F", "#{session_name}"],
        "list tmux sessions",
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stderr.contains("no server running")
            || (stderr.contains("error connecting to")
                && stderr.contains("No such file or directory"))
        {
            return Vec::new();
        }

        panic!(
            "Failed to list tmux sessions\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with(prefix))
        .map(String::from)
        .collect()
}

pub fn cleanup_tmux_sessions_with_prefix(prefix: &str) {
    for session_name in list_tmux_sessions_with_prefix(prefix) {
        let output = run_tmux_output(
            &["kill-session", "-t", &session_name],
            "kill tmux session during cleanup",
        );

        if !output.status.success() && tmux_session_exists(&session_name) {
            panic!(
                "Failed to clean up tmux session {}\nstdout:\n{}\nstderr:\n{}",
                session_name,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

pub fn wait_for_tmux_session_exists(session_name: &str, timeout: Duration) {
    let description = format!("tmux session {} to exist", session_name);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let exists = tmux_session_exists(session_name);
        let observed = format!("session {} exists: {}", session_name, exists);

        if exists {
            WaitStatus::ready((), observed)
        } else {
            WaitStatus::pending(observed)
        }
    });
}

pub fn wait_for_tmux_session_absent(session_name: &str, timeout: Duration) {
    let description = format!("tmux session {} to be absent", session_name);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let exists = tmux_session_exists(session_name);
        let observed = format!("session {} exists: {}", session_name, exists);

        if exists {
            WaitStatus::pending(observed)
        } else {
            WaitStatus::ready((), observed)
        }
    });
}

pub fn wait_for_file_contains(path: &Path, needle: &str, timeout: Duration) -> String {
    let description = format!("file {} to contain {}", path.display(), needle);

    wait_until(
        &description,
        timeout,
        DEFAULT_POLL_INTERVAL,
        || match fs::read_to_string(path) {
            Ok(contents) => {
                if contents.contains(needle) {
                    WaitStatus::ready(contents.clone(), contents)
                } else {
                    WaitStatus::pending(contents)
                }
            }
            Err(error) => WaitStatus::pending(format!("read error: {}", error)),
        },
    )
}

pub struct TestEnv {
    root_dir: PathBuf,
    aliases_file: PathBuf,
    opencode_db: PathBuf,
    tmux_scope_prefix: String,
    tmux_prefix: String,
}

impl TestEnv {
    pub fn new(scope_name: &str) -> Self {
        let tmux_scope_prefix = tmux_scope_prefix(scope_name);
        cleanup_tmux_sessions_with_prefix(&tmux_scope_prefix);

        let root_dir = env::temp_dir()
            .join("oc-tests")
            .join(sanitize_scope_name(scope_name))
            .join(unique_token());
        let config_dir = root_dir.join("config/oc");
        let data_dir = root_dir.join("data/opencode");

        fs::create_dir_all(&config_dir)
            .unwrap_or_else(|error| panic!("Failed to create {}: {}", config_dir.display(), error));
        fs::create_dir_all(&data_dir)
            .unwrap_or_else(|error| panic!("Failed to create {}: {}", data_dir.display(), error));

        Self {
            aliases_file: config_dir.join("aliases"),
            opencode_db: data_dir.join("session-store.sqlite"),
            tmux_prefix: format!("{}{}-", tmux_scope_prefix, unique_token()),
            tmux_scope_prefix,
            root_dir,
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn aliases_file(&self) -> &Path {
        &self.aliases_file
    }

    pub fn opencode_db(&self) -> &Path {
        &self.opencode_db
    }

    pub fn tmux_prefix(&self) -> &str {
        &self.tmux_prefix
    }

    pub fn scope_tmux_prefix(scope_name: &str) -> String {
        tmux_scope_prefix(scope_name)
    }

    pub fn oc_cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("oc").expect("oc binary should build for tests");
        cmd.env("OC_ALIASES_FILE", &self.aliases_file)
            .env("OC_TMUX_PREFIX", &self.tmux_prefix)
            .env("OC_OPENCODE_DB", &self.opencode_db);
        cmd
    }

    pub fn list_tmux_sessions(&self) -> Vec<String> {
        list_tmux_sessions_with_prefix(&self.tmux_scope_prefix)
    }

    pub fn create_tmux_session(&self, suffix: &str) -> String {
        let session_name = format!("{}{}", self.tmux_prefix, sanitize_scope_name(suffix));
        create_tmux_session(&session_name);

        session_name
    }

    pub fn wait_for_tmux_session_exists(&self, session_name: &str) {
        wait_for_tmux_session_exists(session_name, DEFAULT_WAIT_TIMEOUT);
    }

    pub fn wait_for_tmux_session_absent(&self, session_name: &str) {
        wait_for_tmux_session_absent(session_name, DEFAULT_WAIT_TIMEOUT);
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        cleanup_tmux_sessions_with_prefix(&self.tmux_scope_prefix);
        let _ = fs::remove_dir_all(&self.root_dir);
    }
}

pub fn oc_cmd() -> Command {
    Command::cargo_bin("oc").expect("oc binary should build for tests")
}

fn tmux_scope_prefix(scope_name: &str) -> String {
    format!(
        "{}{}-",
        TEST_TMUX_FAMILY_PREFIX,
        sanitize_scope_name(scope_name)
    )
}
