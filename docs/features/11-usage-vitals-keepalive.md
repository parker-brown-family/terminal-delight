# Usage and vitals — what the fleet costs, and whether it is dying

Three surfaces about the *economics* of running agents: how much of each
subscription is spent, whether a given session is healthy or drowning in its own
context, and the cache money that silently burns while nobody is typing.

## Why it matters

Σ session tokens — the number every agent HUD shows — rises whether a session is
healthy or dying, and therefore decides nothing. These three surfaces answer what a
token counter cannot: *am I near a ceiling*, *should this agent compact or hand
off*, and *what did the last idle hour cost me*.

## Σ usage — per-subscription spend

| Feature | What it does | Evidence | Binding / flag |
|---|---|---|---|
| **Usage panel** | How much of each AI coding subscription is spent, and how close it is to its ceiling | `app/src/usage.rs` | `ctrl+shift+u` |
| **TD collects nothing** | Every number arrives as one JSON record per subscription, written by a *collector* that knows that vendor — Claude Code transcripts plus Anthropic's OAuth usage endpoint, Codex's app-server RPC, Fireworks' billing API. TD reads the directory and renders | `usage.rs` module doc | collector-supplied |
| **Worst-limit surfacing** | The panel leads with whichever limit is closest to its ceiling, not with the largest number | `usage.rs::worst_limit` | — |
| **A second key** | `ctrl+shift+u` is a PANE chord; a second binding exists because an input method owns the first on some systems, and the help screen says so in all nine languages | #276, #277, #278 | help screen |

## Vitals — three bars and the call they add up to

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **Three bars per card** | Each agent card carries three health bars rather than one cumulative count | `app/src/vitals.rs` | Shipped |
| **The call** | The bars resolve to one verb: **RUN · WATCH · COMPACT · HAND OFF** — what to actually do about this agent | `vitals.rs::Call` | Shipped |
| **The drag, not the total** | A window grown to 550k tokens costs 550k of re-read before the agent has thought about anything, and will cost it again next turn. That recurring drag is what the bars measure | `vitals.rs` module doc | Shipped |
| **Quiet until it isn't** | Bars stay white while nothing is wrong and shout when something is — a glance at a healthy wall costs no attention | #275 | Shipped |
| **Sleeps behind a closed wall** | The vitals sweep stops when the wall is not visible; the bar reads CTX WINDOW | #274 | Shipped |

## Keepalive — removed, and the measurement that outlived it

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **Cache keepalive** | Typed into an idle agent's prompt before its prompt cache expired, keeping the session warm | removed in #317 | **Removed** |
| **AWAY** | An AWAY campfire in the menu bar marked you gone, so keepalive could tell idle from absent | removed with it | **Removed** |
| **The measurement** | Across 32,037 billed turns in thirty days on this machine: 87 cold returns, 24.2M tokens re-written, average gap 5.0 hours — about **$242, of which roughly $230 buys nothing** | the removed module's doc | measured here |

Terminal Delight no longer types into a terminal on its own initiative. The saving
was real and the mechanism was not defensible: a terminal that sends bytes nobody
asked for is a terminal you cannot trust with a production shell, and no cache
rebate buys that back. The number stays on the page because it is the honest
statement of what idle agents cost — a fact about the industry rather than a
feature we sell.

## Honesty note

The keepalive figures are one machine's thirty days, not a benchmark. Quote them as
exactly that — a measurement taken here — wherever they appear in public material,
and say the feature was withdrawn.

## Status

**Shipped**, less the keepalive. The LeanCTX savings plugin ([plugins](04-plugins.md)) is the adjacent
surface; these three are in-app and vendor-agnostic.
