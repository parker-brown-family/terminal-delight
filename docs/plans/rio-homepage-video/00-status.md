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
