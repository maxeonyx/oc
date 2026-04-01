# oc — Vision

## The problem

I run 4-8 concurrent OpenCode sessions across many project repos, in a VirtualBox VM accessed via SSH from Windows Terminal. These sessions are long-running and valuable — hours of conversation context, in-progress work, agent state.

But they're fragile. If I close a terminal tab, my SSH drops, my laptop sleeps, or Windows Terminal crashes — the session dies. I lose the running OpenCode process and have to reconstruct what I was doing.

I also discovered that Windows Terminal's GPU-accelerated text rendering consumes enormous shared GPU memory (9.8GB!) for scrollback-heavy TUI apps like OpenCode. Running sessions inside tmux solves this — tmux owns the scrollback, and the terminal only renders the visible viewport.

## What I need

A small, polished tool that makes OpenCode sessions survive terminal disconnects and makes managing many concurrent sessions effortless.

### See everything at a glance

When I type `oc` with no arguments, I want to see all my sessions — what's running, what's detached (backgrounded), what's saved but not currently running. I should be able to tell immediately which sessions need my attention.

### Jump to any session instantly

The most common thing I do is switch to an existing session. This must be the fastest, most prominent path. Arrow to the session I want, press Enter, I'm there. Or type `oc dc` and I'm attached to my DataCentral session without seeing a dashboard at all.

### Never lose a session

Sessions live in tmux. If my SSH drops, the session keeps running. When I reconnect, I pick up exactly where I left off. The tmux session's lifecycle matches the OpenCode process — when OpenCode exits, the tmux session cleans up.

### Create new sessions with zero friction

When I start working on something new, I want to create a named session pointing at a directory. The name and directory should be saved automatically so next time I can jump straight back. I should be able to choose between saving it (for ongoing work) or keeping it anonymous (for quick one-offs that disappear when I'm done).

### Adopt existing OpenCode sessions

I have orphaned OpenCode sessions from before I started using this tool. I want to browse them, pick one, give it a name, and bring it under management. Like `opencode session list` but interactive — arrow through the list, press Enter, type a name.

### Smart auto-attach from current directory

When I type `oc` with no arguments and my current directory matches exactly one session or alias, skip the dashboard and attach (or launch) it directly. This is the common case — I'm in `~/dc-main`, I type `oc`, I'm in my DC session. No arrow keys needed. If there are zero matches or multiple matches, show the dashboard as normal.

### Remember session IDs

OpenCode sessions have IDs (like `ses_44b3d03b4ffeKG3VvdRgluoB2W`). When I create or adopt a session, the tool should remember which OpenCode session ID belongs to which named session, so it can pass `--session <id>` when re-launching. I shouldn't have to manage session IDs manually.

### Intuitive keyboard interaction

My fingers are on the arrow keys. The primary interaction model is:

- **Up/down** to move between sessions
- **Enter** to execute the current action for the highlighted row
- **Left/right** to cycle through available actions for the highlighted row (attach, kill, remove alias, etc.). Left/right should wrap.
- **Type to filter** narrows the session list by name
- **Esc** clears the filter (or quits if no filter)

No single-letter hotkeys (n, q, etc.). They conflict with type-to-filter, and the visual dashboard makes them unnecessary. All actions are reachable through row selection + left/right.

The **selected action** should be shown prominently at the bottom of the screen, highlighted. It resets to the default when you move to a different row.

### "New session" is a row, not a hotkey

The session list includes a special "New session" row. Selecting it and pressing Enter starts the new-session flow (prompting for name, etc.) **inside the TUI**, not by closing the TUI and dropping to a shell prompt.

Default selection when the dashboard opens depends on context:
- **In `$HOME` or a directory with no matching sessions**: "New session" row is selected by default
- **In a directory with multiple matching sessions**: the first matching session is selected. "New session" appears after the matches, before the remaining sessions.

### Visual design

- Must work in both light and dark terminal themes. Don't assume dark mode.
- Clean layout with borders and summary header (counts of attached/detached/saved).
- Dynamic column widths based on content.
- Directory abbreviation: when the dir basename matches the session name, abbreviate to highlight only what's different.
- Highlighted row should stand out clearly but not be garish.

### Direct CLI for scripts and muscle memory

Beyond the interactive dashboard, I need non-interactive commands that work in scripts and when I already know what I want:

- `oc <name>` — attach or launch by name, no dashboard
- `oc rm <name>` — remove the session (kill if running) and its alias
- `oc alias <name> [dir] [-- opencode-args...]` — save a named session
- `oc unalias <name>` — remove one
- `oc -h` — help

## Technical context

- The tool manages tmux sessions named `oc-<name>`. It is not a general tmux manager.
- Session configuration (name → directory, opencode session ID, opencode args) is stored in a human-editable aliases file at `~/.config/oc/aliases`.
- When inside tmux already, switching sessions should use `tmux switch-client` (not nest tmux).
- Creating a session runs: `tmux new-session -s oc-<name> -c <dir> opencode <args>`
- This is a Rust CLI tool in the same family as `trunc` and `tmux-bridge` (see those repos for CI, release, testing, and landing page patterns). Follow the `agent-tools` skill.
- Tests should be black-box E2E tests using real tmux sessions and PTY interaction (like tmux-bridge's test architecture).
- The project should use `tdd-ratchet` (the Rust `cargo install tdd-ratchet` tool, NOT a custom ratchet.py script).

## Working prototype

A fish function prototype exists at `~/.config/fish/functions/oc.fish` (plus helpers `_oc_attach.fish`, `_oc_dashboard.fish`, `_oc_new_session.fish`, `_oc_reltime.fish`). This covers the basic flow but is a static print-and-prompt, not a real interactive TUI. Study it for behavioral reference.

## Learnings from prototyping

Two throwaway Rust prototypes were built (one by GPT-5.4, one by Claude Opus) to test architectural choices. Key findings:

- **The TUI is the point.** A print-and-prompt Rust rewrite adds nothing over the fish version. The interactive dashboard with arrow navigation is the core value of the rewrite.
- **Borders and summary header matter.** The version with a bordered layout and session-count summary (attached/detached/saved) felt significantly better than the plain one.
- **Light mode must work.** One prototype assumed dark terminal colors and was unreadable in light mode. Use colors that work in both.
- **Left/right action cycling with wrapping feels right.** But the current action must be shown prominently at the bottom, not inline on the row (too subtle).
- **Type-to-filter is great but kills hotkeys.** Can't have both 'n' for new and typing 'n' to filter. Solution: drop hotkeys entirely, make "New session" a selectable row.
- **Don't close the TUI for new-session creation.** Prompt for name/args inside the TUI. Closing and reopening is jarring.
- **`ratatui` + `crossterm` works well.** Both prototypes used this stack. `Paragraph` with `Line` spans gives better per-row control than ratatui's `Table` widget.
- **All three session states must be visible.** Both prototypes initially missed showing saved-but-not-running aliases. The merged list (tmux sessions + saved aliases) is essential.
