# Handoff — agent-wall vitals, shipped (2026-09-03)

Supersedes `HANDOFF-2026-09-02-agent-wall-vitals.md`, which predates #274–#278.

## Status

**Landed, merged, installed.** Six PRs, `main` at `9707696`, tree clean, no
stashes. `~/.local/bin/terminal-delight` → `td-9707696-vitals`.
**A running window holds the old inode — restart TD to see any of it.**

| PR | What |
|---|---|
| #273 | Three bars per card, `OPUS · MAX` chip, one-to-one fleet binding |
| #274 | Sweep gated on the wall being open, `edges()` cache, pid pruning, CTX WINDOW |
| #275 | RELEVANCE diverges, FATIGUE calm band, gradient spans the fill |
| #276 | Ctrl+Shift+U + help row in nine languages |
| #277 | Chord moved into the pane table (it was in a handler a terminal never reaches) |
| #278 | Ctrl+Shift+Y alternate — fcitx5 owns Ctrl+Shift+U |

## What's done

- **Three bars, each from the agent's own transcript.** CTX WINDOW is exact —
  `input_tokens + cache_creation + cache_read` on the newest assistant turn IS the
  prompt that was sent. FATIGUE is accumulated damage (compaction scars, a rising
  error rate, repeat calls, latency drift, hours). RELEVANCE is what fraction of
  the RESIDENT window still serves the task. *Verified:* 523 tests; oracle exact
  over 12 real transcripts.
- **The pair that earns the space is CTX WINDOW × RELEVANCE.** Full of ballast →
  COMPACT (cheap). Full of load-bearing detail → HAND OFF (compaction destroys
  what is in use). Both look identical on a token counter.
- **Colour means direction, length means number.** White is neutral; each bar's
  ramp is tied to the threshold `verdict` decides on, so a bar cannot look calm
  while the chip beside it says to act. *Verified visually by Parker.*
- **One `OPUS · MAX` chip** replacing MODEL/EFFORT, which never once showed a
  model — `parse_model` read a `--model` flag nobody passes.
- **Ctrl+Shift+U / Y** open Σ usage. *Verified:* Parker saw the panel; the chord
  path was proven by keystroke logging.

## How to run / verify

```bash
cargo test --bin terminal-delight --manifest-path /home/parker/Work/terminal-delight/app/Cargo.toml
```
```bash
bash /home/parker/Work/terminal-delight/scripts/td-vitals-oracle.sh
```
```bash
terminal-delight agent-vitals --parts ~/.claude/projects/<slug>/<session>.jsonl
```

The oracle must report **0 differ**. Run it after touching either
`app/src/vitals.rs` or `scripts/td-agent-vitals.mjs`. `--parts` dumps the six
fatigue components and four relevance buckets; `--keys` dumps every call key —
that pair localises any disagreement.

## Not done / next

- **#272** — `list_panes` / `pane_events` still bind two same-cwd panes to one
  session. The bars are immune (`vitals::assign` claims each conversation once,
  with tests replaying both real collisions). Port that into
  `session.rs::claude_transcript`, which falls through to newest-by-mtime.
- **#279** — Codex panes draw no bars. `total_token_usage.input_tokens` and
  `turn_context.model` make CTX WINDOW + the chip reachable; the rollout schema is
  entirely different. Do not invent FATIGUE/RELEVANCE for it.
- **#280** — the bars have never been seen at 968px or 390px. The label collapse
  keys off `card_scale`, which is a zoom control, not a width.

## Watch out

- **Do not reintroduce a bounded transcript tail.** Whole-file is deliberate and
  measured: 33MB in ~250ms, a 17-agent fleet in 560ms. A tail truncates the churn
  and retread baselines and drops compaction scars.
- **Keep the sweep gated on `mcp_menu`.** Ungated it read ~3.5MB/s of disk with
  the wall closed, because `edges()` runs over every transcript in each pane's
  project directory — one holds 47.
- **The 1M context marker is only in `cost-state`,** never in `.message.model`.
  Reading the message field alone reports a 550k window as 275% full.
- **mtime is not conversation recency.** Claude Code writes bookkeeping records
  back into transcripts a conversation has already left.
- **TD chords live in `pane.rs`'s ctrl+shift table** and arrive as emitted events.
  `Workspace::on_key` only runs when NO pane has focus.
- **Ctrl+Shift+U is claimed by fcitx5's Unicode addon** — do not hand it out as a
  shortcut on this machine.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-03-agent-wall-vitals.md`
- Session harvest: `…/episodes/2026-09-03-agent-wall-vitals.cdx` (422K, 6 facts, 1308 secrets redacted)
- lean-ctx: `ctx_session` decision breadcrumb
- file-memory: three new entries + a sharpened `gh-pr-checks-watch-not-a-poll-loop`
- Issues: #272, #279, #280 — all `follow-up` labelled, all mirrored as APES tasks
