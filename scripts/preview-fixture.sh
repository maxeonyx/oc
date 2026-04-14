#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
TMP_ROOT=${TMPDIR:-/tmp}
FIXTURE_PATTERN="$TMP_ROOT/oc-test-fixture.*"

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

  if [[ -f "$fixture_dir/fixture.env" ]]; then
    "$SCRIPT_DIR/test-fixture.sh" cleanup "$fixture_dir" >/dev/null 2>&1 || true
  fi

  rm -rf "$fixture_dir"
}

cleanup_existing_fixtures() {
  shopt -s nullglob
  local fixture_dirs=( $FIXTURE_PATTERN )
  shopt -u nullglob

  local fixture_dir
  for fixture_dir in "${fixture_dirs[@]}"; do
    [[ -d "$fixture_dir" ]] || continue
    cleanup_one_fixture "$fixture_dir"
  done
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
cleanup_current_fixture() {
  local exit_code=$?

  if [[ -n "$fixture_dir" ]] && [[ -d "$fixture_dir" ]]; then
    cleanup_one_fixture "$fixture_dir"
  fi

  exit "$exit_code"
}

trap cleanup_current_fixture EXIT INT TERM

cargo build --manifest-path "$REPO_ROOT/Cargo.toml"
cleanup_existing_fixtures

fixture_dir=$(mktemp -d "$TMP_ROOT/oc-test-fixture.XXXXXX")
OC_TEST_FIXTURE_DIR="$fixture_dir" "$SCRIPT_DIR/test-fixture.sh" setup >/dev/null

# shellcheck disable=SC1090
source "$fixture_dir/fixture.env"

if [[ -n "$theme" ]]; then
  export OC_THEME="$theme"
fi

(
  cd "$fixture_dir"
  "$REPO_ROOT/target/debug/oc" "${oc_args[@]}"
)
