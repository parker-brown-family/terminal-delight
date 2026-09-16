# Handoff — Omarchy kiosk: theme spine + travelling picker (2026-09-02)

## Status

**Landed and live.** Five PRs merged to `main` and published to
`terminal-delight.brownfamilysports.com/omarchy.html`, each verified against the
LIVE url after publish:

| PR | Squash | What |
|---|---|---|
| #244 | `126bdbd` | Theme spine — every stop wears an Omarchy theme + its own wallpaper |
| #246 | `5893b0d` | Grid rail, standing toggle, lightbox, MCP section, sharp bottom |
| #247 | `c3c87ea` | Ride-along picker docked in the gutter |
| #248 | `8a2dd0e` | A form at every width — gutter panel, column, tray |
| #251 | `69cbe84` | Scatter-and-reassemble replacing the snap |

Nothing of mine is uncommitted. Worktree `~/Work/td-themes` sits on the merged
`feat/kiosk-rail-reassembly` at `58e7a28`.

## What's done

- **Seven themes**, gruvbox → osaka-jade → tokyo-night → ethereal → everforest →
  miasma → retro-82, one per stop, each with that theme's own shipped wallpaper
  (504KB total, 1600px cap + webp). catppuccin-latte was dropped — the page has
  no light theme now, and the light-mode CSS paths are unexercised.
- **The picker travels**: hero chips disperse on golden angles with
  distance-proportional blur while the ride-along's arrive from the right and
  converge. Both take clicks throughout; each chip has a static hit slot so a
  mid-flight click cannot hit the wrong theme.
- **A form at every width**: 214px gutter panel past 1620, 158px from 1250,
  126px column from 900, tray under the nav below that.
- **Bottom of the page is sharp at any height** — the loop lifts every stop back
  to full focus inside the last half-screen of scroll.
- **MCP section second from the top**, naming all five tools.

## How to run / verify

```bash
cd /home/parker/Work/td-themes && python3 -m http.server 8765 --bind 127.0.0.1
```
```bash
node /tmp/claude-1000/-home-parker/dd0e2283-a592-47ea-9ac6-b354d8493325/scratchpad/shoot3.js http://127.0.0.1:8765/omarchy.html
```
```bash
node /tmp/claude-1000/-home-parker/dd0e2283-a592-47ea-9ac6-b354d8493325/scratchpad/shoot3.js https://terminal-delight.brownfamilysports.com/omarchy.html
```

The harness uses Playwright from the wellness-with-kate `node_modules` with
`executablePath: '/usr/bin/chromium'`, so no browser download. It asserts on
computed style: the flight at rest/mid/assembled, clicks on both rails
mid-flight, 8 round trips to the top, and the dock's form at 1800, 1440,
968x1790, 900, 760 and 390x844. **Copy it into the repo before relying on it** —
it currently lives only in this session's scratchpad.

## Not done / next

- `#253` — the vendored Omarchy wallpapers have no licence stated upstream. A
  decision for Parker, not a defect; the footer credit is the interim position.
- `#254` — no favicon on the site; every page load logs a 404.
- `#241` — TD's own window respawns still bypass `uwsm-app`.
- The verification harness deserves to be a `/page-verify` skill rather than a
  scratchpad file.

## Watch out

- **`main` moves under you.** #243 landed the same feature area mid-build this
  session and #249 landed during the tie-off. Rebuild on what landed; never
  text-merge two attempts at one feature.
- **`~/Work/terminal-delight` is a concurrent session's tree** — currently on
  `fix/house-contrast-and-shutdown` with modified `crt.rs`, `main.rs`,
  `pane.rs`. Do not commit or clean there.
- A `position: fixed` element inside `.hero-parallax` anchors to the hero, not
  the window — that wrapper carries a transform and a filter.
- The picker's `t` is measured in **scroll position**, not the hero's rect. Keyed
  to the rect it reads non-zero at rest on a tall window.

## Where it's recorded

- Episode: `apes/projects/terminal-delight/episodes/2026-09-02-kiosk-wears-the-desktop.md` (+ `.cdx`)
- Kanban: `make-the-omarchy-kiosk-wear-a-theme-per-stop-with-a-picker-that-travels-mtjraba3` (done),
  plus todo tickets mirroring #253 and #254
- lean-ctx: session decision breadcrumb
- file-memory: `fixed-inside-a-transform-is-not-fixed`,
  `scrub-dont-snap-for-scroll-ui`, `verify-at-parkers-tiled-pane-width`
