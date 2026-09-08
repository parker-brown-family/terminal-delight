# Sticky notes — the fridge door on a pane's glass

A handwritten note stuck to a terminal's glass, in the pane's own top-right corner,
readable across a room and readable *first* — before any transcript. It is the
answer to the twenty-pane wall problem: coming back cold, you read the notes
instead of scrolling twenty scrollbacks for your own last prompt.

## Why it matters

Every other surface in TD reports *state* — what an agent is doing, what it costs,
whether it is stuck. A note carries *intent*, which no parser can infer: why this
pane exists, what you asked for, what you want done when you get back. Both halves
of the pair can write one — you with `alt+s`, the agent through MCP — so the note
is also the narrowest possible channel for an agent to say one thing to a human
without any of it being a transcript.

## Features

| Feature | What it does | Evidence | Binding / flag |
|---|---|---|---|
| **Post a note** | `alt+s` sticks paper to the focused pane's top-right and hands it the cursor; pressing again puts the pen down, as does Enter, as does clicking off the paper | `app/src/sticky.rs` module doc; `main.rs` help row `Alt+S` | `alt+s` |
| **Peel it off** | Removes the note; the same paper takes new words if you post again | `sticky.rs` | `alt+backspace` |
| **Pin it** | Right-click the paper drives a **pushpin** through it, and the pin surfaces on the pane's **tab in the mother bar** — so a note can ask for attention from a tab you are not currently on. Only a second right-click takes it out | `sticky::right_click`; `main.rs::tab_pinned_notes` | right-click |
| **Handwriting** | Drawn in Caveat (SIL OFL), bundled and registered at startup beside the crawl font, so a box with no handwriting face installed still renders one | `main.rs` font registration | — |
| **Tilted, and PRE-WARPED** | The note is drawn *through* the pane's barrel-distortion map so the post-pass lands it flat on bent glass — it shares the pane's curvature exactly, with no seam. The earlier cut-a-hole approach left a visible one | `sticky.rs` `Warp` note; [visuals](05-visuals-crt.md) | automatic |
| **Rotation-correct hit-testing** | Glyphs and paths carry the tilt; layout, masks and hit-testing stay in the flat box, and clicks invert the rotation — the same contract the Alt-held copy affordance works under | `sticky::Hit::at` | — |
| **Esc is never destructive** | Esc reverts the *composer* only. Once posted, a note is an object on someone's terminal, and swallowing an Esc that the pane below would read as an interrupt is the bug this feature must not ship | `sticky::press` | — |
| **Survives a restart** | Persisted beside the pane's cwd in the workspace state file, seed and all | `main.rs::SavedNote`; `pane.rs` note field | automatic |
| **Agents can write one** | The MCP `leave_note` tool posts, replaces, pins or clears a pane's note — title, ≤10 words of body, `pin` — and is refused when the writes toggle is off | `mcp.rs::leave_note` (+ tests) | `TD_MCP_WRITE` |
| **Ten-word contract** | The body is capped at ten words, validated before anything is applied. A note that needs a paragraph is a transcript, and belongs in the pane | `mcp.rs` note word cap + `leave_note` validation | — |
| **Visible in `list_panes`** | Each pane's current note comes back with its row, so an orchestrator reads the wall's notes without opening anything | `mcp.rs` list_panes | — |

## The rule that makes it work

**Ten words, a shouty title, pinned only when it needs eyes.** A pin that is always
in means nothing is ever urgent. A note replaces the pane's previous note rather
than accumulating — same paper, new words.

## Status

**Shipped.** The human half and the MCP half both landed (#303 gave agents the
pen; the pin arrived alongside the right-button fix in #270, which found that the
pane had never registered the right mouse button at all — every branch testing for
one was dead code).
