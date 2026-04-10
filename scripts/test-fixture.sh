#!/usr/bin/env bash

set -euo pipefail

# Populate an isolated oc fixture so the TUI can be exercised without touching
# Max's real config or real oc-* tmux sessions.

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
LATEST_POINTER="${TMPDIR:-/tmp}/oc-test-fixture-latest"

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

  cat >"$fixture_dir/fixture.env" <<EOF
export OC_ALIASES_FILE='$db_path'
export OC_TMUX_PREFIX='$tmux_prefix'
export OC_OPENCODE_DB='$fixture_dir/opencode.sqlite'
export OC_TEST_FIXTURE_DIR='$fixture_dir'
EOF
}

register_aliases() {
  local oc_bin=$1
  local db_path=$2
  local tmux_prefix=$3
  local fixture_root=$4

  local -n registered_names_ref=$5
  local -n registered_dirs_ref=$6

  local -a candidates=(
    "oc|$REPO_ROOT"
    "tmp|/tmp"
    "cfg|$HOME/.config"
    "scripts-playground|$REPO_ROOT/scripts"
    "home-lab|$HOME"
    "dc-main|$HOME/dc-main"
    "tmp-long-project|/var/tmp"
    "alpha-01|$fixture_root/projects/alpha-01"
    "alpha-ops|$fixture_root/projects/alpha-ops"
    "job-117|$fixture_root/projects/job-117"
    "ses-demo|$fixture_root/projects/ses-demo"
  )

  local entry name dir
  for entry in "${candidates[@]}"; do
    IFS='|' read -r name dir <<<"$entry"
    if [[ ! -d "$dir" ]]; then
      continue
    fi

    OC_ALIASES_FILE="$db_path" OC_TMUX_PREFIX="$tmux_prefix" "$oc_bin" alias "$name" "$dir"
    registered_names_ref+=("$name")
    registered_dirs_ref+=("$dir")
  done

  if (( ${#registered_names_ref[@]} < 4 )); then
    printf 'Expected at least 4 usable fixture directories, found %s\n' "${#registered_names_ref[@]}" >&2
    exit 1
  fi
}

set_saved_session_id() {
  local db_path=$1
  local name=$2
  local session_id=$3

  python3 - "$db_path" "$name" "$session_id" <<'PY'
import sqlite3
import sys

db_path, name, session_id = sys.argv[1:4]
connection = sqlite3.connect(db_path)
updated = connection.execute(
    "UPDATE sessions SET opencode_session_id = ? WHERE name = ?",
    (session_id, name),
).rowcount
connection.commit()
connection.close()
if updated != 1:
    raise SystemExit(f"expected exactly one row updated for {name}, got {updated}")
PY
}

start_detached_session() {
  local session_name=$1
  local directory=$2

  tmux new-session -d -s "$session_name" -c "$directory" "sleep 36000"
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
  mkdir -p "$fixture_root/projects/alpha-01" "$fixture_root/projects/alpha-ops" "$fixture_root/projects/job-117" "$fixture_root/projects/ses-demo"
  local tmux_token
  tmux_token=$(sanitize_tmux_token "$(basename "$fixture_dir")")
  local tmux_prefix="oc-fixture-$tmux_token-"
  local pid_file="$fixture_dir/attached-client-pids"
  local dump_file="$fixture_dir/session-list.txt"
  local attached_log="$fixture_dir/attached-client.log"
  : >"$pid_file"

  write_env_file "$fixture_dir" "$db_path" "$tmux_prefix"
  printf '%s\n' "$fixture_dir" >"$LATEST_POINTER"

  local -a alias_names=()
  local -a alias_dirs=()
  register_aliases "$oc_bin" "$db_path" "$tmux_prefix" "$fixture_root" alias_names alias_dirs

  local attached_name=${alias_names[0]}
  local attached_dir=${alias_dirs[0]}
  local detached_one_name=${alias_names[1]}
  local detached_one_dir=${alias_dirs[1]}
  local detached_two_name=${alias_names[2]}
  local detached_two_dir=${alias_dirs[2]}

  set_saved_session_id "$db_path" "ses-demo" "ses_fixture_demo_123"
  set_saved_session_id "$db_path" "$attached_name" "ses_fixture_running_123"

  start_detached_session "$tmux_prefix$attached_name" "$attached_dir"
  spawn_attached_client "$tmux_prefix$attached_name" "$pid_file" "$attached_log"
  wait_for_attached_client "$tmux_prefix$attached_name"

  start_detached_session "$tmux_prefix$detached_one_name" "$detached_one_dir"
  start_detached_session "$tmux_prefix$detached_two_name" "$detached_two_dir"

  OC_ALIASES_FILE="$db_path" OC_TMUX_PREFIX="$tmux_prefix" "$oc_bin" __dump-session-list | tee "$dump_file"

  cat <<EOF
Created isolated oc fixture.

Fixture directory:
  $fixture_dir

The fixture uses:
  OC_ALIASES_FILE=$db_path
  OC_TMUX_PREFIX=$tmux_prefix

To inspect the populated dashboard with the local build:
  source "$fixture_dir/fixture.env"
  "$oc_bin"

If you want to use whatever oc is on your PATH instead:
  source "$fixture_dir/fixture.env"
  oc

Current dashboard rows:
$(sed 's/^/  /' "$dump_file")

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
