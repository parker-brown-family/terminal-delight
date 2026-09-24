# Research round 1 — "gui-in-the-tui-prior-art"

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

### A · Terminals that are also a workspace
- **Wave Terminal — terminal, web and preview blocks** `(waveterm)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Warp — blocks, closed source** `(warp)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Zed's terminal — gpui + alacritty_terminal** `(zed-terminal)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Extraterm — frames and rich output** `(extraterm)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **DomTerm — HTML in the terminal** `(domterm)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Hyper — Electron terminal** `(hyper)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources

### B · Terminals that draw pixels for programs
- **Kitty — graphics protocol, kittens, icat, overlays** `(kitty)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **WezTerm** `(wezterm)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Ghostty** `(ghostty)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Rio** `(rio)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Contour** `(contour)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **foot** `(foot)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **iTerm2 — OSC 1337 inline images** `(iterm2)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources

### C · Multiplexers and TUI-as-window-manager
- **dmux — agent panes on tmux (named by Parker)** `(dmux)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **tmux — display-popup, image passthrough** `(tmux)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Zellij — floating panes, plugins** `(zellij)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Arcan + cat9 + durden** `(arcan-cat9-durden)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources

### D · Programs that put pixels in a terminal
- **ueberzug++** `(ueberzugpp)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **awrit — Chromium in Kitty** `(awrit)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Carbonyl — Chromium in a terminal** `(carbonyl)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **browsh** `(browsh)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **presenterm** `(presenterm)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **yazi — file manager with previews** `(yazi)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **euporie — Jupyter in the terminal** `(euporie)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **notcurses** `(notcurses)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **mpv — vo=kitty / vo=sixel / vo=tct** `(mpv-terminal-vo)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **ratatui-image** `(ratatui-image)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **chafa** `(chafa)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **timg** `(timg)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **rtv (Laotian-telephonebell2559) — claimed FFmpeg-in-terminal, metadata only** `(rtv)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources

### E · The two posts Parker shared
- **'GUI in the TUI' / HYPERION TUI v2.7.19 (Nix, @NixFred)** `(hyperion-tui)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources
- **Boyd's Jev-plays-Pokemon dashboard (@0xBOYD)** `(boyd-jev-pokemon)` → what_it_is, license, language, graphics_mechanism, where_it_draws, input_model, activity, td_overlap, steal_for_td, trap, primary_sources

## Return shape (JSON `GatherResult`)
```json
{
  "updates": [
    {
      "id": "waveterm",
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