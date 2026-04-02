//! Shared test utilities for black-box E2E tests.
//!
//! The first smoke test only needs the compiled binary, but keep the shared
//! module in place so future session-management tests have a natural home.

use assert_cmd::Command;

pub fn oc_cmd() -> Command {
    Command::cargo_bin("oc").expect("oc binary should build for tests")
}
