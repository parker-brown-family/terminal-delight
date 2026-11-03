# Removing Terminal Delight: the plan

Machine legion (6e3cbd12), home `~/.cache/td-uninstall-admin/qual-home`. Records: **kept**.

## Size

- settings and sessions: 12 B — `~/.cache/td-uninstall-admin/qual-home/.config/terminal-delight`
- records: 0 B — `~/.cache/td-uninstall-admin/qual-home/.local/state/terminal-delight`
- data: not present — `~/.cache/td-uninstall-admin/qual-home/.local/share/terminal-delight`
- cache: not present — `~/.cache/td-uninstall-admin/qual-home/.cache/terminal-delight`
- builds: 95 kB — `~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight`

## What it will do, in order

Terminal Delight closes at the stop step, and every pane closes with it.

1. edit ~/.cache/td-uninstall-admin/qual-home/.claude/settings.json — take out TD's entries (1 hook) and nothing else
2. close session hosts in ~/.cache/td-uninstall-admin/qual-home/run/terminal-delight — ask each session host to save and close
3. close session hosts in ~/.cache/td-uninstall-admin/qual-home/tmp/terminal-delight-1000 — ask each session host to save and close
4. stop processes running 2 TD executable(s) — close TD's windows and the MCP relays agents started, matched by executable
5. check TD is no longer a default terminal in ~/.cache/td-uninstall-admin/qual-home/.config/xdg-terminals.list — make sure the desktop still has a terminal once TD is gone
6. remove ~/.cache/td-uninstall-admin/qual-home/.local/bin/td-agent-hooks — td-agent-hooks, installed by TD (a shipped scripts/td-agent-hooks)
7. remove TD's loose files in ~/.cache/td-uninstall-admin/qual-home/tmp — demo layouts TD left in the temp directory
8. remove TD's loose files in ~/.cache/td-uninstall-admin/qual-home/run — pictures and sounds TD unpacked beside its runtime directory
9. remove ~/.cache/td-uninstall-admin/qual-home/run/terminal-delight/ — TD's runtime directory: sockets and live state
10. remove ~/.cache/td-uninstall-admin/qual-home/tmp/terminal-delight-1000/ — TD's runtime directory: sockets and live state
11. remove ~/.cache/td-uninstall-admin/qual-home/.cache/terminal-delight/ — TD's cache
12. remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/td-qual0001-current — build td-qual0001-current
13. remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/td-qual0002-old — build td-qual0002-old
14. remove ~/.cache/td-uninstall-admin/qual-home/.local/lib/terminal-delight/ if empty — the builds directory, if empty
15. remove ~/.cache/td-uninstall-admin/qual-home/.local/bin/terminal-delight — the terminal-delight launcher

## What it will leave for you

- **Project .mcp.json files** (yours) — a project-scope TD registration lives inside that project's repository, which this does not search. To do it: `grep -rl --include=.mcp.json terminal-delight <your projects directory>`
- **Your settings, sessions and records are kept** (yours) — ~/.cache/td-uninstall-admin/qual-home/.config/terminal-delight and ~/.cache/td-uninstall-admin/qual-home/.local/state/terminal-delight stay where they are `~/.cache/td-uninstall-admin/qual-home/.config/terminal-delight`
