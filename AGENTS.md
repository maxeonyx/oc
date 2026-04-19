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

- `OC_ALIASES_FILE` — path to the SQLite database (legacy name from the old flat-file era; now points to the `.db` file, default `~/.config/oc/oc.db`)
- `OC_TMUX_PREFIX`
- `OC_OPENCODE_DB`

Feature tests should set these through the shared harness rather than ad hoc environment setup.

For manual visual inspection, set `OC_THEME=light` or `OC_THEME=dark` when terminal background detection is unreliable through tmux/SSH.

TDD is enforced via `cargo ratchet` (the `tdd-ratchet` crate). CI runs `cargo ratchet` instead of `cargo test` directly.

### Tmux session cleanup

Tests and fixture scripts create real tmux sessions. **Agents must verify no leaked tmux sessions remain after test runs or fixture use.** After running `cargo ratchet` or the test fixture script, check for leftover sessions with `tmux ls` and kill any that match the test/fixture prefix. The `TestEnv` harness handles cleanup for tests, but if a test crashes or an agent interrupts a run, sessions can leak.

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

## TUI rendering principles

These principles govern the rendering architecture. Code changes to `src/tui/render/` must satisfy all of them. They exist to make entire classes of visual bugs structurally impossible.

**Surface model:**
1. Every screen cell has exactly one owning surface (terminal bg, outer container, or descendant panel). No unpainted gaps.
2. A panel is a rectangular surface — it paints its ENTIRE rect with its background before rendering content.
3. Panels nest through content rects — children never paint outside parent's content rect.
4. Totals, headers, separators belong to their owning panel, not beside it.

**Width consistency:**
5. Every row inside a panel renders to EXACTLY the panel content width — padded with panel bg if shorter, clipped if longer.
6. This applies to ALL row types: data rows, group headers, separator lines, highlights, totals, empty states, selected rows.

**Interface stability:**
7. Dashboard dimensions (width AND height) are computed from unfiltered data with worst-case group header reservation (4 rows). They change only on terminal resize or data structure changes.
8. Filtering changes content within a fixed frame — never changes dashboard dimensions.
8a. Footer rows (totals, status) are pinned to the bottom of their panel. When filtering reduces content, slack space appears above the footer — never below it.

**Reactivity:**
9. Render is pure — render ticks never mutate selection, filter results, or layout caches.
10. Filtered results recompute only when filter text or underlying data changes.
11. Selection snaps to top result only when filter text changes AND the current selection disappears from results.

**Colors:**
12. Every color resolves to a named theme role → terminal background or ANSI palette. No ad hoc RGB in widget code.
13. Half-block transitions represent two known adjacent surfaces — fg and bg trace to the two surfaces being separated.

**Padding and containers:**
14. Padding is painted area — padding cells are filled with the panel's background.
15. The outer container is a real root panel with its own background, padding, content rect, and edges.

**Buttons:**
16. Action buttons share the panel width equally. Width = (available width - gutters) / button count. All buttons same rendered width.
17. Button state (enabled/disabled/selected) changes style only, never geometry.

**Emphasis:**
18. Structural separators (group headers, separator lines) are surface features. Their color derives from the panel background shifted slightly toward text — much closer to background than to content. Not derived from text or muted-text tokens. Contrast ratio against panel background must not exceed ~1.15; they should be near-invisible background features, not readable labels.

**Aspect ratio:**
19. Terminal cells are ~1:2 (1 wide, 2 tall). Horizontal padding should be 2 columns where vertical padding is 1 row (half-block), to maintain visual consistency.

**Framework usage:**
20. Work WITH ratatui, not around it. Use `frame.set_cursor_position()` for cursor management (ratatui handles show/hide). Don't call raw crossterm cursor commands.
