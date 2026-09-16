# A hosted pane's mode is reconciled, and an unclassified pane says so

Issue: `parker-brown-family/terminal-delight#462`
Difficulty: **6/10** — one combined plan page, one approval. A small change by
line count that decides what an entire surface is allowed to see, and it was
wrong for hours in front of everybody without one signal.

## What was measured, on the live session

Host `td-fc6957f-main`, pid 876777, up since 2026-09-16 10:31:57. Window
`td-8d9b25e-paint-the-outer`, pid 2887909, attached 13:07:36.

Three readings, taken against each other:

| source | says about pane 1 (pid 876819) | … and ten others |
|---|---|---|
| `/proc/876819/stat` tpgid → `comm` | `claude` | `claude` / `codex` |
| the host's own `list-panes` reply | `claude` | `claude` / `codex` |
| the window's MCP `list_panes` | `SHELL`, `is_agent: false` | `SHELL` |

**The host is right. The window is wrong. They have disagreed for five hours.**

The issue's three invalidation criteria were run before anything was changed:

1. *"It corrects itself within ten minutes."* It has not corrected itself in
   five hours. **Does not hold.**
2. *"The panes are not hosted."* Every one of them is a direct child of the host
   process and the host holds its pseudoterminal. **Does not hold.**
3. *"`rail_rows` observes non-agent panes after all."* It builds an
   `Observation` for every leaf now — but `attention::project` drops
   `PaneKind::Shell` before a row exists (`app/src/attention.rs:431`), and
   `needs_input`, `agent_working` and `rail_state` each return early on
   `!mode.is_agent()`. The classification is not cosmetic. **Does not hold.**

The issue's second bucket — *roughly ten panes produce no census row at all* —
**does not reproduce today** and is reported closed by its author's own stated
criterion. At the time of writing the host holds 24 panes and the window shows
22; the two it does not show are bash shells spawned 19 seconds before the
window started, not ten agents. That is a separate, much smaller question and it
is not addressed here.

## The mechanism, which is worse than a missed message

The window has exactly one way to learn what a hosted pane is running after the
pane is built: a `Push::Mode` event, and the protocol says plainly that it is
sent *"on the change, not on a clock"* (`hostproto.rs:181`). Four things follow
from that, and all four are live:

- **Pushes are opt-in and late.** `listen_to_the_host` is the last statement of
  `build_attached` (`main.rs:4926`), after every pane has been built. Anything
  the host said while the layout was being restored was said to nobody.
- **The feed is a single thread with no reconnect.** `hostctl::watch` spawns a
  reader that `break`s on any read error and is never restarted. One hiccup
  deafens the window permanently, silently — its stderr is `/dev/null`.
- **There is no path that re-asks.** `list-panes` carries the current mode of
  every pane and is read exactly once per pane, at attach.
- **And the change almost never comes.** A watcher connected to this host for
  forty seconds, across 24 panes and ~20 working agents, received **zero** mode
  pushes. `next_mode`'s sticky rule is doing its job: an agent holds the
  foreground process group and its classification is stable. So the one channel
  that could repair a wrong answer is a channel that, in practice, is silent.

A window that is wrong once is therefore wrong until it is restarted. Which
entry put these particular eleven panes into the wrong state cannot now be
recovered — the window logs nothing — and it does not need to be: every one of
the four holes above produces exactly this, and reconciliation closes all four.

## The second defect, underneath the first

`TerminalView.mode` is a `PaneMode`, and a pane is constructed holding
`PaneMode::Shell` (`pane.rs:3144`). For a pane this window forked, that is a
reading — it just started a shell. For a pane the host owns, the window has read
nothing at all, and **"nobody has told me" and "it is a shell" are stored as the
same value.**

That is the collapse the house rule forbids, and it is what makes this failure
invisible rather than merely wrong. `attention::PaneKind` already has the right
word for it, written for exactly this case and never yet constructed:

> `Unknown` — the pane exists and nothing could be established about what runs
> in it. *It becomes reachable when a client is shown a pane the host owns and
> has not described.*

A pane in that state must draw a row saying so. The unknown lane is already
built to hold it: shown, and never counted toward the red number.

## What changes

**Slice 1 — the window asks again.** A sweep beside the divergence guard reads
`list-panes` on the background executor every five seconds and applies the
host's answer to any pane that disagrees. A pane the host reports **no** mode
for is left alone, never told it is a shell. The push feed stays, demoted from
the correctness path to what it always should have been: a latency
optimisation.

The decision is a pure function, `modes_to_apply`, so the rule can be tested
without a window: apply where the host has an answer and the window disagrees,
and nowhere else.

**Slice 2 — unclassified is its own state.** `PaneMode::Unknown`, which a
*hosted* pane is born holding; a window-owned pane is still born `Shell`,
because there the window did fork the shell and that is a reading. `is_agent()`
is false for `Unknown` — we do not know, and no caller changes behaviour.
`rail_rows` maps it to `PaneKind::Unknown`, which projects into the unknown
lane with its own words: **"Not described by the host"**, read by **"census"**,
never "screen could not be read", which would assert something about a screen
that is perfectly legible.

## Done when

- A window whose panes are already running agents agrees with the host within
  one sweep, with no push sent. Proven by `modes_to_apply` over a census where
  the host says `claude` and the window says `Shell`.
- A pane the host reports no mode for is never assigned one.
- A hosted pane nobody has described reports `PaneKind::Unknown`, draws a row
  in the unknown lane, and is not added to the count.
- `cargo test -p terminal-delight` green; `cargo clippy` clean.

## Deliberately not done

- **The second bucket** (panes the host holds that the window shows no row
  for). It does not reproduce at the scale reported, and a fix aimed at a
  two-pane discrepancy of unknown provenance would be a guess.
- **Reconnecting `hostctl::watch`.** Reconciliation makes the feed
  non-load-bearing, which is the cheaper and more durable repair. A reconnect is
  worth filing separately, and is worth less once the sweep exists.
- **A window-side `/proc` fallback.** The window can read `tpgid` from outside —
  `capture_via_proc` already does, which is why these panes' resume recipes are
  right while their modes are wrong — but #336 removed that second watcher on
  purpose. The host is the better-positioned observer; the fault was never that
  it did not know, only that nothing asked it twice.
