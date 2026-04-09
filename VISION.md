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

Each session row shows the memory usage of the running OpenCode process (e.g. `523 MiB`). Column totals sit at the **bottom of the session list**, well-separated from the sessions themselves, and react to the current filter:

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

The input bar lives at the **top** of the TUI. Typing goes there.

- **Up/down** to move between sessions. When a filter is active, the highlight automatically tracks the top filtered result — but up/down can still override this to select a different row.
- **Enter** to execute the current action for the highlighted row
- **Left/right** to cycle through available actions (see action bar below). Left/right **skips** grayed-out (unavailable) actions and wraps.
- **Type to filter** — narrows the session list (see filtering below)
- **Space** — switches from filter mode to command mode, because session names can't contain spaces. The typed text carries over (e.g. typing `new` then space → command mode with `new ` as the input). If the submitted command isn't valid, show an error.
- **Esc** clears the filter/command (or quits if nothing to clear)
- **Ctrl-C** clears the input (does not quit)
- **Ctrl-D** quits the TUI (only with empty input)
- **Backspace** — if it removes the last space, immediately returns to filter mode

No single-letter hotkeys. They conflict with type-to-filter.

### Action bar

All available actions are shown as **separate items** in the action bar, ordered by frequency of use: **Attach, RM, Stop, Restart**. Left/right moves a **highlight** across them to select which action Enter will execute. Unavailable actions are shown **grayed out**, not hidden — so the layout is stable and the user always knows what exists. Left/right skips unavailable actions. The highlight itself communicates which action is active — no separate "Enter" label (the key help line already says `Enter run`). Action buttons should have clean padding/spacing.

**Action availability by session status:**
- **Attached:** Attach (reattach/focus), RM (remove alias + kill), Stop (graceful shutdown, keep alias), Restart (only with saved OpenCode session ID)
- **Detached:** Attach (attach to running), RM (remove alias + kill), Stop (graceful shutdown, keep alias), Restart (only with saved OpenCode session ID)
- **Saved:** Attach (launch + attach), RM (delete alias), Stop (unavailable — nothing running), Restart (unavailable)
- **No selection (empty filter results):** All actions grayed out, no active highlight

The selected action persists when you move between rows (does not reset to default on row change). This makes repeated actions on multiple sessions fast — especially cleanup flows (select RM, then arrow-Enter-arrow-Enter through sessions). If the persistent action becomes unavailable on the new row, the highlight auto-advances to the next available action.

The input bar shows the current mode: `filter>` or `command>`. This is the primary indicator of which mode the user is in.

A small **key help** line sits at the very bottom of the TUI.

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

Typing filters the session list. Filtering is **case-insensitive**. It matches against all fields: numeric ID, name, directory, and OpenCode session ID. Each session appears once, in the **highest-priority group** where it matched — group priority comes first, then match quality within the group. Results are grouped by which field matched, in this order:

1. **Numeric ID matches** — only shows when the filter is all digits
2. **Name matches**
3. **Directory matches**
4. **OpenCode session ID matches** — pasting in a session ID like `ses_44b3d03b...` filters to the right session

Within each group, sort by match quality:
1. Exact match
2. Prefix match
3. Contains match

Ties within the same quality tier are broken by database order (numeric ID ascending).

A session that has both a numeric-ID contains-match and a name exact-match still appears in the numeric ID group — group priority always wins.

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

**Enter selects the top filtered result** — not the previously-selected row. The type-filter-Enter flow must be predictable: the first match is always what you get. The numeric ID case falls out naturally from the grouping — an exact ID match is always first.

When the filter is empty, all sessions are shown in a single ungrouped list.

### Session creation

New sessions are created via the `new` command — either on the CLI (`oc new myproject`) or typed in TUI command mode (space, then `new myproject`). There is no special "new session" row in the dashboard. Creating sessions from the TUI is rare — a new session almost always needs a directory, and you can't create directories from the TUI.

Default selection when the dashboard opens: the first session matching the current working directory, or the first session in the list if no directory match.

### Visual design

The TUI should feel like it belongs alongside OpenCode in the same terminal — native to the user's environment, not imposing its own aesthetic. OpenCode's own TUI is the visual reference.

**Theme awareness:** The tool must look correct in both light and dark terminal themes. This is the highest-priority visual requirement — a tool that looks wrong in the user's theme is broken. Detect the terminal background color if possible (e.g. `xterm` OSC 11 query), or use the terminal's own ANSI palette colors so the theme mapping happens naturally. Don't hardcode RGB values for backgrounds or panel colors. The user's terminal (light pink background, SSH through VirtualBox from Windows Terminal) must work correctly.

**Color meaning:** Every color should communicate something — status, interactivity, selection state, information hierarchy. Color is not decoration. If a color doesn't carry meaning, it shouldn't be there.

**Layout:** The dashboard is **centered in the terminal** and **sized to its content** — it should not fill the entire window. It expands as needed (more sessions → taller), but there's no empty space within the dashboard area. Totals are always visible right below the last session row. When the session list exceeds the terminal height, the list scrolls and totals remain sticky at the bottom of the session area.

**Layout stability during filtering:** When the user starts typing a filter (transition from empty to non-empty filter text), the horizontal layout dimensions — column widths, total content width — lock to the current unfiltered data. Filter edits never cause horizontal resizing; the user's spatial memory is preserved. Vertical dimensions (row count, panel height) stay live and follow the filtered results — fewer matches make the panel shorter, which is expected. Background data changes while filtering can *expand* the locked horizontal dimensions (if a newly visible row needs more width) but never *shrink* them — shrinking is deferred until the filter is cleared. Clearing the filter (Esc, Ctrl-C, backspace to empty) discards the lock and recomputes fresh from current full data. Terminal resize always adapts regardless of filter state.

**Layout order (top to bottom):**
1. Input/filter bar
2. Summary header (counts of attached/detached/saved — reacting to current filter when filtering, showing totals when in command mode or unfiltered)
3. Session list with column headers + totals row (totals are conceptually part of the sessions panel — separated by a blank line, not a panel border)
4. Action bar
5. Key help line

**Borders and styling:** Panels are separated by **background color** — the terminal background is one color, and foreground panels are a different color. No line-drawing border characters (─│┌┐). Half-block characters (▀▄) create clean transitions between panel and terminal background on **both the top and bottom edges** of each panel. See OpenCode's own TUI for the exact pattern.

**Padding:** Tight and purposeful. The filter bar and action bar should have minimal vertical padding — just enough to read clearly, not enough to feel spacious. The tool should feel compact.

**Information hierarchy:** Important information should be prominent; less important information should be subdued. Specifically:
- Column headers should be clearly styled
- Group headers (when filtering) should be **full-width** and **grayed out** — present but innocuous
- Totals row should be visually distinct from sessions but within the same panel
- The input bar should have a visible cursor

**Other:**
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

Two throwaway Rust prototypes were built (one by GPT-5.4, one by Claude Opus), plus a v0.2.0 implementation that was feature-complete but visually wrong, and a v0.3.x that got the structure right but hardcoded dark-mode colors. Key findings:

- **The TUI is the point.** A print-and-prompt Rust rewrite adds nothing over the fish version. The interactive dashboard with arrow navigation is the core value of the rewrite.
- **Summary header matters.** Session-count summary (attached/detached/saved) gives immediate context.
- **Light mode must actually work.** Multiple iterations assumed dark terminal colors and were unreadable or ugly in light mode. The user's primary terminal is light mode. Don't hardcode colors — use the terminal's palette or detect the background.
- **Left/right action cycling with wrapping feels right.** Actions should be separate items with a moving highlight; unavailable actions grayed out, not hidden.
- **Type-to-filter is great but kills hotkeys.** Can't have both 'n' for new and typing 'n' to filter. Solution: drop hotkeys entirely.
- **"New session" row was wrong.** Creating sessions from the TUI is rare — a new session almost always needs a directory. Keep it as a command for the rare case.
- **Line-drawing borders are ugly.** Use background-color panel separation with half-block transitions, like OpenCode's UI. Colors are essential — bold/dim/reverse alone looks lifeless.
- **Full-window layout is too large.** Dashboard should be centered and sized to content, not filling the terminal.
- **Input bar belongs at the top.** Matches the common mental model (type then see results below).
- **`ratatui` + `crossterm` works well.** Both prototypes used this stack. `Paragraph` with `Line` spans gives better per-row control than ratatui's `Table` widget.
- **All three session states must be visible.** Both prototypes initially missed showing saved-but-not-running aliases. The merged list (tmux sessions + saved aliases) is essential.
- **Layout must not jump during filtering.** The v0.3.x content-sizing was technically correct (wider content = wider panel) but caused the UI to resize as the filter narrowed results, which felt broken.
- **Half-block borders need both edges.** v0.3.x only did top edges of panels, not bottom edges.
- **Padding must be tight.** v0.3.x filter bar and action bar had excessive vertical padding.
- **Action ordering by frequency.** Attach is most common, then RM, then Stop, then Restart. Restart is the rarest and needs a saved OpenCode session ID to be available.
- **Totals row belongs inside the sessions panel.** Not a separate panel with its own borders — a blank-line-separated row within the session table area.
