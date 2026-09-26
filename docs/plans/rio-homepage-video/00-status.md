# Rio on the homepage, and the reel — status

**Difficulty: 7 / 10**, scored before starting. No single piece is hard. The
risk is in the coupling: everything that ships publicly says the core is Rio's,
so nothing can deploy before the core-swap pull request merges, and eight
recordings only switch seamlessly if every take keeps the same timeline to a
frame or two. Parker was away and delegated the day ("You've got time, I'm
AFK"), so the four gates collapsed into this one page.

## What was asked (2026-09-25)

1. The docs homepage says why Terminal Delight moved to Rio's core and how,
   and links out to the five-part series on parker.brownfamilysports.com.
2. The series is finished and published.
3. The reference list drops Alacritty and names Rio. References point at the
   source's own homepage, never at our kiosk: lean-ctx, Rio, Omarchy.
4. The homepage's first demo is a screen recording, sterile of personal
   information, of Terminal Delight playing a video of itself in a pane. The
   video inside tours the attention spine and the agent wall on mock data and
   flips between the workbench and the terminal. About thirty seconds.
5. One recording per docs-site theme. Switching the page's theme switches the
   recording, at the second it was playing.

## What was built

| Piece | Where |
|---|---|
| Homepage: the reel as the first demo, and an "Underneath" section | `docsite/pages/index.html` |
| The player: two stacked videos, seek-then-fade on a theme change, a live island for the CRT tube, pause, reduced motion, off-screen pause | `assets/docs.js` (the reel block), `assets/docs.css` |
| Eight takes and posters | `assets/reel/<theme>.mp4`, `.jpg` |
| The build copies the reel and refuses a theme without a take | `docsite/build.mjs` |
| References: lean-ctx → leanctx.com, Omarchy → omarchy.org, Rio → rioterm.com | `docsite/nav.json` |

## How the recordings were made

The filming kit stays out of this repository, by the 2026-09-16 decision that
filming automation does not belong in the open-source tree. It lives at
`~/Work/td-film-kit`, with `setup-demo-home.sh` to rebuild the sterile home
and `encode.sh` for the web files. In outline:

- **A sterile home.** `/tmp/demo` holds three made-up projects (Orbit API,
  Orbit web, field notes) with commits by `Demo <demo@example.com>`, a plain
  prompt, and its own Terminal Delight config. The window runs with `HOME`,
  `USER` and every `XDG_*` directory pointed there, so it reads none of the
  real configuration, transcripts or sessions. `notify-send`, `xdg-open` and
  `uwsm-app` are stubbed on its `PATH`, so a finished mock agent cannot reach
  the real desktop.
- **Mock agents through the real code paths.** A small script named `claude`
  draws an agent-shaped screen, writes a transcript where Terminal Delight
  reads one, hands the workbench a response and a decision card, and makes
  itself the terminal's foreground job. The pane badges, the attention queue,
  the agent wall and the bench all read that mock data with their shipped
  code. One agent stops at a permission prompt, which is what puts a row in
  the queue.
- **A hidden window.** Each take launches onto a `special:` workspace with
  `render_unfocused`, floated at 1200×675 logical, which is 1920×1080 on the
  laptop panel at scale 1.6. Nothing appears on any monitor.
- **One clock.** `film.py` captures the window with `grim -T` at 30 frames a
  second, resamples to a constant 30 fps into ffmpeg, and fires the tour's key
  chords (`hl.dsp.send_shortcut` to the window's pid) at fixed seconds from
  the take's start. The mock agents read the same start time from a file, so
  second 12 of every theme's take is the same moment.
- **The nesting.** The outer take is a second sterile window whose showreel
  pane runs mpv with `--vo=kitty --vo-kitty-use-shm=no`, loaded paused on the
  inner take's first frame and unpaused over mpv's socket at the outer take's
  first second.
- **The look.** A film theme with every random CRT effect at zero (flicker,
  tracking band, jiggle), so takes line up, and a grade brighter than the
  house default, which is tuned dim for a monitor and reads murky on video.

## Measurements

- Window capture: 29.9 fps (ppm), 30.0 fps (jpeg), measured over 3 s.
- Every chord fired within a millisecond of its scheduled second.
- mpv inside the outer pane dropped 285 to 472 of 900 frames depending on
  the take and on what else the machine was doing, so the nested tour plays at
  roughly 14 to 20 fps. Direct transmission base64-encodes about 3 MB a frame.
- The tour runs 0.167 to 0.233 s late inside each outer take (`align.py`,
  matching when the pane changes against when the inner take changes). Each
  take is trimmed by its own lag, so all eight start on the same frame. A few
  frames of key-to-render jitter per transition remain; they only show if the
  theme changes in the tenth of a second a transition happens.
- The player in headless Chromium (`reelcheck.mjs` in the film kit, served by
  its `serve.mjs`, which answers byte ranges as Caddy does; nine checks, all pass): a
  theme switch puts the new take on show within 0.10 to 0.22 s, landing 0.005
  to 0.087 s from where the old take would have been; three switches in half a
  second end on the last theme, playing; with the CRT on, the tube's island
  receives frames; Pause stops it; reduced motion waits on the first frame.
- `scripts/verify-site.mjs` against this build: 688 passed, 0 failed.
- Web encode: H.264 High, CRF 26, a keyframe every second so a theme switch
  can seek anywhere cheaply, faststart. About 2.8 MB a take.

## Found on the way (filed)

- mpv's shared-memory mode, which kitty's own docs recommend for video, draws
  nothing in Terminal Delight, because rio-vt and the host's picture rewriter
  both treat `m=1` as chunking for non-direct media (issue 833).
- A command started by `ctl adopt --run` sees a 100×28 pane for its first
  50–200 ms (issue 834).

## Order of landing

1. Core swap merges (another session owns the review and the merge).
2. This branch rebases onto main; `trueOf` on the homepage becomes
   `main at <merge commit>`; the docs build and verify; pull request, merge.
3. Deploy the docs (`docsite/deploy.sh`).
4. Publish the series (fast-forward parkerbrown-dev main, `./deploy.sh`).

## Progress

- [x] References, homepage copy, player, build step
- [x] Film kit, sixteen takes filmed, two re-filmed on a quieter machine
- [x] Aligned, encoded, the player verified in a browser
- [x] Merged (`96c7d12`) and deployed after the core swap landed; the series
  published on parker.brownfamilysports.com (parkerbrown-dev main `c91ed2a`).
  The reel's browser check passes 9 of 9 against the live site.

## Re-filmed on the flat default (2026-09-25, evening)

Pull request 821 made a fresh install open flat: the tube off, the CRT an
easter egg in the gauges. Parker, seeing it beside the old look: *"looks 100x
cleaner for public facing draw"*. Of this reel he asked for a redo in the
default theme, and said *"the theme adapter on the demo is 100000% killER!!! so
that MUST stay"*. The player, the page and the eight-theme switching are
untouched. Only the takes changed.

- **The look is the fresh-install default** (`theme::house_outer` at
  `08a0a4a`): `crt = false`, on-theme program colour, agentic syntax,
  brightness −24, contrast +25, colour +50, and the `hacker` theme file a fresh
  install seeds. Each take still wears its docs theme's palette, which is the
  adapter. Only the two size dials keep the film's framing (text 100%, menu
  bar 99%), for legibility at the size the page plays the video.
- **A grade with no `crt` key now reads as on.** The old film layouts wrote
  none, so a re-film from them would have come back with scanlines, bloom and
  vignette. The layouts now say `crt = false` explicitly.
- **The nested tour froze in three of the first eight outer takes**, and in
  ethereal every time (six of six). The window's log said *"pane 5 is being
  shown by another window; this one keeps what it last drew and stops there"*.
  mpv's Kitty frames outran the host's 512-chunk queue for that window, and the
  host dropped the stream as its comment intends ("it re-attaches"). The
  window then read the live pane as taken by another window, and froze it.
  Filed as issue 853. **mpv is now capped at 15 fps** (`--vf=fps=15` in the
  template's `bin/reel`). With the cap, all eight outer takes dropped zero
  frames, against 218 to 373 before, and every one passed on the first try.
  The tour was already showing about 20 fps inside the pane after drops, so
  the cap costs little.
- **The correlation in `align.py` stops meaning anything on dark, flat
  palettes.** Scores fell to 0.02 on the frozen takes, and a lag search from 0
  to 1.5 s cannot see a tour that stopped. Two cross-checks now sit beside it
  in the kit. `align-events.py` times the tour's own chords (queue, workbench,
  wall) in each take and takes the median. `wall-time.py` finds when the agent
  wall lands in absolute seconds, which is what proved the frozen takes were
  frozen. The takes are trimmed by the event medians, 0.133 to 0.167 s.

Measured on the takes as shipped: the wall lands at 18.63 to 18.73 s in all
eight takes, a spread of three frames. Each take is 29.97 s and 2.2 to 2.7 MB.
The reel's browser check passes 9 of 9 locally, with theme switches landing
0.002 to 0.090 s from where the old take would have been.
`scripts/verify-site.mjs` passes 688 of 688.

## Retrospective

**Post-hoc difficulty: 7 / 10, as scored.** The coupling cost what the score
predicted: every public step waited on another session's review and merge of the
swap, and the review changed facts the series had already stated, which cost two
rounds of corrections to Part 5.

What went wrong that was not code:

- Two outer takes were filmed while an encode test ran on the same machine, and
  their dropped frames doubled; both were re-filmed.
- A probe window stayed up on the hidden workspace, mock agents running, for
  about ninety minutes, and one early take sent a real desktop notification
  before the rig stubbed `notify-send`.
- The public copy went out before the house slop pass. Run afterwards, it caught
  a comparison with no scale on this homepage and an undefined term in Part 5.
- The first plan for publishing the series was a deploy from a clean checkout,
  which would have reverted the live Offerings page, built from uncommitted work
  (parkerbrown-dev issue 24). Comparing the build with the live site, page by
  page, caught it before anything shipped.
- The reel was filmed on the build before the swap's review. Nothing it shows
  changed in review; a re-film from main is about twenty minutes with the kit.
