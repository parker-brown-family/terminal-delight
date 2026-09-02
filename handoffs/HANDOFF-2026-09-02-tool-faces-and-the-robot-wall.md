# Handoff — tool faces and the robot wall (2026-09-02)

## Status

Shipped and pushed: `77e6532..d959a1f` on `main`, installed as
`td-d959a1f-robots`. Issue #266 closed, #255 closed by the push. Three
follow-ups open: #267, #268, #269.

**The one thing left is a relaunch.** Every terminal-delight process on the box
at the time of writing is `td-393e572-usage`, started 10:57. Running windows hold
the binary they started with, so nobody has yet watched the robots move.

## What it does

A pane's logo follows the tool its agent is holding.

- **Pane header square** (~18px): the prop alone, a still PNG. A whole robot is a
  smudge at that size; one object still reads.
- **Agent wall card** (228×116): the playhouse's robot, animated — holding the
  prop, wearing the face that tool's row specifies, with the **verb** lettered on
  the glass ("at the console", not `Bash`).
- **Precedence:** explicit `logo` → tool → `dir_logo` → placeholder. The face is
  set only while the agent is *working*, so a busy pane wears what it is doing
  and a resting one goes back to wearing where it is. That is why ranking the
  tool above the per-directory default cost that default nothing.
- **Off switch:** `follows_tool = false` in
  `~/.config/terminal-delight/tool-props.toml`, which is also where a user can
  add or override rows without a rebuild.

## How the art gets here

Nothing is hand-maintained. `scripts/sync-tool-props.mjs` generates everything
from a sibling agent-playhouse checkout:

```bash
node scripts/sync-tool-props.mjs
```
```bash
node scripts/sync-tool-props.mjs --check
```

Two pipelines, because the two assets are different kinds of thing:

- **Props** are `<g>` elements in `web/art/props.svg` plus the `.pa-*` rules —
  static art, rasterised with `rsvg-convert`, then trimmed to the ink and
  re-centred. That trim matters: a prop is drawn so its GRIP lands at (27, 17)
  where the robot's hand is, so it is deliberately off-centre in its own box.
- **Scenes** are photographed, because the robot is not a file anywhere — he is
  composed at run time from an SVG rig, a stylesheet posing him by `data-state` /
  `data-face`, and three custom properties. `scripts/lib/shoot-sheet.mjs` serves
  the playhouse's `web/` on an ephemeral port, drives a headless chromium over
  the DevTools protocol (no Playwright, no npm install — node has had a WebSocket
  since 22), and screenshots cells out of `/sheet.html`.

Animation frames are sampled off the playhouse's own CSS: pause every animation,
wind each frame on with a negative `animation-delay`. Durations are re-timed to
divide a 1.24s loop (two of the rig's 0.62s forearm pumps) so the 3.4s hover and
5.2s blink close the loop instead of snapping back.

`--check` byte-compares props (rsvg is deterministic) and compares scenes by
RMSE against a 0.04 tolerance, since screenshots are not byte-reproducible across
chromium builds.

## Why it is affordable

gpui decodes multi-frame WebP itself and advances it on its own clock — but only
while the window is ACTIVE and the element is actually laid out
(`gpui/src/elements/img.rs`). So a closed wall, a background window, and every
pane header (still prop, never a scene) all cost zero. The bill is bounded to
"somebody is looking at the wall while an agent works", and the render code never
changed: it is the same `img(path)` it already called.

19 animated scenes cost 776KB committed, because WebP stores only the changed
rectangle per frame and most of a scene is a dark room. The decoded cost is
~6.4MB per *distinct* scene in gpui's cache — arithmetic, not a measurement,
which is #268.

## How to verify without killing a session

```bash
node scripts/td-pane-tools.mjs
```

Asks every live instance, over its own control socket, what each agent pane is
holding — and reports the pid and binary behind each one, so "is this window even
running my build?" is the same command. A pane showing `—` is an idle agent,
which is the design.

To see it without relaunching the live window:

```bash
TD_SESSION=toolprop-test /home/parker/.local/bin/terminal-delight
```

`TD_SESSION` is precedence #1 for the state key, so a second instance gets its
own `sessions/<name>.toml` and lock and cannot clobber the live one.

## Two faults worth remembering

**The feature shipped switched off.** `#[derive(Default)]` beside
`#[serde(default = "yes")]` yields `false` — serde's default applies only to a
field missing from a document being deserialised, never to `Default::default()`.
No machine ships `tool-props.toml`, so the `unwrap_or_default()` branch is the
only branch that ever runs. Eleven tests passed; not one went through the switch.
Fixed in `821df57` with a hand-written `Default` and regression tests.

**A new tile is not a new process.** Splitting a pane forks a shell inside the
already-running window. Two rounds went into "I opened a tile, ran an agent, saw
nothing", which is indistinguishable from a render bug by looking. The decisive
check is `readlink /proc/<pid>/exe`, which is why `td-pane-tools.mjs` now reports
it.

## Watch out

- `~/Work/terminal-delight` is a **shared worktree** with other agents' uncommitted
  edits appearing mid-session (`app/src/pane.rs` and `app/src/sticky.rs` were
  modified by someone else during this thread). Stage explicit paths; never
  `git add -A`.
- agent-playhouse is being actively edited in parallel. `web/playhouse.css`
  changed 19 minutes after the bake here. `--check` passed, but nothing runs it
  automatically — that is #267.

## Where it is recorded

- Issues: #266 (closed, full record in comments), #255 (closed), #267/#268/#269 open
- Memory: `merged-is-not-installed` (extended), `test-the-default-that-ships` (new)
- APES task: `…-mtkk2l1w`, done
