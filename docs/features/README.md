# Feature pages: where they went

This folder was the feature catalogue: twelve pages and an environment-flag
table, written in June 2026 and extended by hand as features landed. By
September they contradicted the code in dozens of places, from the pane cap to
the bell to the name of an MCP tool (issue 803 lists them). Two copies of the
same description is how that happened, so there is now one.

**The description of every feature lives on the docs site**, at
https://docs.terminal-delight.brownfamilysports.com, built from
[`docsite/pages/`](../../docsite/pages/). Each page there was traced to the code
and says, under its title, which commit it is true of. Development work for the
next release is marked on the page where it appears.

The files here keep their names so old links still land somewhere, and each one
points at the pages that replaced it. Two keep reasoning that is not a feature
description and exists nowhere else:

- [04-plugins.md](04-plugins.md) — why shipping Omarchy's usage collectors
  inside the binary adds no dependency on Omarchy.
- [11-usage-vitals-keepalive.md](11-usage-vitals-keepalive.md) — the prompt-cache
  keepalive that was withdrawn, the measurement that outlived it, and why a
  terminal must not type into a shell on its own.

| Old page | Now |
|---|---|
| [01 Agentic / MCP](01-agentic-mcp.md) | `/mcp`, `/agent-to-agent`, `/threshing-floor`, `/restricted` |
| [02 Agent dashboard](02-agent-dashboard.md) | `/agent-wall`, `/attention` |
| [03 Agent graveyard](03-agent-graveyard.md) | `/graveyard`, `/sessions` |
| [04 Plugins](04-plugins.md) | `/plugins`, `/usage`, and the collector argument |
| [05 Visuals / CRT](05-visuals-crt.md) | `/crt`, `/focus-reader`, `/text-crawl` |
| [06 Themes](06-themes.md) | `/themes`, `/skins`, `/ui-config` |
| [07 Terminal core](07-terminal-core.md) | `/panes-and-tabs`, `/left-bar`, `/sessions`, `/documents`, `/pictures`, `/keys` |
| [08 Internationalization](08-i18n.md) | `/languages`, and [`docs/TRANSLATING.md`](../TRANSLATING.md) |
| [09 Packaging](09-packaging.md) | `/install`, and the README's License section |
| [10 Sticky notes](10-sticky-notes.md) | `/sticky-notes` |
| [11 Usage and vitals](11-usage-vitals-keepalive.md) | `/usage`, `/agent-wall`, and the keepalive record |
| [12 ctl / scripting](12-ctl-scripting.md) | `/ctl`, `/cli` |
| [ENV-FLAGS](ENV-FLAGS.md) | `/environment` |

The positioning line the old catalogue carried still holds: Terminal Delight is
where you watch your agents work and decide what they may reach of each other.
It does not orchestrate them.
