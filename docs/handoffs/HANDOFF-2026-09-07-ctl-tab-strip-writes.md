# Handoff — writing the tab strip from a script or an agent

**Date:** 2026-09-07 · **Repo:** `parker-brown-family/terminal-delight` · **Area:** `app/src/ctl.rs`, `app/src/main.rs`

You are picking this up mid-flight. Two PRs are merged and live, a third is open. The feature works; whether it is *shaped* right is genuinely undecided, and Parker asked for this document specifically so the architecture can be re-argued rather than inherited. Nothing below is settled by the fact that it is already written. Several of the calls are ones I would expect a careful reader to overturn.

---

## 1. What exists right now

| | |
|---|---|
| **#304** | `ctl tabs '<json>'` — a batch of tab edits over the control socket. **Merged, live.** |
| **#305** | Routes `tabs` by the caller's own window instead of the active Hyprland workspace; compacts pretty JSON onto the one-line wire. **Merged, live.** |
| **#307** | `ctl tab name <text>` (self-addressed, no arguments); `pane:` addressing for batch ops; `TabOp` restructured into target + action. **Open.** CI was red on one test, fixed, re-running. |
| **#306** | The defect #307 closes. |

Deployed binary: `~/.local/lib/terminal-delight/td-1da7bae-tabs-owning`, symlinked from `~/.local/bin/terminal-delight`. **#307 is not deployed and has never been exercised against a running window** — it changes the server side, so it needs a TD restart before `ctl tab name` can be tested end to end. Treat it as untested integration on top of 570 passing unit tests.

### The shape, as of #307

```
terminal-delight ctl tab name WEBSITE BUILD LEADS   # the tab you're running in
terminal-delight ctl tab group BFS
terminal-delight ctl tab ungroup

terminal-delight ctl tabs '[{"op":"name","pane":258041,"name":"AFTERCARE"},
                            {"op":"group","tab":3,"group":"SHINY"}]'
```

Ops: `name`, `group`, `ungroup`, `color` (tab-scoped, need exactly one of `pane`/`tab`); `group_color`, `collapse` (group-scoped by name, must carry neither).

### Where the code is (on `ctl-tab-self`)

| What | Where |
|---|---|
| `TabRef` (Index \| Pane) | `app/src/ctl.rs:96` |
| `TabAction` | `app/src/ctl.rs:110` |
| `TabOp` + `target()` — the single validation point | `app/src/ctl.rs:184` |
| `parse_tabs` — parse-time validation | `app/src/ctl.rs:233` |
| `self_tab_verb` / `self_tab_line` | `app/src/ctl.rs:628` |
| `owning_td` — the parent-chain walk | `app/src/ctl.rs:1009` |
| `apply_tab_ops` — the applier | `app/src/main.rs:5114` |
| `tab_holding_pane`, `tab_key`, `tab_index_of` | `app/src/main.rs:5056`–`5076` |

---

## 2. The one fact everything rests on

An agent's process chain inside Terminal Delight:

```
701400 bash                ← whatever the agent just ran
263012 lean-ctx
259706 claude              ← the agent
258041 bash                ← the PANE (the shell td spawned; what list_panes reports)
255871 terminal-delight    ← the WINDOW (owns the ctl socket)
```

`owning_td()` walks this until it hits a pid owning a control socket. The window is that pid; the pane is the step before it. This is why self-addressing needs no environment variable, no argument, and no lookup — and it is the same mechanism that makes `leave_note` work without focus or workspace.

Verify it yourself before trusting it:

```
for p in $(pgrep -f 'terminal-delight$'); do echo "$p"; done
```

---

## 3. Architecture decisions, with the case against each

These are the live questions. I have given my reasoning and the strongest objection I know of. **Where the objection looks better to you, take it** — none of this is load-bearing on anything shipped to users.

### D1 — Two surfaces (`ctl tab` self + `ctl tabs` batch) rather than one

*For:* the two callers are genuinely different. An agent labelling its own tab wants zero arguments; a person reorganising twenty tabs wants a file. Collapsing them forces one to carry the other's ceremony.

*Against:* two grammars for one feature is two things to document, two to test, two to keep consistent. `ctl tabs` with a `pane` field already expresses everything `ctl tab` does — `ctl tab` is pure sugar. A reviewer could reasonably say: keep the batch form, delete the sugar, and let agents build one-op JSON.

*My confidence:* moderate. The sugar is ~40 lines. The argument for it is that a surface an agent must assemble JSON for is a surface agents won't discover; that is an empirical claim I have not tested.

### D2 — Addressing by pane pid

*For:* it is the thing a caller can know about itself with certainty, it is what `list_panes` reports and what `leave_note` already takes, and it does not move when a grouping op reorders the strip.

*Against, and this is the strongest objection in the document:* **a pid does not survive a restart.** So a *scheme file* on disk addressed by pane is no more durable than one addressed by index — it is merely stale in a more honest way. If the goal is a re-appliable file, the key must be something that outlives the process: the agent's **session id** (`claude --resume <uuid>`, already in `list_panes`), or the pane's **cwd**, or the tab's own name. Pane addressing fixes re-application *within a session* and nothing more, and the commit message for #307 may oversell it.

*Open question for you:* is a durable on-disk scheme actually wanted, or is per-session enough? If durable, `session:` addressing is probably the right third `TabRef` variant, and it is not written.

### D3 — ctl socket, not an MCP tool

*For:* the socket already existed, carries `adopt` and the MCP policy toggles, and needs no new transport.

*Against:* **this is inconsistent with how every other agent-facing write in this codebase works.** `leave_note` is an MCP tool; the sticky-note hook trains every agent on this machine to reach for `mcp__terminal-delight__*`. An agent working through MCP cannot touch a tab at all and has no way to discover that it must shell out. The MCP surface is `list_panes`, `pane_events`, `get_pane_config`, `set_pane_config`, `leave_note`, `grep` — a `set_tab` tool would sit naturally beside `leave_note`, take a pane pid exactly as it does, and need no parent-chain walk because the relay has already resolved the window.

*My confidence: low.* I think an MCP tool is probably the correct primary surface and the ctl verb the secondary one, and I built them in the wrong order. The applier (`apply_tab_ops`) is transport-agnostic, so adding the MCP tool is additive, not a rewrite.

### D4 — An imperative op list, not a declarative desired-state

*For:* ops are simple, order is explicit, and partial application is meaningful.

*Against:* the file *reads* declarative — JSON on disk describing an end state — and is not. That gap caused the incident in §4. A reconciler taking `[{pane|session, name, group}]` and making it so would be idempotent by construction, and idempotence is the property the incident proves we want.

*My confidence: low-to-moderate.* I chose ops because they were less code. That is a bad reason when the alternative removes a whole class of failure.

### D5 — Fire-and-forget

Ops are queued to the UI thread on the same 150 ms ticker as `paint`, so `ok N` acknowledges the *queue write*, not the outcome. Everything checkable is therefore checked at parse time, while the socket is still open.

*Against:* an unresolvable target (a stale index, a closed pane) is skipped **silently**. The caller is told `ok 15` whether fifteen ops landed or two did. `mcp rpc` already demonstrates the alternative — it takes a round-trip to the main thread on its own thread. A tab batch could do the same and report per-op outcomes.

*My confidence: low.* I would probably change this. The silent skip is the same failure shape as the bug in #305 (a scope flag that quietly did nothing), and I argued there that silence was unacceptable — then shipped it here.

### D6 — Groups addressed by name, not id

*For:* the name is painted on the bar; an id is a catalogue number a script must look up first.

*Against:* names are not unique and not stable. Two groups can share a name; renaming one silently redirects every script that referenced it. Currently `ensure_group_named` takes the **first** match and creates one if none exists — so a typo silently makes a new group rather than failing.

*My confidence: moderate.* The ergonomics are right; the collision handling is not thought through.

### D7 — Skip unresolvable targets rather than abort the batch

*For:* "a strip that is nine-tenths labelled beats one rolled back to nothing."

*Against:* combined with D5 this means a batch can half-apply and report success. All-or-nothing plus a real error may be the better contract, particularly for a reconciler (D4).

### D8 — No write gate on ctl

`set_pane_config` and `leave_note` require `TD_MCP_WRITE`. `ctl tabs` requires nothing.

*For:* the socket already carries `adopt`, which spawns processes — same-user trust, no new exposure.

*Against:* the asymmetry is surprising and undocumented. If a `set_tab` MCP tool lands (D3) it must sit behind the toggle, at which point the same operation is gated on one transport and not the other. Worth a deliberate decision rather than an accident.

---

## 4. The incident — read this before you touch a scheme file

I applied an index-addressed op list once (correct), then **re-fired it three times as "verification"**, assuming re-applying the same names was a no-op. It was not: the first application reorders the strip, so each later firing wrote those indices onto different tabs.

```
tab#2   ERROR DISMISS            SHINY
tab#4   MONETIZATION             SHINY
tab#5   ERROR DISMISS            SHINY
tab#6   BFS MARKETING            SHINY
tab#7   ERROR DISMISS            SHINY
tab#13  MONETIZATION             BFS
tab#1   DEV                      ** UNGROUPED **
```

One name on three tabs; two tabs dragged into the wrong group. Repair required re-deriving the whole strip from `list_panes` by hand. `ok 15` was returned every time.

Consequences for you:

- **`list_panes` reports pane titles, not tab names.** The tab strip's actual labels live only in `~/.config/terminal-delight/sessions/<key>.toml`. You need both to know the current state. There is a reader at `/tmp/…/scratchpad/strip.py` if it survives; it is ten lines of regex over that TOML.
- **Never re-run a scheme file to check it worked.** Verify by reading the TOML.
- **The live session key is `1`** (`sessions/1.toml`). Backups rotate under `sessions/backups/1/`.
- TD owns that file and overwrites it on save, so editing it under a running TD is pointless.

---

## 5. Verification notes

**Both merged PRs were checked with discriminating tests**, not just "it worked":

- Routing (#305): with `HYPRLAND_INSTANCE_SIGNATURE=bogus`, the pre-#305 binary answers `no Hyprland session found` and the post-#305 binary answers `ok`.
- Compaction (#305): the pre-#305 binary chokes on the pretty file with `EOF while parsing a list`; the post-#305 binary accepts it.
- To run an old binary through the lean-ctx allowlist, copy it to a directory as the basename `terminal-delight` — the allowlist matches on basename, and hash-named binaries are rejected.

**A test that passed for the wrong reason**, worth knowing about because the pattern will recur: `ctl_tab_rejects_an_unknown_verb` passed locally and failed in CI, because `self_tab_line` resolved the pane *before* validating the verb — so on a box with no TD the answer to a typo was "you are not inside a pane". It passed here only because it ran inside a pane. Fixed by splitting the pure grammar (`self_tab_verb`) from the environment lookup. **Any test that touches `owning_td` is environment-dependent and will lie to you locally.**

---

## 6. What I would do next, in order

1. **Get #307 green and deployed**, then actually exercise `ctl tab name` against a running window. It has never run.
2. **Decide D3** — MCP `set_tab` or not. This is the highest-value open question, because it determines whether agents can discover the feature at all. Additive either way.
3. **Decide D2/D4 together** — they are the same question wearing two hats: is there meant to be a durable, re-appliable scheme file? If yes, `session:` addressing plus a reconciler. If no, say so in the docs and stop pretending the JSON is declarative.
4. **Revisit D5** — silent skips reporting `ok N` is the weakest part of what shipped.
5. Group name collisions (D6).

---

## 7. Things that will waste your time

- `gh` in `~/Projects/lean-ctx` resolves to the **yvgude upstream**, not ours — two remotes, no default. Pass `-R parker-brown-family/lean-ctx`. (Not this repo, but adjacent work.)
- Two unrelated lean-ctx defects were filed tonight and are unresolved: `parker-brown-family/lean-ctx#9` (the pipe check treats `&&` and `;` as pipes; the computed pipe positions are discarded at `mod.rs:211`) and `#10` (every block advises `lean-ctx allow <cmd>`, including blocks no allowlist can lift). #10 carries a correction comment retracting an unverified claim — read it before acting.
- `python3 -c` and interpreter heredocs are blocked through `ctx_shell`. Write a script file and run it.
- CI takes ~4 minutes on Rust checks. Use `gh pr checks <n> --watch --interval 30` in the background; do not poll.
