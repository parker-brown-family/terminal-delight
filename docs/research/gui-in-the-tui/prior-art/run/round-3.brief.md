# Research round 3 — "gui-in-the-tui-prior-art"

Subjects below are NOT yet complete. Fill **only** their listed gap fields.

## Definition of Done (per field)
- **what_it_is** (required) — what_it_is [source-required] — one or two sentences: what the project is and what graphical content it puts in or beside a terminal
- **license** (required) — license [source-required] — SPDX id from the repo (gh api repos/O/R --jq .license.spdx_id) or 'proprietary' with the vendor page as source
- **language** (required) — language [source-required] — primary implementation language(s), from the repo's language breakdown or docs
- **graphics_mechanism** (required) — graphics_mechanism [source-required] — HOW pixels or rich content get there: native GPU drawing, Kitty graphics protocol, Sixel, iTerm2 OSC 1337, Unicode half/quadrant/sextant blocks, separate overlay OS window, embedded webview/browser engine, DOM (Electron/xterm.js) … cite the doc or source file
- **where_it_draws** (required) — where_it_draws [source-or-flag] — split pane | floating panel | inline in scrollback | fullscreen | separate OS window | cell-anchored overlay — say which, and whether it scrolls with text
- **input_model** (preferred) — input_model [source-or-flag] — how mouse and keys reach the embedded thing (focus model, mouse passthrough, keyboard capture, escape key)
- **activity** (preferred) — activity [source-required] — stars, last push / last release date — measured with `gh api repos/O/R` (source: 'measured 2026-09-24: gh api repos/O/R') or the vendor's release page
- **td_overlap** (required) — td_overlap — comma-separated tags from exactly this vocabulary: open-on-click, pane-content, floating, host-protocol, tui-wm, agent-panes, same-stack — or 'none'. Drives the derived closeness score.
- **steal_for_td** (required) — steal_for_td — THE one idea Terminal Delight should take from it, concrete enough to act on (inferred — say so in the note)
- **trap** (preferred) — trap — what TD should avoid, learned from this project's design or its issue tracker
- **primary_sources** (required) — primary_sources [source-required] — space-separated repo/docs URLs actually read; mark blog posts or AI summaries as 'synthesis:'

## Integrity rules
- Never invent a value (phone, name, figure). If unfound, omit it or set `flagged: true` with a note like "verify by phone".
- Every value must carry `provenance.source` (a URL) OR be `flagged`. Unsourced, unflagged values are rejected by the harness.
- Only fill the gap fields listed for each subject — do not re-research satisfied fields.
- Emit a `bubbles` entry of kind "serendipity" for anything useful you find that is outside the requested fields.

## Gaps to fill

### E · The two posts Parker shared
- **'GUI in the TUI' / HYPERION TUI v2.7.19 (Nix, @NixFred)** `(hyperion-tui)` → license, language, graphics_mechanism, activity

## Return shape (JSON `GatherResult`)
```json
{
  "updates": [
    {
      "id": "hyperion-tui",
      "fields": {
        "<field>": {
          "value": "<value>",
          "provenance": {
            "source": "https://…",
            "confidence": "high"
          }
        },
        "<unsourced-field>": {
          "value": "<value>",
          "flagged": true,
          "note": "verify by phone"
        }
      }
    }
  ],
  "bubbles": [
    {
      "kind": "serendipity",
      "subjectId": "<id>",
      "note": "…",
      "why": "…",
      "source": "https://…"
    }
  ],
  "tokens": 0
}
```