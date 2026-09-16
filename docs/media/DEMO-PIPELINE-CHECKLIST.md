# Demo pipeline — the shot list, one agent prompt per clip

Terminal Delight's problem is not that it lacks features. It is that a marketing
clip used to cost an afternoon, so none were ever made. This document makes a clip
cost about ninety seconds: pick a row, paste its prompt into a fresh agent, get back
a file you can drag into a post.

**How to use it.** Work top to bottom — the rows are ordered by how well they land
on someone who has never heard of TD, not by how much work they took to build. Each
block is self-contained: hand its **Agent prompt** to a fresh agent in
`~/Work/terminal-delight` and it has everything it needs. Tick the box when the clip
is captured, and again when it has been posted.

**The format, for every clip.** One gesture. Four to eight seconds. No narration, no
title card, no cursor, no "we're excited to announce". The terminal is the argument.
Post the clip with one line of text and nothing else — never a thread.

**The tool.** `scripts/td-clip.sh` (see [its header](../../scripts/td-clip.sh) for
flags). Output lands in `docs/media/clips/`, which is gitignored.

---

## Phase 0 — one-time setup

- [x] **A real recorder is already here.** `gpu-screen-recorder` is installed and is
      what `td-clip.sh` uses: measured at a true 60 fps over a pane-sized region,
      GPU-encoded, and it writes physical pixels — a 744x410 logical region comes
      out as a 1190x656 file on this 1.6x display, which is sharper than the region
      you asked for. Nothing to install. `wf-recorder` and a ~1–3 fps `grim` loop
      remain as fallbacks in that order.

- [ ] **Build the current binary**, so clips show today's product, not the installed
      one from three weeks ago. The `td-redeploy` skill does this.

- [ ] **Stage a clean demo workspace.** Fictional agents only — no real prompts,
      paths, client names or business ever appear in public media. The demo gates
      are `TD_DEMO`, `TD_WALL_DEMO`, `TD_DEMO_LOGOS`; see
      [ENV-FLAGS](../features/ENV-FLAGS.md).

- [ ] **Silence the desktop.** Notifications, the clock second-hand, anything that
      moves and is not TD. A stray Slack toast ruins an otherwise finished clip and
      you will not notice until it is posted.

- [ ] **Do not touch the terminal while it records.** The capture photographs the
      screen, not the pane — switching TD tabs mid-record silently swaps a
      different pane into the clip. Measured the hard way on 2026-09-07.

- [ ] **Pick one theme and stay on it** for a whole batch. A feed where every clip
      is a different colour reads as a theme demo; a feed with one consistent look
      reads as a product.

---

## The full list lives in the shot board

This file is the working checklist for the first eight shots. The **complete**
inventory — forty candidate clips mined from all twelve capability panels, each
scored by tier and by who has to drive it, each with a ready-to-run agent prompt —
is `shot-list.html` beside this file, published as an artifact you can tick through
in a browser.

**In the can as of 2026-09-07, all three socket-driven with nobody in the room:**
`sticky-note-agent` (a note landing and taking a pin), `agent-wall` (cards arriving
as agents are adopted), `mcp-policy` (the read-only stance and the write gate
flipping live). All at 1504×1408, 60 fps, in `clips/`.

## Who has to be at the keyboard (measured 2026-09-07)

An agent can drive the ctl socket, and that is all. It cannot click, and on this
box it could not take keyboard focus either — `hl.dsp.focus({window=…})` returns
`ok` and the focus does not move, so `wtype` had nothing to type into. Hyprland's
Lua dispatch offers `cursor.move` but no button press, and there is no `ydotool`.
So the shots divide cleanly:

| Shot | Driver | Status |
|---|---|---|
| 2 · sticky note (agent half) | `leave_note` over ctl | **hands-free**, shot |
| 8 · scriptable tabs | `ctl tabs` | hands-free in principle — see the blocker below |
| 6 · per-pane appearance | `set_pane_config` over ctl | hands-free in principle |
| 1 · copy chips | Alt-hold **+ click** | needs a human |
| 2 · sticky note (`alt+s` half) | keypress | needs a human |
| 3 · agent wall | `ctrl+shift+A`, then clicks | needs a human |
| 4 · FOCUS reader | `alt+R`, then typing | needs a human |
| 5 · theme tray | `ctrl+shift+D`, then clicks | needs a human |
| 7 · graveyard | click to resurrect | needs a human |

**The blocker under the two "in principle" rows.** A throwaway instance
(`TD_SESSION=<name>`, which stops it restoring your real panes — without it, a second
terminal-delight comes up wearing your actual work) opens on the MCP CONTROL panel,
and nothing socket-driven dismisses it: the tab strip and the panes are behind a
screen only a keypress or a click can close. Adding `ctl tab select <n>` and a verb
to close the front panel would make shots 6 and 8 fully hands-free, and would let an
agent bring the right tab forward before recording — which is also the fix for the
tab-switch hazard below.

Until then, the honest shape of this pipeline is: **an agent stages, frames, records
and checks; a human performs the gesture.** That was always the design; the ctl
socket just moves a couple of shots across the line.

## The shot list

### 1. Alt-hold copy chips — `copy-chips`

- [ ] captured  · [ ] posted

**Why it leads.** Everyone has copied a wrapped command out of a terminal and got
back a broken one. This shows the fix happening, and needs no explanation at all.
It is the single strongest four seconds TD has.

**What the viewer sees.** A scrollback with a long command wrapped over three lines.
Alt goes down; a frame appears around the whole logical line — not the painted rows.
One click. Then the paste, in a fresh prompt, whole and correct on one line.

**Post text:** `Copying a wrapped command out of a terminal, without the wrap.`

**Agent prompt:**

```
Capture the Alt-held copy-chip clip for Terminal Delight. Read docs/features/07-terminal-core.md for the exact semantics of the affordance (it is strict: commands only, and the text copied is the reconstructed logical line with wrap seams healed). Stage a TD pane, narrow enough that a long realistic-but-fictional command wraps across three lines, run it so it sits in the scrollback, then record with scripts/td-clip.sh copy-chips -d 8 --gif: hold Alt so the frame appears, click the line, then paste into a fresh prompt so the viewer sees the healed line arrive whole. No cursor in frame beyond the click, no real paths or client names. Report the output path and tell me whether the frame is legible at 800px wide, because it is going on X.
```

### 2. Sticky notes — `sticky-note`

- [ ] captured · [ ] posted

**Why it is second.** Nothing else does this, so it cannot be read as a feature
another terminal has. And it is the clearest one-shot statement of what TD is *for*:
a wall of agents you come back to.

**What the viewer sees.** A pane. `alt+s`. Paper lands on the glass, tilted, curved
with the CRT, and takes handwriting. Right-click drives a pin through it — and the
pin appears on the tab up in the mother bar.

**Post text:** `My terminal has a fridge door.`

**Already scripted.** The agent-writes-a-note half needs no hands at all —
`scripts/stage-sticky-clip.sh <pane-pid> "<X,Y WxH>" <workspace>` peels the glass
clean, starts the recorder, then posts and pins a note over the ctl socket on cue.
The human `alt+s` half still has to be performed.

**Agent prompt:**

```
Capture the sticky-note clip for Terminal Delight. Read docs/features/10-sticky-notes.md first — the note is pre-warped so it sits flat on bent glass, it is written in Caveat, and right-clicking it drives a pin that surfaces on the pane's TAB. Record with scripts/td-clip.sh sticky-note -d 8 --gif, framing wide enough that the tab strip is in shot, because the pin appearing on the tab is the payoff. The gesture: alt+s, write a short fictional note (ten words or fewer, in the house style — shouty title, no real work), Enter, then right-click to pin. Then take a second clip, sticky-note-agent, showing an agent writing one through the MCP leave_note tool while the human watches. Report both paths.
```

### 3. The agent wall — `agent-wall`

- [ ] captured · [ ] posted

**Why.** This is the actual product thesis, and the hardest to make legible in four
seconds. Do not lead with it — post it third, once the account has people who
already know what TD is.

**What the viewer sees.** A wall of cards. Robots working the tools their agents are
actually holding. One card's vitals go red and it sorts itself to the front.

**Post text:** `Twelve agents. One of them needs you. You can see which.`

**Agent prompt:**

```
Capture the agent-wall clip for Terminal Delight. Read docs/features/02-agent-dashboard.md and docs/features/11-usage-vitals-keepalive.md. Stage the demo wall (TD_WALL_DEMO / TD_DEMO_LOGOS — all data fictional, that is a hard rule) with enough cards that it reads as a fleet. Record with scripts/td-clip.sh agent-wall -d 8 --gif. The clip must show two things and only two: the robots animating the tool each agent is holding, and one card crossing into needs-you so the bars shout and it sorts to the front. If staging a live state change is impractical, say so rather than faking a number — we can script it instead.
```

### 4. FOCUS reader — `focus-reader`

- [ ] captured · [ ] posted

**What the viewer sees.** `Alt+R`. One pane zooms up to full screen; everything
behind it frosts with real GPU blur. Typing still goes through to the agent
underneath — show that, it is the part people do not expect.

**Post text:** `Read the agent without losing the agent.`

**Agent prompt:**

```
Capture the FOCUS reader clip for Terminal Delight. Read docs/features/05-visuals-crt.md for the blur pipeline. Record with scripts/td-clip.sh focus-reader -d 8 --gif: a pane with real-looking (fictional) agent output, Alt+R to raise the reader, a beat on the frosted background, then TYPE while the reader is open so the viewer sees the keystrokes reach the agent underneath — that is the point of the clip, not the blur. Esc to close. Report the path and flag it if the blur bands or looks cheap at 800px, which would mean we shoot it at a different scale.
```

### 5. The theme tray — `theme-tray`

- [ ] captured · [ ] posted

**Why.** Pure eye candy. It teaches nothing and it will outperform everything else
in the list, which is fine — it is the filler between substantive clips, not the
substance.

**What the viewer sees.** The tray opens as a spectrum. Sets get picked; the whole
terminal changes underneath. Then INVERT flips the lot photo-negative.

**Post text:** `Every pane wears its own.`

**Agent prompt:**

```
Capture the theme-tray clip for Terminal Delight. Read docs/features/06-themes.md — the tray is ordered ROYGBIV, and INVERT sits above the picker because it photo-negates whatever set you pick as the last colour op. Record with scripts/td-clip.sh theme-tray -d 8 --gif: open the tray, walk two or three sets so the pane changes underneath, then hit INVERT last as the button. Keep it to one pane so the change is legible. Report the path.
```

### 6. Per-pane themes on a split — `per-pane`

- [ ] captured · [ ] posted

**What the viewer sees.** Four panes, four different looks, one window — then one
pane alone gets its curvature dialled up while the others sit flat.

**Post text:** `Per pane. Not per app.`

**Agent prompt:**

```
Capture the per-pane theming clip for Terminal Delight. Read docs/features/06-themes.md (theme and grade groups inherit from the outer INDEPENDENTLY) and docs/features/05-visuals-crt.md (warp is a per-pane grade channel). Record with scripts/td-clip.sh per-pane -d 8 --gif: a four-pane split where each pane already wears a different colour set, then dial ONE pane's warp up on the DISPLAY tray so it bends alone. Report the path.
```

### 7. The graveyard — `graveyard`

- [ ] captured · [ ] posted

**What the viewer sees.** A list of dead agent sessions found on disk. One click.
It comes back, mid-conversation.

**Post text:** `The agent you closed yesterday is still on disk.`

**Agent prompt:**

```
Capture the graveyard clip for Terminal Delight. Read docs/features/03-agent-graveyard.md. Record with scripts/td-clip.sh graveyard -d 8 --gif: open the graveyard so the found sessions list is visible, then resurrect one and let the clip run long enough that the restored transcript is on screen and readable. All sessions in frame must be fictional or scrubbed — check before recording, not after. Report the path.
```

### 8. Scriptable tabs — `ctl-tabs`

- [ ] captured · [ ] posted

**Why last.** It films badly (a command, then a change) but it is the clip that
makes another developer think *I could build on this*. Worth having; not worth
leading with.

**Post text:** `The terminal is running. You can still script it.`

**Agent prompt:**

```
Capture the ctl scripting clip for Terminal Delight. Read docs/features/12-ctl-scripting.md. Record with scripts/td-clip.sh ctl-tabs -d 8 --gif with the tab strip in frame: run a terminal-delight ctl tab command in one pane and let the viewer watch the tab strip relabel itself live. Prefer the zero-argument self-naming form if it reads more clearly on screen. Report the path.
```

---

## After a batch

- [ ] Watch every clip once at 800px wide with the sound off — that is how it will
      actually be seen. Anything illegible gets reshot, not posted smaller.
- [ ] Confirm no real path, prompt, client name or token count is in any frame.
- [ ] Three posts a week, one clip each, spaced. A batch dumped in one day buys one
      day of attention and then silence.
- [ ] Anything that filmed badly is a note back into
      [`docs/features/`](../features/README.md) — if a feature cannot be shown in
      four seconds, that is a fact about the feature worth writing down.
