#![allow(dead_code)]

use assert_cmd::Command;
use rand::Rng;
use rusqlite::{params, Connection, OpenFlags};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Output};
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

fn run_tmux_success(args: &[&str], description: &str) -> Output {
    let output = run_tmux_output(args, description);

    if !output.status.success() {
        panic!(
            "Failed to {}\nstdout:\n{}\nstderr:\n{}",
            description,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    output
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

pub fn create_tmux_session_in_dir(session_name: &str, directory: &Path) {
    let directory = directory
        .to_str()
        .unwrap_or_else(|| panic!("Directory should be valid UTF-8: {}", directory.display()));
    let output = run_tmux_output(
        &["new-session", "-d", "-s", session_name, "-c", directory],
        "create tmux session for test in directory",
    );

    if !output.status.success() {
        panic!(
            "Failed to create tmux session {} in {}\nstdout:\n{}\nstderr:\n{}",
            session_name,
            directory,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

pub fn create_tmux_session_in_dir_with_size(
    session_name: &str,
    directory: &Path,
    width: u16,
    height: u16,
) {
    let directory = directory
        .to_str()
        .unwrap_or_else(|| panic!("Directory should be valid UTF-8: {}", directory.display()));
    let output = run_tmux_output(
        &[
            "new-session",
            "-d",
            "-x",
            &width.to_string(),
            "-y",
            &height.to_string(),
            "-s",
            session_name,
            "-c",
            directory,
        ],
        "create tmux session for test in directory with size",
    );

    if !output.status.success() {
        panic!(
            "Failed to create tmux session {} in {} sized {}x{}\nstdout:\n{}\nstderr:\n{}",
            session_name,
            directory,
            width,
            height,
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

        if is_tmux_server_unavailable_error(&stderr) {
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

pub fn wait_for_file_exists(path: &Path, timeout: Duration) {
    let description = format!("file {} to exist", path.display());

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let exists = path.exists();
        let observed = format!("path {} exists: {}", path.display(), exists);

        if exists {
            WaitStatus::ready((), observed)
        } else {
            WaitStatus::pending(observed)
        }
    });
}

pub fn tmux_pane_current_command(session_name: &str) -> String {
    let current_command = tmux_display_message(session_name, "#{pane_current_command}");

    if current_command == "sh" {
        return tmux_display_message(session_name, "#{pane_start_command}");
    }

    current_command
}

pub fn capture_tmux_pane(session_name: &str) -> String {
    let output = run_tmux_success(
        &["capture-pane", "-p", "-t", session_name],
        "capture tmux pane",
    );

    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn send_keys_to_tmux_session(session_name: &str, keys: &[&str]) {
    let mut args = vec!["send-keys", "-t", session_name];
    args.extend(keys);
    run_tmux_success(&args, "send keys to tmux session");
}

pub fn wait_for_tmux_pane_contains(session_name: &str, needle: &str, timeout: Duration) -> String {
    let description = format!("tmux pane {} to contain {}", session_name, needle);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let contents = capture_tmux_pane(session_name);
        if contents.contains(needle) {
            WaitStatus::ready(contents.clone(), contents)
        } else {
            WaitStatus::pending(contents)
        }
    })
}

fn tmux_display_message(session_name: &str, format_string: &str) -> String {
    let output = run_tmux_success(
        &["display-message", "-p", "-t", session_name, format_string],
        "read tmux display message",
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn tmux_pane_pid(session_name: &str) -> u32 {
    let output = run_tmux_success(
        &["display-message", "-p", "-t", session_name, "#{pane_pid}"],
        "read tmux pane pid",
    );

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or_else(|error| panic!("Failed to parse tmux pane pid: {}", error))
}

pub fn detach_tmux_client_from_session(session_name: &str) {
    let output = run_tmux_output(
        &["detach-client", "-s", session_name],
        "detach tmux client from session",
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stderr.contains("no current client") || stderr.contains("can't find client") {
            return;
        }

        panic!(
            "Failed to detach tmux client from session {}\nstdout:\n{}\nstderr:\n{}",
            session_name,
            String::from_utf8_lossy(&output.stdout),
            stderr,
        );
    }
}

pub fn tmux_session_attached_count(session_name: &str) -> usize {
    let output = run_tmux_output(
        &[
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_attached}",
        ],
        "read tmux attached session count",
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_tmux_server_unavailable_error(&stderr) {
            return 0;
        }

        panic!(
            "Failed to read tmux attached session count\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let (name, attached_count) = line.split_once('\t')?;
            (name == session_name).then_some(attached_count)
        })
        .unwrap_or("0")
        .parse()
        .unwrap_or_else(|error| panic!("Failed to parse tmux attached count: {}", error))
}

fn is_tmux_server_unavailable_error(stderr: &str) -> bool {
    stderr.contains("no server running")
        || (stderr.contains("error connecting to") && stderr.contains("No such file or directory"))
        || stderr.contains("server exited unexpectedly")
}

pub fn wait_for_tmux_session_attached(session_name: &str, timeout: Duration) {
    let description = format!("tmux session {} to be attached", session_name);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let attached_count = tmux_session_attached_count(session_name);
        let observed = format!(
            "session {} attached count: {}",
            session_name, attached_count
        );

        if attached_count > 0 {
            WaitStatus::ready((), observed)
        } else {
            WaitStatus::pending(observed)
        }
    });
}

pub fn spawn_tmux_attach_client(session_name: &str) -> Child {
    let mut command = StdCommand::new("python3");
    command
        .arg("-c")
        .arg(
            "import os, pty, sys; pid, _ = pty.fork();\nif pid == 0: os.execvp('tmux', ['tmux', 'attach-session', '-t', sys.argv[1]]);\n_, status = os.waitpid(pid, 0); raise SystemExit(os.waitstatus_to_exitcode(status))",
        )
        .arg(session_name);

    if env::var_os("TERM").is_none() {
        command.env("TERM", "screen");
    }

    command.spawn().unwrap_or_else(|error| {
        panic!(
            "Failed to attach tmux client for {}: {}",
            session_name, error
        )
    })
}

pub fn wait_for_tmux_client_detach_window() {
    thread::sleep(Duration::from_millis(500));
}

pub struct TestEnv {
    root_dir: PathBuf,
    aliases_file: PathBuf,
    opencode_db: PathBuf,
    tmux_scope_prefix: String,
    tmux_prefix: String,
}

pub struct FakeOpenCode {
    bin_dir: PathBuf,
    log_dir: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SavedSessionRow {
    pub id: i64,
    pub name: String,
    pub directory: PathBuf,
    pub opencode_session_id: Option<String>,
    pub opencode_args: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpenCodeSessionRow {
    pub id: String,
    pub directory: PathBuf,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeProcessSessionRow {
    pub pid: u32,
    pub proc_start_ticks: u64,
    pub session_id: Option<String>,
    pub directory: PathBuf,
    pub updated_at: i64,
    pub reason: Option<String>,
}

const FAKE_OPENCODE_SCRIPT: &str = r#"#!/bin/sh
set -eu

log_dir="${OC_FAKE_OPENCODE_LOG_DIR:?}"
opencode_db="${OC_OPENCODE_DB:?}"
lifecycle_delay_ms="${OC_FAKE_OPENCODE_LIFECYCLE_DELAY_MS:-0}"
disable_session_table="${OC_FAKE_OPENCODE_DISABLE_SESSION_TABLE:-0}"
disable_process_session_table="${OC_FAKE_OPENCODE_DISABLE_PROCESS_SESSION_TABLE:-0}"
mode=new
session_id=
cleanup_done=0

pwd >"$log_dir/cwd.txt"
printf '%s\n' "$@" >"$log_dir/args.txt"
printf '%s\n' "$$" >"$log_dir/pid.txt"

generate_session_id() {
  python3 - <<'PY'
import uuid

print(f"ses_{uuid.uuid4().hex}")
PY
}

maybe_delay_lifecycle() {
  if [ "$lifecycle_delay_ms" -gt 0 ]; then
    python3 - "$lifecycle_delay_ms" <<'PY'
import sys
import time

time.sleep(int(sys.argv[1]) / 1000)
PY
  fi
}

write_session_row() {
  if [ "$disable_session_table" = "1" ]; then
    return
  fi
  python3 - "$opencode_db" "$PWD" "$1" <<'PY'
import sqlite3
import sys
import time
from pathlib import Path

db_path = Path(sys.argv[1])
directory = sys.argv[2]
session_id = sys.argv[3]

db_path.parent.mkdir(parents=True, exist_ok=True)
connection = sqlite3.connect(db_path)
try:
    connection.execute(
        '''
        CREATE TABLE IF NOT EXISTS session (
            id TEXT PRIMARY KEY NOT NULL,
            directory TEXT NOT NULL,
            parent_id TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        )
        '''
    )
    now = int(time.time())
    connection.execute(
        '''
        INSERT INTO session (id, directory, parent_id, time_created, time_updated)
        VALUES (?, ?, NULL, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            directory = excluded.directory,
            time_updated = excluded.time_updated
        ''',
        (session_id, directory, now, now),
    )
    connection.commit()
finally:
    connection.close()
PY
}

upsert_process_session() {
  if [ "$disable_process_session_table" = "1" ]; then
    return
  fi
  python3 - "$opencode_db" "$PWD" "$$" "$1" "${2-}" <<'PY'
import sqlite3
import sys
import time
from pathlib import Path

db_path = Path(sys.argv[1])
directory = sys.argv[2]
pid = int(sys.argv[3])
reason = sys.argv[4]
session_id = sys.argv[5] if len(sys.argv) > 5 and sys.argv[5] else None
proc_start_ticks = int(Path(f"/proc/{pid}/stat").read_text(encoding="utf-8").split()[21])

db_path.parent.mkdir(parents=True, exist_ok=True)
connection = sqlite3.connect(db_path)
try:
    connection.execute(
        '''
        CREATE TABLE IF NOT EXISTS process_session (
            pid INTEGER PRIMARY KEY,
            proc_start_ticks INTEGER NOT NULL,
            session_id TEXT,
            directory TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            reason TEXT
        )
        '''
    )
    updated_at = time.time_ns() // 1_000_000
    connection.execute(
        '''
        INSERT INTO process_session (pid, proc_start_ticks, session_id, directory, updated_at, reason)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(pid) DO UPDATE SET
            proc_start_ticks = excluded.proc_start_ticks,
            session_id = excluded.session_id,
            directory = excluded.directory,
            updated_at = excluded.updated_at,
            reason = excluded.reason
        ''',
        (pid, proc_start_ticks, session_id, directory, updated_at, reason),
    )
    connection.commit()
finally:
    connection.close()
PY
}

delete_process_session() {
  if [ "$disable_process_session_table" = "1" ]; then
    return
  fi
  python3 - "$opencode_db" "$$" <<'PY'
import sqlite3
import sys
from pathlib import Path

db_path = Path(sys.argv[1])
pid = int(sys.argv[2])

if not db_path.exists():
    raise SystemExit(0)

connection = sqlite3.connect(db_path)
try:
    connection.execute(
        "CREATE TABLE IF NOT EXISTS process_session (pid INTEGER PRIMARY KEY, proc_start_ticks INTEGER NOT NULL, session_id TEXT, directory TEXT NOT NULL, updated_at INTEGER NOT NULL, reason TEXT)"
    )
    connection.execute("DELETE FROM process_session WHERE pid = ?", (pid,))
    connection.commit()
finally:
    connection.close()
PY
}

cleanup() {
  if [ "$cleanup_done" -eq 1 ]; then
    return
  fi
  cleanup_done=1
  printf 'EXIT\n' >>"$log_dir/events.txt"
  delete_process_session || true
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --session)
      if [ "$#" -lt 2 ]; then
        printf '%s\n' 'fake opencode expected a session ID after --session' >&2
        exit 1
      fi
      mode=resume
      session_id=$2
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

printf 'START\n' >>"$log_dir/events.txt"
upsert_process_session startup
trap 'printf "INT\n" >>"$log_dir/events.txt"' INT
trap 'exit 0' TERM
trap 'cleanup' EXIT

if [ "$mode" = "new" ]; then
  maybe_delay_lifecycle
  session_id=$(generate_session_id)
  write_session_row "$session_id"
  upsert_process_session created "$session_id"
else
  maybe_delay_lifecycle
  write_session_row "$session_id"
  upsert_process_session resumed "$session_id"
fi

printf '%s\n' "$session_id" >"$log_dir/session-id.txt"
printf 'Fake OpenCode started: %s\n' "$session_id"

while IFS= read -r line; do
  printf 'LINE:%s\n' "$line" >>"$log_dir/events.txt"
done

printf 'EOF\n' >>"$log_dir/events.txt"
exit 0
"#;

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

    pub fn std_oc_cmd(&self) -> StdCommand {
        let mut cmd = StdCommand::new(assert_cmd::cargo::cargo_bin("oc"));
        cmd.env("OC_ALIASES_FILE", &self.aliases_file)
            .env("OC_TMUX_PREFIX", &self.tmux_prefix)
            .env("OC_OPENCODE_DB", &self.opencode_db)
            .current_dir(&self.root_dir);
        cmd
    }

    pub fn install_fake_opencode(&self) -> FakeOpenCode {
        let bin_dir = self.root_dir.join("bin");
        let log_dir = self.root_dir.join("fake-opencode-log");

        fs::create_dir_all(&bin_dir)
            .unwrap_or_else(|error| panic!("Failed to create {}: {}", bin_dir.display(), error));
        fs::create_dir_all(&log_dir)
            .unwrap_or_else(|error| panic!("Failed to create {}: {}", log_dir.display(), error));

        let script_path = bin_dir.join("opencode");
        fs::write(&script_path, FAKE_OPENCODE_SCRIPT)
            .unwrap_or_else(|error| panic!("Failed to write {}: {}", script_path.display(), error));

        let mut permissions = fs::metadata(&script_path)
            .unwrap_or_else(|error| panic!("Failed to stat {}: {}", script_path.display(), error))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).unwrap_or_else(|error| {
            panic!(
                "Failed to set executable permissions on {}: {}",
                script_path.display(),
                error
            )
        });

        FakeOpenCode { bin_dir, log_dir }
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

impl FakeOpenCode {
    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn cwd_log_path(&self) -> PathBuf {
        self.log_dir.join("cwd.txt")
    }

    pub fn args_log_path(&self) -> PathBuf {
        self.log_dir.join("args.txt")
    }

    pub fn pid_log_path(&self) -> PathBuf {
        self.log_dir.join("pid.txt")
    }

    pub fn events_log_path(&self) -> PathBuf {
        self.log_dir.join("events.txt")
    }

    pub fn session_id_log_path(&self) -> PathBuf {
        self.log_dir.join("session-id.txt")
    }

    pub fn apply_to_command(&self, command: &mut StdCommand) {
        command
            .env("PATH", prepend_path(&self.bin_dir))
            .env("OC_FAKE_OPENCODE_LOG_DIR", &self.log_dir);
    }

    pub fn apply_to_assert_cmd(&self, command: &mut Command) {
        command
            .env("PATH", prepend_path(&self.bin_dir))
            .env("OC_FAKE_OPENCODE_LOG_DIR", &self.log_dir);
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        cleanup_tmux_sessions_with_prefix(&self.tmux_scope_prefix);
        let _ = fs::remove_dir_all(&self.root_dir);
    }
}

pub fn read_saved_sessions(db_path: &Path) -> Vec<SavedSessionRow> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let mut statement = connection
        .prepare(
            "
            SELECT id, name, directory, opencode_session_id, opencode_args
            FROM sessions
            ORDER BY id
            ",
        )
        .expect("sessions table should be queryable");

    statement
        .query_map(params![], |row| {
            Ok(SavedSessionRow {
                id: row.get(0)?,
                name: row.get(1)?,
                directory: PathBuf::from(row.get::<_, String>(2)?),
                opencode_session_id: row.get(3)?,
                opencode_args: row.get(4)?,
            })
        })
        .expect("session rows should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("session rows should decode")
}

pub fn read_opencode_sessions(db_path: &Path) -> Vec<OpenCodeSessionRow> {
    if !db_path.exists() {
        return Vec::new();
    }

    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let mut statement = connection
        .prepare(
            "
            SELECT id, directory, parent_id
            FROM session
            ORDER BY id
            ",
        )
        .expect("OpenCode session table should be queryable");

    statement
        .query_map(params![], |row| {
            Ok(OpenCodeSessionRow {
                id: row.get(0)?,
                directory: PathBuf::from(row.get::<_, String>(1)?),
                parent_id: row.get(2)?,
            })
        })
        .expect("OpenCode session rows should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("OpenCode session rows should decode")
}

pub fn read_opencode_process_sessions(db_path: &Path) -> Vec<OpenCodeProcessSessionRow> {
    if !db_path.exists() {
        return Vec::new();
    }

    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let table_exists = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'process_session'",
            params![],
            |_| Ok(()),
        )
        .is_ok();

    if !table_exists {
        return Vec::new();
    }

    let mut statement = connection
        .prepare(
            "
            SELECT pid, proc_start_ticks, session_id, directory, updated_at, reason
            FROM process_session
            ORDER BY pid
            ",
        )
        .expect("OpenCode process_session table should be queryable");

    statement
        .query_map(params![], |row| {
            Ok(OpenCodeProcessSessionRow {
                pid: row.get(0)?,
                proc_start_ticks: row.get(1)?,
                session_id: row.get(2)?,
                directory: PathBuf::from(row.get::<_, String>(3)?),
                updated_at: row.get(4)?,
                reason: row.get(5)?,
            })
        })
        .expect("OpenCode process_session rows should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("OpenCode process_session rows should decode")
}

pub fn wait_for_opencode_process_session(
    db_path: &Path,
    pid: u32,
    timeout: Duration,
) -> OpenCodeProcessSessionRow {
    let description = format!("process_session row for pid {}", pid);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let rows = read_opencode_process_sessions(db_path);
        if let Some(row) = rows.into_iter().find(|row| row.pid == pid) {
            WaitStatus::ready(row.clone(), format!("found row for pid {}", pid))
        } else {
            WaitStatus::pending(format!(
                "rows present: {:?}",
                read_opencode_process_sessions(db_path)
            ))
        }
    })
}

pub fn wait_for_opencode_process_session_state<F>(
    db_path: &Path,
    pid: u32,
    timeout: Duration,
    description_suffix: &str,
    predicate: F,
) -> OpenCodeProcessSessionRow
where
    F: Fn(&OpenCodeProcessSessionRow) -> bool,
{
    let description = format!("process_session row for pid {} {}", pid, description_suffix);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let rows = read_opencode_process_sessions(db_path);
        if let Some(row) = rows.into_iter().find(|row| row.pid == pid) {
            if predicate(&row) {
                WaitStatus::ready(row.clone(), format!("matched row for pid {}", pid))
            } else {
                WaitStatus::pending(format!("current row: {:?}", row))
            }
        } else {
            WaitStatus::pending(String::from("row missing"))
        }
    })
}

pub fn wait_for_opencode_process_session_absent(db_path: &Path, pid: u32, timeout: Duration) {
    let description = format!("process_session row for pid {} to disappear", pid);

    wait_until(&description, timeout, DEFAULT_POLL_INTERVAL, || {
        let rows = read_opencode_process_sessions(db_path);
        if rows.iter().any(|row| row.pid == pid) {
            WaitStatus::pending(format!("rows present: {:?}", rows))
        } else {
            WaitStatus::ready((), format!("pid {} absent", pid))
        }
    });
}

pub fn insert_opencode_session(
    db_path: &Path,
    id: &str,
    directory: &Path,
    parent_id: Option<&str>,
) {
    let parent = db_path
        .parent()
        .unwrap_or_else(|| panic!("OpenCode db path should have parent: {}", db_path.display()));
    fs::create_dir_all(parent)
        .unwrap_or_else(|error| panic!("Failed to create {}: {}", parent.display(), error));

    let connection = Connection::open(db_path)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS session (
                id TEXT PRIMARY KEY NOT NULL,
                directory TEXT NOT NULL,
                parent_id TEXT,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL
            );
            ",
        )
        .unwrap_or_else(|error| {
            panic!(
                "Failed to initialize fake OpenCode schema in {}: {}",
                db_path.display(),
                error
            )
        });

    connection
        .execute(
            "
            INSERT INTO session (id, directory, parent_id, time_created, time_updated)
            VALUES (?1, ?2, ?3, 1, 1)
            ",
            params![id, directory.display().to_string(), parent_id],
        )
        .unwrap_or_else(|error| {
            panic!(
                "Failed to insert fake OpenCode session {} into {}: {}",
                id,
                db_path.display(),
                error
            )
        });
}

pub fn ensure_opencode_process_session_table(db_path: &Path) {
    let parent = db_path
        .parent()
        .unwrap_or_else(|| panic!("OpenCode db path should have parent: {}", db_path.display()));
    fs::create_dir_all(parent)
        .unwrap_or_else(|error| panic!("Failed to create {}: {}", parent.display(), error));

    let connection = Connection::open(db_path)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS process_session (
                pid INTEGER PRIMARY KEY,
                proc_start_ticks INTEGER NOT NULL,
                session_id TEXT,
                directory TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                reason TEXT
            );
            ",
        )
        .unwrap_or_else(|error| {
            panic!(
                "Failed to initialize fake OpenCode process_session schema in {}: {}",
                db_path.display(),
                error
            )
        });
}

pub fn update_saved_session_opencode_session_id(db_path: &Path, name: &str, session_id: &str) {
    let connection = Connection::open(db_path)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let updated = connection
        .execute(
            "UPDATE sessions SET opencode_session_id = ?1 WHERE name = ?2",
            params![session_id, name],
        )
        .unwrap_or_else(|error| {
            panic!(
                "Failed to update OpenCode session ID for {}: {}",
                name, error
            )
        });

    assert_eq!(
        updated, 1,
        "Expected exactly one saved session named {}",
        name
    );
}

pub fn update_opencode_process_session_start_ticks(
    db_path: &Path,
    pid: u32,
    proc_start_ticks: u64,
) {
    let connection = Connection::open(db_path)
        .unwrap_or_else(|error| panic!("Failed to open {}: {}", db_path.display(), error));

    let updated = connection
        .execute(
            "UPDATE process_session SET proc_start_ticks = ?1 WHERE pid = ?2",
            params![proc_start_ticks, pid],
        )
        .unwrap_or_else(|error| {
            panic!(
                "Failed to update process_session start ticks for pid {} in {}: {}",
                pid,
                db_path.display(),
                error
            )
        });

    assert_eq!(
        updated, 1,
        "Expected exactly one process_session row for pid {}",
        pid
    );
}

pub fn saved_session_row(
    id: i64,
    name: &str,
    directory: &Path,
    opencode_args: &str,
) -> SavedSessionRow {
    SavedSessionRow {
        id,
        name: String::from(name),
        directory: directory.to_path_buf(),
        opencode_session_id: None,
        opencode_args: String::from(opencode_args),
    }
}

pub fn oc_cmd() -> Command {
    Command::cargo_bin("oc").expect("oc binary should build for tests")
}

fn prepend_path(prefix_dir: &Path) -> OsString {
    let mut path_entries = Vec::new();
    path_entries.push(prefix_dir.to_path_buf());

    if let Some(existing_path) = env::var_os("PATH") {
        path_entries.extend(env::split_paths(&existing_path));
    }

    env::join_paths(path_entries).expect("PATH entries should be joinable")
}

fn tmux_scope_prefix(scope_name: &str) -> String {
    format!(
        "{}{}-",
        TEST_TMUX_FAMILY_PREFIX,
        sanitize_scope_name(scope_name)
    )
}
