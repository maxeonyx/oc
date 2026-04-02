# oc — Agent Guide

Interactive TUI session manager for OpenCode. Manages tmux sessions named `oc-<name>`.

Part of the [maxeonyx agent-tools](https://tools.maxeonyx.com) family (alongside trunc, tmux-bridge, dotsync, tdd-ratchet). Follow the `agent-tools` skill for repo conventions.

## Requirements

See [VISION.md](VISION.md) for the full product vision and stakeholder stories.

## Tech stack

- Rust (edition 2024), binary name `oc`
- `ratatui` + `crossterm` for the TUI
- `clap` (derive) for CLI parsing
- Black-box E2E tests with `assert_cmd` + `predicates`, using real tmux sessions (see tmux-bridge's test architecture for the pattern)

## Testing

Tests should stay black-box E2E — spawn the `oc` binary, interact through its public CLI surface, and assert on outcomes. Never import application internals in tests.

Shared test helpers live in `tests/common/mod.rs`. Use the `TestEnv` harness there so each test gets isolated storage paths, a unique tmux prefix, pre/post tmux cleanup, and polling-based assertions instead of fixed sleeps.

The binary supports test-only environment overrides for isolation:

- `OC_ALIASES_FILE`
- `OC_TMUX_PREFIX`
- `OC_OPENCODE_DB`

Feature tests should set these through the shared harness rather than ad hoc environment setup.

TDD is enforced via `cargo ratchet` (the `tdd-ratchet` crate). CI runs `cargo ratchet` instead of `cargo test` directly.

## CI & release

Single `.github/workflows/ci.yml` with 4 chained jobs: Check → Build → Release + Pages. Auto-releases on every push to main (no manual tagging). Version bumps are only required for artifact-affecting changes; run `scripts/check-version-bump.py` when changing the release policy.

The repo pins Rust via `rust-toolchain.toml`; CI should use that same toolchain. The check job installs `tmux`, pins `cargo-nextest` to a Rust-1.88-compatible version, pins `tdd-ratchet` to a specific git revision for reproducibility, and runs `cargo ratchet` (never `cargo test` directly).

## Git identity

This is a personal repo (`maxeonyx/oc`). Repo-level git config must use the personal identity:

```
user.name = Maxwell Clarke
user.email = maxeonyx@gmail.com
```

## Pushing

Pushing to main is safe — it's just remote preservation. Commit and push frequently.
