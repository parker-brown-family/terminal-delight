# Plugins — the extension surface

Terminal Delight is both an MCP **server** (watched by orchestrators) *and* an MCP
**client** (it drives plugins). Plugins are stdio MCP processes that surface extra
data on the agent wall, the graveyard, or globally.

## Why it matters

The wall isn't a closed box. Anything that speaks MCP can light up a surface in TD —
token-savings ledgers, context harvesters, your own tooling — discovered from a
manifest, launched on demand, with a wedged plugin unable to freeze the UI.

## Features

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **MCP client host** | Discovers, launches, and JSON-RPC-handshakes plugins over stdio; separate from the server transport | `app/src/plugins.rs` | Shipped |
| **Discovery** | Scans `~/.config/terminal-delight/plugins/*/plugin.json` + built-in fallbacks | `discover(home)` | Shipped |
| **Manifest** | Each plugin declares name/version/description/command/args/env/scope + per-surface **actions** (agent / graveyard / global) | `PluginManifest`, `action_for(surface)` | Shipped |
| **Timeout + isolation** | 10 s per tool call; isolated stdio; reader thread so a hung plugin can't block paint | `RPC_TIMEOUT`, `PluginClient` | Shipped |

## ⭐ Built-in: LeanCTX token-savings → [leanctx.com](https://leanctx.com/)

The `</> LeanCTX` plugin shows, on the wall, the **token savings** that
[lean-ctx](https://leanctx.com/) achieved by compressing each agent's context — a
live, quantified "this is how much cheaper your agents got." lean-ctx **precomputes**
the savings; TD reads and displays them. It's the flagship plugin and the canonical
backlink to leanctx.com.

- Evidence: `plugins.rs::builtin_leanctx_savings` / `resolve_leanctx_mcp`.
- Known gap: per-agent attribution (the ledger currently keys `agent_id:"local"`);
  full per-agent cost breakdown is the next step.
- Demo: `TD_SAVINGS_DEMO`.

## ⭐ Subscription usage — the other half of the same budget

The `</>` card has two faces, switched by the chips in its header. **savings** is
lean-ctx's compression ledger: tokens TD's own plumbing never had to send.
**usage** is what the subscriptions actually charged for the tokens that did go —
one tab per plan, with the ceiling each is up against.

Per subscription the card draws the plan and its limits (a meter apiece, coloured
by pressure rather than by theme, with the time until each window resets), today's
tokens and prompts, the last seven days, and where the tokens went by model. A
prepaid plan reports a draining credit balance instead of resetting windows.

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

- Evidence: `app/src/usage.rs` (record contract, discovery, countdowns, the
  compiled-in collectors and their runner; 16 tests), `app/src/vendor/README.md`,
  `main.rs::render_usage_body`, `main.rs::open_usage` / `refresh_usage`.
- Demo: `TD_USAGE_DEMO` (fictional). Dev: `TD_USAGE_LIVE` (this machine's real
  records — never for capture).

## Optional: jev — typed judgement, and only if you have it

Every other plugin here reads something already on the disk. This one calls a
hosted model behind an API key — TypeSafe's System One — and that makes it the one
place where a public repository could acquire a paid third-party
dependency. It does not, and the design is the proof.

**Terminal Delight's source knows nothing about Jev but the plugin's name.** No
HTTP client, no key, no endpoint, no model id, and not one line of question text.
Everything that knows what Jev *is* lives in `plugins/jev-mcp/jev-mcp`. That is
not a convention to be remembered — `plugins.rs::source_says_nothing_about_jev_but_its_name`
walks every `.rs` file under `app/src` and fails the build if an endpoint, a key
name or a model id appears in any of them.

**Installing is the opt-in, and it is the only one.** `resolve_jev_mcp` looks on
`PATH`, in `~/.local/bin` and in `~/.cargo/bin` — and deliberately *not* at the
copy bundled in this checkout, which is what `resolve_leanctx_mcp` and
`resolve_cdx_mcp` both do. That walk is right for a ledger reader and wrong for
something that spends money over the network: cloning a repository is not consent.
The bundled copy is the thing you install *from*.

**Three absences, three answers, never collapsed.** This is the whole contract:

| What is missing | What happens |
|---|---|
| The server isn't installed | `discover()` never returns it. No Jev surface exists anywhere — not a greyed one waiting for a key, and above all not a zero. |
| Installed, but no client or no key | Every tool answers `available: false` **with a reason**. Callers draw *unknown*. |
| Configured, but a question abstained | That answer is `null` with its own reason; the other answers still stand. |

Every measured field is nullable and nothing defaults: `probabilities` is `null`
rather than `{}` when none came back, `counts.unknown` is reported on its own line
and added to no other count, and `needs_you` is `{total, measured, judged,
unknown}` so a total of 0 sitting beside 6 unknowns cannot read as calm.

**It writes no client.** The wire format, the ports and the abstention semantics
already exist in the `jev` package; the server imports it and adapts MCP onto it,
and says `available: false` when it cannot. Point `TD_JEV_HOME` at a checkout or
`pip install jev`.

**Measurement and judgement never share a field.** Two things are answered without
asking the model, because they are facts read off the pane: a pane running a plain
shell is not an agent, and a pane already known to be awaiting input already wants
you. Those rows come back `source: "measured"` with no probability. A pane whose
`awaiting_input` is `null` — TD could not tell — *is* asked, because unknown is
not false.

Four tools: `jev_status` (makes no call — the cheap check before drawing),
`judge` (any typed question set over any state, batched into one request),
`workspace_weather` (per-pane state) and `rail_weather` (ranks the project rail's
ticker frames). Every question's wording lives in the plugin, so TD's own source
stays free of Jev vocabulary.

**`rail_weather` is a Score per frame, not a Choice, and the rail's own code is
why.** `ProjectState::frames()` is an accumulator: past the calm early return
nothing returns and nothing clears, so sections 1 through 7 all push into one
vector — and section 7 is commented *"the derived sentence, last, so it lands
after the facts it sums"*. Several frames are true of one reading at once and one
of them contains its neighbours, so a Choice would spread its mass over options
that do not compete and come back flat. The Score's levels are about what a
person would **do**, never about how much a line covers, because a rubric that
rewards coverage hands first place to the summary frame forever.

**The raw score carries a measured positional bias, so the ranking is banded.**
Repeating one batch four times moved a frame by at most 0.22 levels (stdev 0.10).
Reversing the frame array moved scores by 0.43 on average and 1.26 at worst, and
two byte-identical frames sat ~0.98 apart in every run with the *earlier* one
always winning — a gap that flips sign when the array is reversed. So the model
partly anchors on the order it is handed, which is the order the ranking exists
to improve on. `_band_width` therefore coarsens scores before ordering and lets
the rail's own order stand inside a band. That number is a **resolution**, not a
confidence floor: it is the worst measured positional effect rounded up, from two
fixtures on one day, and `TD_JEV_RAIL_BAND` overrides it.

**The rail's own severity is withheld from the model on purpose.** Each frame
arrives carrying a `tone`, and passing it along looks free. Measured on four
deliberately ambiguous frames, holding the text fixed and flipping only the tone:
with the tone in the state and the question silent about it, scores moved 0.40 on
average; with the question naming it, 0.41. Naming it costs nothing — **putting
it in the state at all is what moves the answer**, four times the noise floor,
before a word of the question mentions it.

So it is dropped before the request. The value of this question is a *second*
opinion, and the caller already enforces severity with an ordering floor that a
warning cannot fall through — a constraint, which needs no agreement from here.
Two readings that agree because one of them read the other are one reading. What
is *not* measured is whether that shift would have been correct, so this is a
switch rather than a deletion: `TD_JEV_RAIL_TONE=1` sends it and names it. With
it off the experiment becomes its own control — flipping a withheld field has no
causal path, and that arm reads 0.16, the noise floor measured under the same
conditions instead of quoted from elsewhere.

No threshold ships. A confidence floor is a measurement, nobody has swept labelled
cases for these questions yet, and a default would be a number nothing measured —
so `TD_JEV_MIN_CONFIDENCE` defaults to no floor and every answer carries its own
probability.

- Evidence: `plugins/jev-mcp/jev-mcp`, `plugins.rs::builtin_jev` / `resolve_jev_mcp`,
  and three tests: the checkout-never-resolves rule, the rows-are-never-blank
  invariant driven against the real server, and the source gate.

## Built-in: context-delight harvest

If the `cdx-mcp` binary is on `PATH`, TD auto-registers
**[context-delight](https://github.com/parker-brown-family/context-delight)** as a
plugin — harvest a live session into a portable `.cdx` / lean-ctx package, right
from the wall. `plugins.rs::builtin_context_delight` / `resolve_cdx_mcp`.

## Status

**Shipped** (client host, discovery, both built-ins). Per-agent LeanCTX attribution
is the tracked follow-up.
