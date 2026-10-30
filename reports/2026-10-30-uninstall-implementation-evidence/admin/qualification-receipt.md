# Terminal Delight was removed

15 steps done, 0 not attempted, 0 failed, 0 blocked. Records: kept.

## What happened

- **done** — edit ~/.cache/td-uninstall-admin/qual-home/.claude/settings.json — take out TD's entries (1 hook) and nothing else. removed 1 entries
- **done** — close session hosts in ~/.cache/td-uninstall-admin/qual-home/run/terminal-delight — ask each session host to save and close. 0 host(s) asked to save and close
- **done** — close session hosts in ~/.cache/td-uninstall-admin/qual-home/tmp/terminal-delight-1000 — ask each session host to save and close. 0 host(s) asked to save and close
- **done** — stop processes running 2 TD executable(s) — close TD's windows and the MCP relays agents started, matched by executable. 1 process(es) closed
- **done** — check TD is no longer a default terminal in ~/.cache/td-uninstall-admin/qual-home/.config/xdg-terminals.list — make sure the desktop still has a terminal once TD is gone. no terminal list
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.local/bin/td-agent-hooks — td-agent-hooks, installed by TD (a shipped scripts/td-agent-hooks). removed
- **done** — remove TD's loose files in ~/.cache/td-uninstall-admin/qual-home/tmp — demo layouts TD left in the temp directory. 0 file(s) removed
- **done** — remove TD's loose files in ~/.cache/td-uninstall-admin/qual-home/run — pictures and sounds TD unpacked beside its runtime directory. 1 file(s) removed; 1 kept: nothing records that TD wrote them (in a runtime directory they go at logout)
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/run/terminal-delight/ — TD's runtime directory: sockets and live state. removed
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/tmp/terminal-delight-1000/ — TD's runtime directory: sockets and live state. already gone
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.cache/terminal-delight/ — TD's cache. removed
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/td-qual0001-current — build td-qual0001-current. removed
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/td-qual0002-old — build td-qual0002-old. removed
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/ if empty — the builds directory, if empty. removed
- **done** — remove ~/.cache/td-uninstall-admin/qual-home/.local/bin/terminal-delight — the terminal-delight launcher. removed

## Left for you

- **Project .mcp.json files** (yours) — a project-scope TD registration lives inside that project's repository, which this does not search. To do it: `grep -rl --include=.mcp.json terminal-delight <your projects directory>`
- **Your settings, sessions and records are kept** (yours) — ~/.cache/td-uninstall-admin/qual-home/.config/terminal-delight and ~/.cache/td-uninstall-admin/qual-home/.local/state/terminal-delight stay where they are `~/.cache/td-uninstall-admin/qual-home/.config/terminal-delight`

This receipt, the plan and a copy of every shared file before it was edited are in `~/.cache/td-uninstall-admin/qual-home/.local/state/terminal-delight-removal/20261030-201459`. Delete that directory when you no longer need it.
