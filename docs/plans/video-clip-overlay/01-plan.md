# A video opens in its own box over the page, and closes when it ends

**Difficulty 5/10**, scored before any code. The change stays inside the document view and the pane's floating square, and if it is wrong it shows on screen at once. What could hide is a player or a texture that never stops, and the existing video bench check already measures that.

**Approved with one amendment, 2026-10-13.** The first draft played the video *inside* the page, drawn over its box. Parker turned that down: *"we need to make sure we don't LOSE the current open overlay html, so the video SHOULD OPEN IN IT'S OWN video overlay glowbox, then auto close when finished. This should also be the behaviour when a VIDEO render complete and the agent says: 'hey look at this mp4' and plays it in the pane... it will be an AUTO-CLOSING overlay."*

## Why it doesn't play today

A page in the square is a **photograph**. Headless Chromium lays the page out once, TD takes pictures of it in tiles and scrolls those pictures itself. So the `<video>` on the welcome page is Chromium's picture of a player: the poster, the ▶, the `0:00 / 3:56`. Pressing ▶ presses a photograph. A video opened on its own plays because it goes to libmpv instead.

## What changes

**Every video that opens in the square opens in its own glowing box, above whatever square is already up, and closes itself when it ends.** The square underneath is not closed, re-read or scrolled. When the video box goes, the page is exactly where it was.

```
┌ the page's square (kept, dimmed, takes no presses) ──────┐
│ ☰ welcome.html                       ⇲ split  ↗  ✕ esc   │
│                                                          │
│    ┌ the clip's own box (hyperglow, its own strip) ───┐  │
│    │ ☰ welcome-pass-one.mp4          ⇲  ↗  ✕ esc      │  │
│    │                                                  │  │
│    │            libmpv frames                         │  │
│    │                                                  │  │
│    │ ❚❚  0:12 / 3:56  ──────●─────────────  sound     │  │
│    └──────────────────────────────────────────────────┘  │
│                                                          │
└──────────────────────────────────────────────────────────┘
        plays once → at its end the box closes → the page again
```

Three roads lead to this box, and all three behave the same:

1. **A press on a `<video>` in a page** in the square. The probe that already finds links in every page (`docview/extract.js`) also reports each video's box and the file it plays. A press there opens that file in the clip box. A link to a video file (`<a href="clip.mp4">`) does the same: it no longer replaces the page.
2. **An agent's `open_document` on a video**, for example after a render: *"hey, look at this mp4."* The clip opens over the agent's pane, plays once and goes.
3. **Alt+click on a video path** in the terminal.

Escape or ✕ closes the clip early and returns to what was under it. A second Escape closes the page, as today. ⇲ split still moves the clip into a pane of its own, where it **loops**, as a video on the Document face does today. Only the overlay closes itself.

## How

1. **mpv plays a clip once.** The square used to loop every video. A clip now starts with looping off and reports the moment it reaches its end. A clip moved onto the Document face turns looping back on.
2. **The view says "finished"**, and the pane, which owns the square, closes the clip and puts back the square it covered.
3. **The square becomes a stack of at most two.** The clip holds the square it covers. Every control, key and press keeps going to the top box, so nothing that already works has to learn about the stack. Closing the top puts the one beneath back on top. Closing the pane drops both. A replica repair carries both across.
4. **The covered square is drawn but inert**: dimmed, and a press on it does nothing rather than falling through to the terminal.
5. **The probe reports videos**, and its version number goes up, so a page cached before this change is probed again instead of being read as "no videos".

## Decisions, as amended

| | Decision | Why |
|---|---|---|
| **Where it plays** | **Its own box above the square** (Parker) | The page underneath is never lost. |
| **When it closes** | **At its end, or on Escape / ✕** | An overlay that ends by itself needs no hand to put it away after a render. |
| **Autoplay** | **Never on its own**: a page's video plays when pressed | Five looping clips on a page would be five players at about 0.6 of a core each. |
| **What it can play** | **Local files.** A web address opens in the browser, like any web link on a page | It stays the same road every other link already takes. Handing URLs to mpv can follow later. |
| **A page on the Document face** | **Not in this change** | A video pressed on a page in a split pane still replaces it there. The square is what was asked for. Filed as a follow-up. |
| **A video inside a brief's dialog** | **Not in this change** | A dialog is drawn as a separate picture. |

## Proof

- **Unit tests:** the probe's JSON parses, absent fields included. A press on a video's box finds the video, and a press on a dialog's video finds nothing. The clip box sits inside the square it covers, with the page's strip still showing. mpv's end-of-file reasons are read the right way round: a clip that ends closes, while one stopped or failed does not. Closing a clip puts back the square beneath, and closing the pane drops both.
- **Bench, in a hidden window:** `scripts/video-check.sh` gains a clip case. It opens a page, opens a short clip over it, checks the threads are alive, then waits for the clip to end. It then checks the clip's threads are gone and the page square is still up.
- **Then your welcome page**, by eye.

## Slices, one pull request

1. **Tracer:** mpv once and finished, the stack, and the clip opening over a square from Alt+click and `open_document`.
2. **The page road:** probe → layout → a press on a video opens the clip.
3. **The bench case**, then the PR.
