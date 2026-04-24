#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
TMP_ROOT=${TMPDIR:-/tmp}
PREVIEW_TMUX_PREFIX="oc2-preview-"
LATEST_ENV_FILE="$TMP_ROOT/oc-fixture-env.sh"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/preview-fixture.sh
  ./scripts/preview-fixture.sh --theme light
  ./scripts/preview-fixture.sh --theme dark -- --help

Options:
  --theme light|dark  Override TUI theme. Default: auto-detect.
  -h, --help          Show this help.

Any remaining arguments after -- are passed to the oc binary.
Additional environment variables from your shell are inherited automatically.
EOF
}

cleanup_one_fixture() {
  local fixture_dir=$1
  local fixture_tmux_prefix=

  if [[ -f "$fixture_dir/fixture.env" ]]; then
    # shellcheck disable=SC1090
    source "$fixture_dir/fixture.env"
    fixture_tmux_prefix=${OC_TMUX_PREFIX:-}
  fi

  if [[ -f "$fixture_dir/fixture.env" ]]; then
    "$SCRIPT_DIR/test-fixture.sh" cleanup "$fixture_dir" >/dev/null 2>&1 || true
  fi

  if [[ -n "$fixture_tmux_prefix" ]]; then
    while IFS= read -r session_name; do
      [[ "$session_name" == "$fixture_tmux_prefix"* ]] || continue
      tmux detach-client -s "$session_name" >/dev/null 2>&1 || true
      tmux kill-session -t "$session_name" >/dev/null 2>&1 || true
    done < <(tmux list-sessions -F '#{session_name}' 2>/dev/null || true)
  fi

  rm -rf "$fixture_dir"
}

clear_latest_env_file() {
  if [[ -f "$LATEST_ENV_FILE" ]]; then
    rm -f "$LATEST_ENV_FILE"
  fi
}

latest_fixture_dir() {
  if [[ ! -f "$LATEST_ENV_FILE" ]]; then
    return 1
  fi

  # shellcheck disable=SC1090
  source "$LATEST_ENV_FILE"

  if [[ -z "${OC_TEST_FIXTURE_DIR:-}" ]]; then
    return 1
  fi

  printf '%s\n' "$OC_TEST_FIXTURE_DIR"
}

fixture_is_live() {
  local candidate_dir=$1

  [[ -d "$candidate_dir" ]] || return 1
  [[ -f "$candidate_dir/fixture.env" ]] || return 1

  local fixture_tmux_prefix=

  # shellcheck disable=SC1090
  source "$candidate_dir/fixture.env"
  fixture_tmux_prefix=${OC_TMUX_PREFIX:-}

  [[ -n "$fixture_tmux_prefix" ]] || return 1

  while IFS= read -r session_name; do
    [[ "$session_name" == "$fixture_tmux_prefix"* ]] || continue
    return 0
  done < <(tmux list-sessions -F '#{session_name}' 2>/dev/null || true)

  return 1
}

theme=
declare -a oc_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --theme)
      if [[ $# -lt 2 ]]; then
        printf '%s\n' 'Missing value for --theme' >&2
        exit 1
      fi
      case "$2" in
        light|dark)
          theme=$2
          ;;
        *)
          printf 'Invalid theme: %s\n' "$2" >&2
          exit 1
          ;;
      esac
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      oc_args=("$@")
      break
      ;;
    *)
      oc_args+=("$1")
      shift
      ;;
  esac
done

fixture_dir=

cargo build --manifest-path "$REPO_ROOT/Cargo.toml"
if fixture_dir=$(latest_fixture_dir) && fixture_is_live "$fixture_dir"; then
  :
else
  if [[ -n "$fixture_dir" ]] && [[ -d "$fixture_dir" ]]; then
    cleanup_one_fixture "$fixture_dir"
  else
    clear_latest_env_file
  fi

  fixture_dir=$(mktemp -d "$TMP_ROOT/oc-test-fixture.XXXXXX")
  OC_TEST_FIXTURE_DIR="$fixture_dir" OC_TMUX_PREFIX="$PREVIEW_TMUX_PREFIX" "$SCRIPT_DIR/test-fixture.sh" setup >/dev/null
fi

# shellcheck disable=SC1090
source "$fixture_dir/fixture.env"

if [[ -n "$theme" ]]; then
  export OC_THEME="$theme"
fi

(
  cd "$fixture_dir"
  "$REPO_ROOT/target/debug/oc" "${oc_args[@]}"
)
