# Vendored usage collectors

The three files beside this one are **byte-identical copies** of Omarchy's agent
usage collectors, compiled into the terminal-delight binary by
[`crate::usage`](../usage.rs) and written back out to
`~/.cache/terminal-delight/agent-usage/` to be run.

| File | Upstream |
|---|---|
| `omarchy-agent-usage-claude` | `omarchy/bin/omarchy-agent-usage-claude` |
| `omarchy-agent-usage-codex` | `omarchy/bin/omarchy-agent-usage-codex` |
| `omarchy-agent-usage-fireworks` | `omarchy/bin/omarchy-agent-usage-fireworks` |

- **Source:** <https://github.com/basecamp/omarchy>, MIT.
- **Vendored from:** the `omarchy` package, version `4.0.1-1`, on 2026-09-02.

## Why byte-identical

They are copied without a single edit, and that is the whole maintenance plan: a
`cmp` against an installed Omarchy is the entire audit, and a re-sync is a `cp`.
The moment we start patching them — to move a cache directory, to rename a key —
every future upstream fix becomes a merge instead of a copy, for a gain of
nothing a comment could not have delivered.

The visible consequence is that a collector run keeps upstream's paths even on a
machine with no Omarchy on it:

- `~/.cache/omarchy/agent-usage/` — the collectors' own cache.
- `~/.config/omarchy/agents/fireworks.json` — where the Fireworks collector reads
  a configured funding figure, if you want a balance rather than just tokens.

That is a directory name, not a dependency. Nothing in these scripts calls
Omarchy, Hyprland, or a bar: they are Python 3 **stdlib only**, and they read the
agent's own files and ask the vendor for the authoritative limits. See
`docs/features/04-plugins.md` for the full argument.

## Re-syncing

```
cmp app/src/vendor/omarchy-agent-usage-claude /usr/share/omarchy/bin/omarchy-agent-usage-claude
```

If it differs, copy the upstream file over ours, run `cargo test usage::`, and say
in the commit which Omarchy version you took it from. The record contract is the
thing to watch: `crate::usage::Record::from_json` parses what these print, and a
field that changes name upstream goes quiet here rather than loud.

## Where the records go

The runner writes each collector's stdout to
`${XDG_STATE_HOME:-~/.local/state}/terminal-delight/agents/usage/<id>.json` —
TD's own directory, never Omarchy's. On a machine with both, Omarchy keeps
writing its directory on the bar widget's timer, TD writes this one, and
`usage::read_all` resolves the overlap by whichever record is newer.
