#!/usr/bin/env bash

set -euo pipefail

# Populate an isolated oc fixture so the TUI can be exercised without touching
# Max's real config or real oc-* tmux sessions.

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
LATEST_POINTER="${TMPDIR:-/tmp}/oc-test-fixture-latest"
LATEST_ENV_FILE="${TMPDIR:-/tmp}/oc-fixture-env.sh"

usage() {
  cat <<EOF
Usage:
  $(basename "$0")               # create a new isolated fixture
  $(basename "$0") cleanup DIR   # tear down a previously created fixture

Environment overrides:
  OC_BIN=/path/to/oc              Use a specific oc binary instead of target/debug/oc
  OC_TEST_FIXTURE_DIR=/tmp/path   Reuse a specific fixture directory instead of mktemp
EOF
}

ensure_command() {
  local command_name=$1
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
}

ensure_oc_bin() {
  if [[ -n "${OC_BIN:-}" ]]; then
    if [[ ! -x "$OC_BIN" ]]; then
      printf 'OC_BIN is not executable: %s\n' "$OC_BIN" >&2
      exit 1
    fi
    printf '%s\n' "$OC_BIN"
    return
  fi

  local debug_bin="$REPO_ROOT/target/debug/oc"
  if [[ ! -x "$debug_bin" ]]; then
    cargo build --quiet --manifest-path "$REPO_ROOT/Cargo.toml"
  fi

  printf '%s\n' "$debug_bin"
}

sanitize_tmux_token() {
  printf '%s' "$1" | tr -c '[:alnum:]-_' '-'
}

write_env_file() {
  local fixture_dir=$1
  local db_path=$2
  local tmux_prefix=$3
  local fake_bin_dir=$4
  local env_file="$fixture_dir/fixture.env"
  local shell_path=$PATH

  write_shell_exports "$env_file" \
    OC_ALIASES_FILE "$db_path" \
    OC_TMUX_PREFIX "$tmux_prefix" \
    OC_OPENCODE_DB "$fixture_dir/opencode.sqlite" \
    OC_TEST_FIXTURE_DIR "$fixture_dir" \
    PATH "$fake_bin_dir:$shell_path"

  cp "$env_file" "$LATEST_ENV_FILE"
}

write_shell_exports() {
  local output_path=$1
  shift

  : >"$output_path"
  while [[ $# -gt 0 ]]; do
    local key=$1
    local value=$2
    local escaped_value
    printf -v escaped_value '%q' "$value"
    printf 'export %s=%s\n' "$key" "$escaped_value" >>"$output_path"
    shift 2
  done
}

register_alias() {
  local oc_bin=$1
  local db_path=$2
  local tmux_prefix=$3
  local opencode_db=$4
  local tool_path=$5
  local name=$6
  local dir=$7

  OC_ALIASES_FILE="$db_path" \
    OC_TMUX_PREFIX="$tmux_prefix" \
    OC_OPENCODE_DB="$opencode_db" \
    PATH="$tool_path" \
    "$oc_bin" alias "$name" "$dir"
}

install_fake_opencode() {
  local fixture_dir=$1
  local bin_dir="$fixture_dir/bin"
  local script_path="$bin_dir/opencode"

  mkdir -p "$bin_dir"
  cp "$SCRIPT_DIR/fake-opencode" "$script_path"
  chmod 755 "$script_path"

  printf '%s\n' "$bin_dir"
}

seed_opencode_session() {
  local db_path=$1
  local directory=$2
  local session_id=$3

  python3 - "$db_path" "$directory" "$session_id" <<'PY'
import sqlite3
import sys
import time

db_path, directory, session_id = sys.argv[1:4]
connection = sqlite3.connect(db_path)
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
        parent_id = excluded.parent_id,
        time_updated = excluded.time_updated
    ''',
    (session_id, directory, now, now),
)
connection.commit()
connection.close()
PY
}

spawn_attached_client() {
  local session_name=$1
  local pid_file=$2
  local log_file=${3:-/dev/null}

  nohup env TERM="${TERM:-screen}" python3 -c $'import os, pty, sys\npid, _ = pty.fork()\nif pid == 0:\n    os.execvp("tmux", ["tmux", "attach-session", "-t", sys.argv[1]])\n_, status = os.waitpid(pid, 0)\nraise SystemExit(os.waitstatus_to_exitcode(status))' "$session_name" >"$log_file" 2>&1 < /dev/null &
  printf '%s\n' "$!" >>"$pid_file"
}

session_attached_count() {
  local session_name=$1
  local line name attached_count

  while IFS=$'\t' read -r name attached_count; do
    if [[ "$name" == "$session_name" ]]; then
      printf '%s\n' "$attached_count"
      return
    fi
  done < <(tmux list-sessions -F '#{session_name}	#{session_attached}' 2>/dev/null || true)

  printf '0\n'
}

wait_for_attached_client() {
  local session_name=$1
  local attempt

  for attempt in {1..50}; do
    if [[ $(session_attached_count "$session_name") -gt 0 ]]; then
      return
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for tmux session %s to become attached\n' "$session_name" >&2
  exit 1
}

wait_for_detached_session() {
  local session_name=$1
  local attempt

  for attempt in {1..50}; do
    if [[ $(session_attached_count "$session_name") -eq 0 ]]; then
      return
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for tmux session %s to become detached\n' "$session_name" >&2
  exit 1
}

wait_for_tmux_session() {
  local session_name=$1
  local attempt

  for attempt in {1..100}; do
    if tmux has-session -t "$session_name" >/dev/null 2>&1; then
      return
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for tmux session %s to exist\n' "$session_name" >&2
  exit 1
}

wait_for_oc_process_exit() {
  local pid=$1
  local attempt

  for attempt in {1..100}; do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      wait "$pid"
      return
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for oc process %s to exit\n' "$pid" >&2
  exit 1
}

wait_for_saved_session_id() {
  local db_path=$1
  local name=$2
  local attempt

  for attempt in {1..100}; do
    local session_id
    session_id=$(python3 - "$db_path" "$name" <<'PY'
import sqlite3
import sys

db_path, name = sys.argv[1:3]
connection = sqlite3.connect(db_path)
try:
    row = connection.execute(
        "SELECT opencode_session_id FROM sessions WHERE name = ?",
        (name,),
    ).fetchone()
finally:
    connection.close()

if row and row[0]:
    print(row[0])
PY
)
    if [[ -n "$session_id" ]]; then
      printf '%s\n' "$session_id"
      return
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for saved OpenCode session ID for %s\n' "$name" >&2
  exit 1
}

launch_via_oc_new() {
  local oc_bin=$1
  local fixture_dir=$2
  local db_path=$3
  local tmux_prefix=$4
  local opencode_db=$5
  local tool_path=$6
  local name=$7
  local directory=$8
  local pid_file=$9

  local log_file="$fixture_dir/${name}-oc-new.log"
  OC_ALIASES_FILE="$db_path" \
    OC_TMUX_PREFIX="$tmux_prefix" \
    OC_OPENCODE_DB="$opencode_db" \
    PATH="$tool_path" \
    "$oc_bin" new "$name" "$directory" >"$log_file" 2>&1 &
  local oc_pid=$!
  printf '%s\n' "$oc_pid" >>"$pid_file"

  local session_name="$tmux_prefix$name"
  wait_for_tmux_session "$session_name"
  wait_for_attached_client "$session_name"
  tmux detach-client -s "$session_name"
  wait_for_detached_session "$session_name"
  wait_for_oc_process_exit "$oc_pid"
  wait_for_saved_session_id "$db_path" "$name" >/dev/null
}

session_row_summary() {
  local db_path=$1
  local opencode_db=$2
  local tmux_prefix=$3

  python3 - "$db_path" "$opencode_db" "$tmux_prefix" <<'PY'
import sqlite3
import subprocess
import sys

db_path, opencode_db, tmux_prefix = sys.argv[1:4]

connection = sqlite3.connect(db_path)
rows = connection.execute(
    "SELECT name, directory, opencode_session_id FROM sessions ORDER BY id"
).fetchall()
connection.close()

attached_counts = {}
try:
    output = subprocess.check_output(
        ["tmux", "list-sessions", "-F", "#{session_name}\t#{session_attached}"],
        text=True,
        stderr=subprocess.DEVNULL,
    )
except subprocess.CalledProcessError:
    output = ""

for line in output.splitlines():
    session_name, attached = line.split("\t", 1)
    attached_counts[session_name] = int(attached)

opencode_connection = sqlite3.connect(opencode_db)
try:
    for name, directory, session_id in rows:
        matches = opencode_connection.execute(
            "SELECT COUNT(*) FROM session WHERE directory = ? AND parent_id IS NULL",
            (directory,),
        ).fetchone()[0]
        session_name = f"{tmux_prefix}{name}"
        if session_name in attached_counts:
            runtime = "attached" if attached_counts[session_name] > 0 else "detached"
        else:
            runtime = "saved"
        print(f"{name}\t{runtime}\t{session_id or '(null)'}\troot_rows={matches}\t{directory}")
finally:
    opencode_connection.close()
PY
}

cleanup_fixture() {
  local fixture_dir=$1
  local env_file="$fixture_dir/fixture.env"
  local pid_file="$fixture_dir/attached-client-pids"

  if [[ ! -f "$env_file" ]]; then
    printf 'Fixture env file not found: %s\n' "$env_file" >&2
    exit 1
  fi

  # shellcheck disable=SC1090
  source "$env_file"

  if [[ -f "$pid_file" ]]; then
    while IFS= read -r pid; do
      [[ -n "$pid" ]] || continue
      kill "$pid" >/dev/null 2>&1 || true
    done <"$pid_file"
  fi

  if [[ -n "${OC_TMUX_PREFIX:-}" ]]; then
    while IFS= read -r session_name; do
      [[ "$session_name" == "$OC_TMUX_PREFIX"* ]] || continue
      tmux detach-client -s "$session_name" >/dev/null 2>&1 || true
      tmux kill-session -t "$session_name" >/dev/null 2>&1 || true
    done < <(tmux list-sessions -F '#{session_name}' 2>/dev/null || true)
  fi

  rm -rf "$fixture_dir"
  if [[ -f "$LATEST_POINTER" ]] && [[ $(<"$LATEST_POINTER") == "$fixture_dir" ]]; then
    rm -f "$LATEST_POINTER"
  fi
  if [[ -f "$LATEST_ENV_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$LATEST_ENV_FILE" || true
    if [[ "${OC_TEST_FIXTURE_DIR:-}" == "$fixture_dir" ]]; then
      rm -f "$LATEST_ENV_FILE"
    fi
  fi

  printf 'Cleaned up fixture: %s\n' "$fixture_dir"
}

create_fixture() {
  ensure_command cargo
  ensure_command python3
  ensure_command tmux

  local oc_bin
  oc_bin=$(ensure_oc_bin)

  local fixture_dir=${OC_TEST_FIXTURE_DIR:-}
  if [[ -z "$fixture_dir" ]]; then
    fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/oc-test-fixture.XXXXXX")
  else
    mkdir -p "$fixture_dir"
    if [[ -e "$fixture_dir/oc.db" ]]; then
      printf 'Refusing to reuse existing fixture DB: %s\n' "$fixture_dir/oc.db" >&2
      printf 'Run cleanup first or choose a different OC_TEST_FIXTURE_DIR.\n' >&2
      exit 1
    fi
  fi

  local db_path="$fixture_dir/oc.db"
  local fixture_root="$fixture_dir"
  mkdir -p \
    "$fixture_root/projects/alpha-01" \
    "$fixture_root/projects/alpha-ops" \
    "$fixture_root/projects/job-117" \
    "$fixture_root/projects/ses-demo"
  local fake_bin_dir
  fake_bin_dir=$(install_fake_opencode "$fixture_dir")
  local tool_path="$fake_bin_dir:$PATH"
  local tmux_token
  tmux_token=$(sanitize_tmux_token "$(basename "$fixture_dir")")
  local tmux_prefix=${OC_TMUX_PREFIX:-"oc-fixture-$tmux_token-"}
  local pid_file="$fixture_dir/attached-client-pids"
  local dump_file="$fixture_dir/session-list.txt"
  local attached_log="$fixture_dir/attached-client.log"
  : >"$pid_file"

  write_env_file "$fixture_dir" "$db_path" "$tmux_prefix" "$fake_bin_dir"
  printf '%s\n' "$fixture_dir" >"$LATEST_POINTER"

  local attached_name=job-117
  local attached_dir="$fixture_root/projects/job-117"
  local detached_name=alpha-01
  local detached_dir="$fixture_root/projects/alpha-01"
  local catchup_name=alpha-ops
  local catchup_dir="$fixture_root/projects/alpha-ops"
  local ambiguous_name=ses-demo
  local ambiguous_dir="$fixture_root/projects/ses-demo"

  register_alias "$oc_bin" "$db_path" "$tmux_prefix" "$fixture_dir/opencode.sqlite" "$tool_path" "$catchup_name" "$catchup_dir"
  register_alias "$oc_bin" "$db_path" "$tmux_prefix" "$fixture_dir/opencode.sqlite" "$tool_path" "$ambiguous_name" "$ambiguous_dir"

  launch_via_oc_new "$oc_bin" "$fixture_dir" "$db_path" "$tmux_prefix" "$fixture_dir/opencode.sqlite" "$tool_path" "$attached_name" "$attached_dir" "$pid_file"
  spawn_attached_client "$tmux_prefix$attached_name" "$pid_file" "$attached_log"
  wait_for_attached_client "$tmux_prefix$attached_name"

  launch_via_oc_new "$oc_bin" "$fixture_dir" "$db_path" "$tmux_prefix" "$fixture_dir/opencode.sqlite" "$tool_path" "$detached_name" "$detached_dir" "$pid_file"

  seed_opencode_session "$fixture_dir/opencode.sqlite" "$catchup_dir" 'ses_fixture_catchup_001'
  seed_opencode_session "$fixture_dir/opencode.sqlite" "$ambiguous_dir" 'ses_fixture_ambiguous_001'
  seed_opencode_session "$fixture_dir/opencode.sqlite" "$ambiguous_dir" 'ses_fixture_ambiguous_002'

  session_row_summary "$db_path" "$fixture_dir/opencode.sqlite" "$tmux_prefix" | tee "$dump_file"

  cat <<EOF
Created isolated oc fixture.

Fixture directory:
  $fixture_dir

The fixture uses:
  OC_ALIASES_FILE=$db_path
  OC_TMUX_PREFIX=$tmux_prefix
  OC_OPENCODE_DB=$fixture_dir/opencode.sqlite
  PATH=$fake_bin_dir:\$PATH

Sourceable env file:
  $LATEST_ENV_FILE

To inspect the populated dashboard with the local build:
  source "$LATEST_ENV_FILE"
  "$oc_bin"

If you want to use whatever oc is on your PATH instead:
  source "$LATEST_ENV_FILE"
  oc

Current dashboard rows:
$(python3 - "$dump_file" <<'PY'
from pathlib import Path
import sys

for line in Path(sys.argv[1]).read_text(encoding='utf-8').splitlines():
    print(f"  {line}")
PY
)

Cleanup when you're done:
  "$SCRIPT_DIR/test-fixture.sh" cleanup "$fixture_dir"

The most recent fixture dir is also recorded in:
  $LATEST_POINTER
EOF
}

main() {
  case "${1:-setup}" in
    setup)
      create_fixture
      ;;
    cleanup)
      if [[ $# -ne 2 ]]; then
        usage >&2
        exit 1
      fi
      cleanup_fixture "$2"
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      usage >&2
      exit 1
      ;;
  esac
}

main "$@"
