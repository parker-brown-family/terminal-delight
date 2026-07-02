# Anchor-Top Inverted Read — Target Behaviour Specification

> Canonical definition of what the ⚓ anchor-top read (and its scrolling) MUST do,
> for every client and screen mode. This exists because the behaviour has drifted
> and broken *differently per client* (Claude vs Codex vs shell vs vim) across
> #126–#156. Any change to the inverted read or wheel handling is measured against
> this document. **A fix that helps one client MUST NOT regress another** (PR #142
> was closed for exactly that — it would have regressed Codex).

Code: `app/src/pane.rs` — `should_invert`, `invert_logical_read`, `bottom_anchor_rows`,
the render inversion (~line 4206), `scroll_by_wheel`, `cell_at`/`link_under`,
`paint_row_to_grid_row`. Toggle: the ⚓ control / `TD_ANCHOR_TOP=1`.
Diagnostic: `TD_ANCHORDEBUG=1` (see §7).

---

## 1. The mental model

Anchor-top is a **reading-order inversion**, like a chat pane pinned to the top:

- **OFF (default):** native terminal — the live prompt/input is at the BOTTOM,
  output scrolls up into history. Byte-identical to a normal terminal.
- **ON:** the live prompt/input sits at the **TOP**; the **newest** output is
  directly beneath it; **older** output flows **DOWNWARD**; blank padding sits at
  the bottom. You read top-to-bottom = newest-to-oldest.

The single promise a user relies on: **"newest is at the top; to go back through
history I scroll DOWN."**

---

## 2. The four coupled axes — they MUST stay consistent

Every inverted pane has four things that have to agree. Most "wonky per client"
bugs are two of these disagreeing.

1. **Render row order** — which grid row is painted where (the block/line reversal).
2. **Within-unit order** — the rows of a *single* logical line or message stay in
   natural top-to-bottom reading order. A unit is reversed as a whole; its insides
   are never flipped.
3. **Scroll direction** — the gesture that reveals **older** content.
4. **Hit-test & selection** — a click/drag maps to the cell actually under the
   cursor; copy yields visual reading order.

> **THE INVARIANT:** if the display shows older *down*, then the *down* gesture
> reveals older — **in every scroll model** — clicks map to the visually-correct
> cell, and selection follows visual order. A pane whose DISPLAY is inverted but
> whose SCROLL is not feels backwards. **This was the Claude bug (§6).**

---

## 3. The unit of inversion — per client

| Client | Unit reversed | Why |
|---|---|---|
| **Shells** (normal screen) | soft-wrapped **logical line** (WRAPLINE groups) | a wrapped line's continuation rows must stay in order (#128) |
| **Agents** (Claude / Codex) | **message block** (blank-line-delimited), kept upright | agents draw multi-row messages/boxes by cursor positioning, no WRAPLINE; line-level reverse flipped each box bottom-to-top (#129) |
| **Non-agent full-screen TUIs** (vim/htop/less on alt-screen) | **nothing — not inverted** | their box drawing assumes a fixed top-to-bottom layout (#127) |

The input/prompt box always ends up on TOP and **grows DOWN as you type**.

### 3a. Block boundary caveat (root of "paragraphs out of order")

Block-mode splits on **every run of blank lines** and reverses block order
(`invert_logical_read`, `block_mode`). Correct ONLY if the client uses a blank line
to mean "message boundary." If a client prints blank lines *inside* a single
logical message (between paragraphs, around tool results), the detector over-splits
that one message into several blocks and reverses them → **paragraphs print out of
order.** The boundary must match the client's real message delimiter (turn marker,
or ≥2 consecutive blanks), not "any blank line." Confirm with §7's `groups`/`sizes`
before changing it.

---

## 4. `should_invert` — the gate

```
inverted = anchor_top && !crawl && (!alt_screen || agent_mode)
```

- crawl mode owns the layout → never also invert.
- non-agent alt-screen TUI → **not** inverted (upright box drawing).
- agent (Claude/Codex), even on the alternate screen → inverted, block-mode.

**Dependency:** `agent_mode = mode.is_agent()`, from `foreground_mode` →
`classify(comm, cmdline)`, which matches only when the **foreground process group**
is `claude`/`codex` (comm) or has `/claude`//`/codex` in cmdline. **Failure mode:**
while an agent runs a tool, the foreground pgid is a child (bash/node/rg), so the
pane momentarily classifies as **Shell** → `agent_mode=false` → on the alt-screen
`should_invert` flips to **false** mid-turn (flicker). Agent detection SHOULD be
sticky (track the agent by the pane's process tree / session, not the instantaneous
foreground pgid). Confirm with §7 whether `agent`/`inverted` flicker on a Claude
pane.

---

## 5. Scroll direction — the rule, per scroll model

There are **two** scroll models and the inverted read must be honoured in **both**.
`scroll_by_wheel` dispatches three legs:

- **Leg 1 — MOUSE_MODE** (app has mouse reporting): forward wheel **button events**.
- **Leg 2 — ALT_SCREEN + ALTERNATE_SCROLL** (Claude Code, less, man, vim): the alt
  screen has no scrollback, so forward the wheel as **arrow keys** (`ESC[A`/`ESC[B`,
  or `ESC O A/B` under APP_CURSOR). (This is Model B — app-owned scroll; #156.)
- **Leg 3 — normal-screen shell**: move **our** scrollback (`scroll_display`).
  (Model A — local scrollback.)

**RULE:** compute the gesture's *intent* once — "reveal older?" — as
`reveal_older = (scroll_up) XOR paint_inverted`, and every leg honours it:
Leg 3 already did (`if paint_inverted { -lines }`); Legs 1 & 2 must flip the
forwarded button/arrow the same way. On an inverted pane, a physical **DOWN**
gesture must send the app the input that scrolls it toward **older** content.

> **History:** #156 (`3374fda`) landed Legs 1–2 but only Leg 3 was inversion-aware —
> so alt-screen agents (Claude) scrolled backwards. Fixed by flipping the gesture
> sense once, before the leg dispatch, so all three legs agree. **Do not** re-add a
> per-leg flip on top of that (double-inversion).

---

## 6. The Claude bug — CONFIRMED root cause + fix

**Symptom:** anchor-top ON; Codex scroll DOWN walks back through messages (correct);
Claude Code needs scroll UP; paragraphs printed out of order.

**Root cause (scroll):** Codex prints to the **normal screen** → Leg 3, which flips
for `paint_inverted` → down = older (works). Claude Code runs on the **alternate
screen** → Leg 2 (arrow keys), which did **not** consult `paint_inverted` → a
physical scroll-DOWN sent `ESC[B` (Claude scrolls toward newer) → the user had to
scroll UP for older. **Fixed** (this change): flip the gesture sense for inverted
panes before the leg dispatch (`up ^= paint_inverted`), so Leg 2 (and Leg 1) send
the older-direction input on a physical DOWN. Safe for Codex: a *working* Codex is
on Leg 3, which is untouched.

**Root cause (out-of-order paragraphs):** likely §3a — Claude's blank-line
structure over-splits one message into several blocks that get reversed. This is a
**separate** fix from the scroll direction; confirm the `groups`/`sizes` in §7's
`invert_logical_read` line before changing the boundary rule.

"Did they change something or did we?" — **us.** The inverted read (#126→#149) and
the wheel legs (#156) are ours; Leg 2 was simply never made inversion-aware.

---

## 7. Diagnostic — `TD_ANCHORDEBUG=1`

Emits to stderr, per pane:

- render: `program · agent_mode · alt_screen · anchor_top · crawl · inverted`
- `invert_logical_read`: `block_mode · group_count · group sizes` (over-split? §3a)
- `scroll_by_wheel`: `paint_inverted · reveal_older · mouse_mode · alt_screen ·
  alt_scroll` (which leg fires, and whether the inversion flip applied)

**Use it before touching code.** Run `TD_ANCHORDEBUG=1 TD_ANCHOR_TOP=1`, scroll a
Claude pane and a Codex pane, compare. The deviation names the axis (§2) to fix —
then re-run the §8 checklist for BOTH clients so you don't regress one.

---

## 8. Verification checklist — run for EVERY client after any change

For shell · Codex · Claude · a non-agent TUI (vim), with anchor-top ON:

- [ ] prompt/input sits on TOP; typing grows the box DOWNWARD.
- [ ] the newest message is directly under the input; older is below it.
- [ ] within one message, lines read top-to-bottom (not reversed).
- [ ] **scroll DOWN reveals older; scroll UP returns toward the prompt.**
- [ ] click/drag selects the cell under the cursor; copy = visual order.
- [ ] a non-agent full-screen TUI (vim/htop) renders UPRIGHT, unaffected.
- [ ] **Codex and Claude behave identically.**
- [ ] anchor-top OFF is byte-identical to a normal terminal.

Regression guard: keep `should_invert_truth_table_preserves_codex_alt_screen`
green, and add a case for any client-specific rule you change.
