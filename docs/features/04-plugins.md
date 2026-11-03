# Plugins, and why the usage collectors add no dependency

How plugins work, and the usage card, are described on the docs site, where each
page was checked against the code and names the commit it is true of:

- Plugins (the MCP servers Terminal Delight calls): [https://docs.terminal-delight.brownfamilysports.com/plugins](https://docs.terminal-delight.brownfamilysports.com/plugins) (source: [`docsite/pages/plugins.html`](../../docsite/pages/plugins.html))
- Usage and vitals (the `</>` card's usage face): [https://docs.terminal-delight.brownfamilysports.com/usage](https://docs.terminal-delight.brownfamilysports.com/usage) (source: [`docsite/pages/usage.html`](../../docsite/pages/usage.html))

What stays here is the argument `app/src/vendor/README.md` points to: why
shipping Omarchy's usage collectors inside the binary does not make Omarchy a
dependency. It was checked against `app/src/usage.rs` on 2026-11-01.

## The usage records

**TD collects none of this.** It reads one JSON record per subscription out of a
state directory, and that is the entire contract:

```
${XDG_STATE_HOME:-~/.local/state}/omarchy/agents/usage/<id>.json
${XDG_STATE_HOME:-~/.local/state}/terminal-delight/agents/usage/<id>.json
```

Both are read, TD's own last, and a record in both resolves to the newer
`updatedAt`. The shape is Omarchy's — its `omarchy-agent-usage-<agent>` collectors
publish it and its agents widget reads it — so **on an Omarchy box TD picks up
records somebody else is already writing and refreshing, for free**.

**This does not make Omarchy a dependency, and the binary carries the proof.**
The collectors are standalone Python 3 stdlib scripts that read the agent's own
files (`~/.claude/projects`, Codex session files) and ask the vendor for the
authoritative limits; nothing in them touches Hyprland, the bar, or Omarchy's
runtime. So **terminal-delight ships them**, byte-identical and MIT-attributed
(`app/src/vendor/`, compiled in with `include_str!`), and can write the records
itself:

```
terminal-delight agent-usage update
```

That unpacks the collectors to `~/.cache/terminal-delight/agent-usage/`, runs each
one, and publishes what it prints to
`${XDG_STATE_HOME:-~/.local/state}/terminal-delight/agents/usage/`. `list` names
the collectors the binary carries; `where` prints the directories the panel reads.
A collector that fails never fails the run — a machine signed in to Claude and not
to Codex is the normal case. `python3` is a **soft** runtime requirement: without
it the panel says so and still draws whatever is on disk.

Adding a subscription never touches TD either: publish a record under a new `id`
and the card gains a tab.

A refresh is optional and never blocks a frame. TD runs
`omarchy-agent-usage-update` if it is on `PATH` — it is wired to that box's own
per-agent enable/disable settings, so using it respects a subscription the user
turned off — else a packaged `td-agent-usage`, else this binary's own
`agent-usage update`. It fires only when the newest record is over five minutes
old, and always on a pool thread: asking Anthropic's usage endpoint takes seconds,
and the card is up on the frame of the click with whatever it already has.
