---
name: oc
description: When managing or reattaching long-running OpenCode sessions in tmux
---

# oc

Use `oc` to view, jump to, and preserve OpenCode sessions in tmux.

## Install

```bash
curl -Lo ~/.local/bin/oc https://oc.maxeonyx.com/releases/oc-x86_64-linux
chmod +x ~/.local/bin/oc
```

## Usage

```bash
oc             # Open the interactive dashboard
oc dc          # Attach to or launch the named session directly
oc alias dc .  # Save a named session for this directory
oc rm dc       # Remove the saved session and kill it if running
```

Sessions live in tmux so they survive SSH disconnects, terminal crashes, and laptop sleep.
