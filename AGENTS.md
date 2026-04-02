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

Shared test helpers live in `tests/common/mod.rs`. Expand that module toward polling-based real-tmux helpers as session-management behavior is implemented; avoid fixed sleeps.

TDD is enforced via `cargo ratchet` (the `tdd-ratchet` crate). CI runs `cargo ratchet` instead of `cargo test` directly.

## CI & release

Single `.github/workflows/ci.yml` with 4 chained jobs: Check → Build → Release + Pages. Auto-releases on every push to main (no manual tagging). Version-bump enforcement — every push must bump the version in Cargo.toml.

The check job installs `tmux`, `cargo-nextest`, and runs `cargo ratchet` (never `cargo test` directly).

## Git identity

This is a personal repo (`maxeonyx/oc`). Repo-level git config must use the personal identity:

```
user.name = Maxwell Clarke
user.email = maxeonyx@gmail.com
```

## Pushing

Pushing to main is safe — it's just remote preservation. Commit and push frequently.
