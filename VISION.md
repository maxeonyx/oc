# oc — Vision

## The problem

I run 4-8 concurrent OpenCode sessions across many project repos, in a VirtualBox VM accessed via SSH from Windows Terminal. These sessions are long-running and valuable — hours of conversation context, in-progress work, agent state.

But they're fragile. If I close a terminal tab, my SSH drops, my laptop sleeps, or Windows Terminal crashes — the session dies. I lose the running OpenCode process and have to reconstruct what I was doing.

I also discovered that Windows Terminal's GPU-accelerated text rendering consumes enormous shared GPU memory (9.8GB!) for scrollback-heavy TUI apps like OpenCode. Running sessions inside tmux solves this — tmux owns the scrollback, and the terminal only renders the visible viewport.

## What I need

A small, polished tool that makes OpenCode sessions survive terminal disconnects and makes managing many concurrent sessions effortless.

### See everything at a glance

When I type `oc` with no arguments, I want to see all my sessions — what's running, what's detached (backgrounded), what's saved but not currently running. I should be able to tell immediately which sessions need my attention.

The TUI updates live as sessions are created or destroyed in the background — not just a snapshot on startup. All live state (session list, memory usage, status) should refresh as fast as possible — ideally 10Hz if performant.

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

### Session status

Show whether OpenCode is actively working vs idle in each session, not just whether the tmux session is attached or detached. This might need an OpenCode plugin hook — deferred until we have a reliable mechanism. For now, use PID to determine running/not-running.

### Memory usage

Each session row shows the memory usage of the running OpenCode process (e.g. `523 MiB`). The bottom of the TUI shows column totals that react to the current filter:

- **ID column total**: count of aliases matching the filter
- **Status column total**: count of running OpenCode + tmux processes matching the filter
- **Memory column total**: sum of memory usage for filtered sessions

This requires tracking OpenCode PIDs (by inspecting the tmux pane's child process), not just tmux session names.

### Return to TUI on detach

`oc` is always run from outside tmux — it is the outer process. When you pick a session in the TUI and attach, detaching from tmux (ctrl-b d) returns control to the parent `oc` process and the TUI redraws. The TUI is the hub.

If a session was launched directly from the CLI (`oc dc`), detaching returns to the shell as normal — no TUI to return to.

### Restart sessions

Graceful restart: send ctrl-c then ctrl-d to the running OpenCode process, re-launch the same OpenCode session ID, wait ~10 seconds, then send `continue` + enter. This lets you restart a misbehaving session without losing the OpenCode conversation history.

If the graceful stop times out, restart fails — it does not force-kill. Restart is unavailable for sessions without a saved OpenCode session ID.

### Move sessions

`oc mv <name> <new-dir>` changes a session's directory and then attaches. The session must be stopped first — error if it's running. This only updates oc's database record; it doesn't move files on disk. You move the files yourself, then tell oc about it. On next launch, oc starts OpenCode with the same session ID from the new directory, and OpenCode picks up the change naturally.

## Interaction model

### CLI and TUI are unified

`oc new myproject` on the CLI and launching the TUI then typing `new myproject` work the same. The TUI is not a separate interface — it's the CLI with a visual context around it.

### Commands

Commands work in both CLI and TUI:

- **`new`** / **`n`** `<name>` — create and launch a new session
- **`rm`** / **`delete`** / **`d`** `<name>` — remove the alias and kill the tmux session if running
- **`stop`** `<name>` — graceful shutdown (ctrl-c + ctrl-d), keep the alias
- **`restart`** `<name>` — graceful stop, re-launch, resume
- **`mv`** `<name> <new-dir>` — change a session's directory (must be stopped), then attach

CLI-only commands (not surfaced in TUI — these are plumbing for tooling like `spin-out`):

- **`alias`** `<name> [dir] [-- opencode-args...]` — save a named session mapping
- **`unalias`** `<name>` — remove a mapping
- **`migrate`** — one-time import of old aliases file into SQLite

### Keyboard interaction

The command bar lives at the bottom of the TUI. Typing goes there.

- **Up/down** to move between sessions
- **Enter** to execute the current action for the highlighted row
- **Left/right** to cycle through available actions for the highlighted row (attach, stop, rm, restart, etc.). Left/right should wrap.
- **Type to filter** — narrows the session list (see filtering below)
- **Space** — switches from filter mode to command mode, because session names can't contain spaces. If the typed text isn't a valid command, show an error.
- **Esc** clears the filter/command (or quits if nothing to clear)
- **Backspace** — if it removes the last space, immediately returns to filter mode

No single-letter hotkeys. They conflict with type-to-filter.

The **selected action** is shown prominently at the bottom of the screen — large and obvious. It persists when you move between rows (does not reset to default on row change). This makes repeated actions on multiple sessions fast.

### Numeric IDs

Every session gets a stable, dense numeric ID assigned from storage. IDs are stable while the session exists but get reused after deletion — the next new session fills the lowest available gap. Session names can't be plain numbers — this keeps names and IDs unambiguous.

Example:
```
oc new → 1
oc new → 2
oc new → 3
oc rm 2
oc new → 2  (fills gap)
oc new → 4  (next after highest)
```

Sessions can be referenced by name or numeric ID interchangeably — `oc 3` and `oc dc` both work on the CLI.

### Filtering and groups

Typing filters the session list. Filtering matches against all fields: numeric ID, name, directory, and OpenCode session ID. Each session appears once, in the highest-priority group where it matched. Results are grouped by which field matched, in this order:

1. **Numeric ID matches** — only shows when the filter is a valid number or number-prefix
2. **Name matches**
3. **Directory matches**
4. **OpenCode session ID matches** — pasting in a session ID like `ses_44b3d03b...` filters to the right session

Within each group, sort by match quality:
1. Exact match
2. Prefix match
3. Contains match

Example — typing `1`:

```
 ID  Name
─────────────────
  1  job-fix-abc      ← exact ID match
 12  thingymyob       ← ID prefix match
 31  tofu-x           ← ID contains "1"

 16  1abc             ← name prefix match
 54  aid-11000        ← name contains "1"
 43  fix-1            ← name contains "1"
```

Enter selects the top filtered result. The numeric ID case falls out naturally from the grouping — an exact ID match is always first.

When the filter is empty, all sessions are shown in a single ungrouped list.

### "New session" is a row, not a hotkey

The session list includes a special "New session" row. Selecting it and pressing Enter starts the new-session flow (prompting for name, etc.) **inside the TUI**, not by closing the TUI and dropping to a shell prompt. Typing `new <name>` as a command is the direct/expert path; the row is the discoverable/guided path.

Default selection when the dashboard opens depends on context:
- **In `$HOME` or a directory with no matching sessions**: "New session" row is selected by default
- **In a directory with multiple matching sessions**: the first matching session is selected. "New session" appears after the matches, before the remaining sessions.

### Visual design

- Must work in both light and dark terminal themes. Don't assume dark mode.
- Clean layout with borders and summary header (counts of attached/detached/saved).
- Dynamic column widths based on content.
- Directory abbreviation: when the dir basename matches the session name, abbreviate to highlight only what's different.
- Highlighted row should stand out clearly but not be garish.

## Storage

### SQLite database

Session configuration is stored in a SQLite database (not a flat file). Location: `~/.config/oc/oc.db` or similar.

Each session stores: numeric ID, name, directory, OpenCode session ID, OpenCode PID (runtime, not persisted), OpenCode args.

### Uniqueness constraints

- No two aliases can share a tmux session
- No two aliases can share an OpenCode PID
- No two aliases sharing a directory — TBC (except home directory)

### Directory sync with OpenCode

oc's database is the source of truth for the intended directory of a managed session. OpenCode's own database is observed/read-only state. oc should read OpenCode's SQLite DB to detect drift but should not write to it.

On normal launch: oc starts `opencode --session <id>` from oc's saved directory, which lets OpenCode update its own state naturally. If the directories diverge (e.g. from a manual change), oc should detect and surface the drift.

### Migration from old aliases file

The old fish prototype used a human-editable aliases file at `~/.config/oc/aliases`. New `oc` reads this file (read-only) to display old sessions in the TUI alongside SQLite sessions, with a visual indicator showing they haven't been migrated yet.

Migration is an explicit user action (`oc migrate`), idempotent — safe to run multiple times. For each old-file alias: import if no equivalent SQLite row exists, skip if already migrated, report conflicts without overwriting. The old file is never modified or deleted automatically. Once all sessions are migrated and the user confirms, the migration code gets removed from the codebase entirely.

## Technical context

- The tool manages tmux sessions named `oc-<name>`. It is not a general tmux manager.
- `oc` is always run from outside tmux. It attaches to tmux sessions as a parent process. No `switch-client` — the TUI is the navigation hub.
- Creating a session runs: `tmux new-session -s oc-<name> -c <dir> opencode <args>`
- This is a Rust CLI tool in the same family as `trunc` and `tmux-bridge` (see those repos for CI, release, testing, and landing page patterns). Follow the `agent-tools` skill.
- Tests should be black-box E2E tests using real tmux sessions and PTY interaction (like tmux-bridge's test architecture).
- The project should use `tdd-ratchet` (the Rust `cargo install tdd-ratchet` tool, NOT a custom ratchet.py script).

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
