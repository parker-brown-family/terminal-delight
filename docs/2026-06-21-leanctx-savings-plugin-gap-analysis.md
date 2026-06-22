# LeanCTX "token savings" in the TD agent dashboard — gap analysis

**Date:** 2026-06-21 · **Status:** design / feasibility (no code yet)
**Premise tested:** *"Live token-savings across ~36 agents is probably heavy →
so make it a CLICK-to-get-the-number button on the agent wall."*
**Verdict:** the heavy-compute premise is **false**. A live total is cheap; the
only real gap is **per-agent attribution**. Click-to-compute is still the right
**v0** — for plugin-doctrine purity and zero idle cost, not because live is slow.

---

## TL;DR

| Question | Answer |
|---|---|
| Does TD have to compute token savings? | **No.** lean-ctx precomputes `saved_tokens`/`saved_usd` at every `ctx_*` call and persists it. TD reads a number. |
| Cost of the TOTAL (all-agents) number? | `lean-ctx gain --json` = **180 ms / 63 MB RSS**, cold. One process. |
| Cost of a *live* total? | Negligible. lean-ctx's own `gain --live` already refreshes **1 Hz**. A `stat()`+parse of `stats.json` (23 KB) or a tail-delta of the ledger is sub-ms. |
| Cost across **36 agents**? | Cheap **if you read the one shared rollup file**, ruinous if you spawn `gain` per-agent (63 MB × 36). The data is already aggregated in a single JSON. |
| What's actually missing? | **Per-agent SAVED tokens.** The savings ledger tags every entry `agent_id:"local"`; per-agent identity lives in a *different* file (`cost_attribution.json`) that tracks cost but not savings. The two aren't joined upstream. |
| Right shape? | A **plugin** (standalone MCP server) exposing a `savings` tool, wired to TD's existing plugin host exactly like context-delight. Button = the `</> LeanCTX` logo on `global` + `agent` surfaces. |

---

## Evidence (this box, today)

lean-ctx data dir is **`~/.lean-ctx/`** (not `~/.local/share/lean-ctx`, which only
holds the daemon socket).

**1. Savings are precomputed per call — `~/.lean-ctx/savings/ledger.jsonl`** (1.2 MB,
2 645 lines after ~1 month, live-appended; hash-chained / tamper-evident):

```json
{"ts":"2026-06-22T06:55:27Z","tool":"ctx_read","baseline_tokens":102828,
 "actual_tokens":24,"saved_tokens":102804,"saved_usd":0.25701,
 "repo_hash":"dd350f2792ecbec3","agent_id":"local",
 "prev_hash":"…","entry_hash":"…"}
```

`baseline_tokens` (what native Read/Grep would have cost) and `actual_tokens` (the
compressed result) are tokenized **at the moment of the call** — the only place
the work *can* happen, because compression needs it anyway. `saved_tokens` is a
subtraction. **TD never tokenizes anything.**

**2. Running aggregate already summed — `~/.lean-ctx/stats.json`** (23 KB):
`total_input_tokens`, `total_output_tokens`, per-tool counts, per-day series.

**3. `lean-ctx gain --json` — the clean primitive (180 ms / 63 MB):**

```json
{"summary":{"tokens_saved":64614131,"gain_rate_pct":81.5,
            "net_tokens_saved":64614131,"avoided_usd":161.5,"roi":2.10,
            "score":{"total":70,"compression":82,...}},
 "tasks":[{"category":"Exploration","tokens_saved":59940838,...}, ...],
 "heatmap":[{"path":".../main.rs","tokens_saved":10131759,"compression_pct":98.2}, ...]}
```

Related CLI surfaces that already exist: `gain --live` (1 Hz in-place refresh),
`gain --deep` (adds an **agents** section), `gain --cost` (agent cost attribution),
`gain --graph`, `lean-ctx watch` ("live observatory"), `lean-ctx dashboard`
(localhost:3333). lean-ctx is third-party: `github.com/yvgude/lean-ctx` (v3.8.9).

**4. Per-agent identity DOES exist — `~/.lean-ctx/cost_attribution.json`** (62 KB),
keyed by `agent_id`:

```json
"mcp-426715-83162324":{"agent_type":"claude-code","total_input_tokens":4965,
 "total_output_tokens":16270,"total_calls":41,"cost_usd":0.175,
 "tools_used":{"ctx_shell":12,"ctx_edit":15,"ctx_read":9,"ctx_search":5},
 "first_seen":"…","last_seen":"…"}
```

Agent id format = `mcp-<pid>-<hash>`, type `claude-code`. This is the bridge to
TD's panes — **but this file has token COST, not token SAVINGS.**

---

## The gap, precisely

Two facts live in two files that aren't joined:

- `savings/ledger.jsonl` → **savings** (`saved_tokens`/`saved_usd`) but `agent_id:"local"` (global / per-repo only).
- `cost_attribution.json` → **per-agent identity + cost** but no `saved_tokens`.

So **out of the box** TD can show, truthfully and cheaply:

- ✅ **Total tokens saved** (all agents) — `gain --json → summary.tokens_saved`.
- ✅ **Savings by repo** (`repo_hash`) and **by file** (heatmap) and **by task category**.
- ✅ **Per-agent token cost + call counts + tools used** (`cost_attribution.json`).
- ❌ **Per-agent tokens *saved*** — not directly recorded. Two ways to fill it:
  1. **Estimate (ships today):** apportion total `saved_tokens` to each agent by
     its share of read/search calls (the savings-bearing tools, from
     `cost_attribution.tools_used`). Label it `≈`. Good enough for a wall field.
  2. **Exact (upstream ask):** get lean-ctx to stamp the **real** MCP `agent_id`
     into ledger entries instead of `"local"` (or add `saved_tokens` to
     `cost_attribution.json`). One-line PR conceptually; not ours to merge.

**Performance is NOT the gap.** The only way to make this slow is to do the wrong
thing: spawn `lean-ctx gain` once per agent per frame (63 MB × 36 × N Hz). The
right thing — read the single shared rollup once — is sub-second for all 36.

---

## Recommended design

### Shape: a plugin (honors plugin doctrine — `app/src/plugins.rs`)

A standalone MCP server `leanctx-mcp` (or reuse the existing lean-ctx MCP if it
exposes a savings tool) discovered via
`~/.config/terminal-delight/plugins/leanctx/plugin.json`:

```json
{
  "name": "leanctx-savings",
  "version": "0.1.0",
  "description": "Token savings from lean-ctx context compression.",
  "command": "leanctx-mcp",
  "scope": "global",
  "actions": [
    { "tool": "savings", "label": "</> savings", "surfaces": ["global", "agent"] }
  ]
}
```

The tool wraps `lean-ctx gain --json` (total + by-repo/file/task) and
`cost_attribution.json` (per-agent). Zero new TD plumbing — `run_action()` in
`plugins.rs` already does spawn → `initialize` → `tools/call` → flatten text,
exactly as the context-delight ⬇ button does. The `</> LeanCTX` logo is the
button glyph (place on the dashboard global rollup + each agent row).

### Interaction: click-to-compute is v0, cheap live is a stretch goal

- **v0 — click `</> savings`:** spawn the plugin, return the number, show in the
  same flat float popup the graveyard/plugins panels use (remember the
  `warp::set_suppressed()` rule — any new overlay must register or it bows with
  the CRT glass). Zero idle cost. Plugin-doctrine pure. ~200 ms to the number.
- **Stretch — live total:** a background poll every ~3–5 s that reads
  `stats.json`/ledger-delta directly (NOT a per-agent `gain` spawn) and updates a
  single "Σ saved: 64.6M / $161" field in the dashboard header. Microsecond reads;
  no per-frame work; one file regardless of agent count. Safe to add later.

Do **not** attempt per-agent `gain` spawns on a timer. That, and only that, is
where the 36-agent cost would blow up.

---

## Caveats / honesty

- Numbers above are **this box's real lean-ctx history** (one shared daemon, one
  `~/.lean-ctx`). If agents ever run with **separate** `LEANCTX`/data dirs, the
  "one shared rollup" assumption breaks and you'd fan out reads over N dirs (still
  cheap — N small files — just not one).
- `gain` shows `engagement: proxy_down` → savings = compression on lean-ctx-touched
  traffic, not the full provider bill. The field should read **"context saved by
  lean-ctx,"** not "your total bill saved," to stay truthful.
- Per-agent saved-tokens is an **estimate** until upstream stamps real `agent_id`
  into the savings ledger. Mark it `≈` and link the upstream issue.

## Next actions (if greenlit)

1. File upstream issue on `yvgude/lean-ctx`: stamp MCP `agent_id` into
   `savings/ledger.jsonl` (or add `saved_tokens` to `cost_attribution.json`).
2. Build `leanctx-mcp` thin wrapper (or confirm lean-ctx's own MCP exposes a
   savings tool) + `plugin.json`.
3. TD: add `</> savings` button to the dashboard global rollup + agent rows;
   render result in a warp-suppressed float popup.
4. Optional: background `stats.json` poller for a live Σ field.
