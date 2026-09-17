# Handoff — the paint overlay goes flat (2026-09-16)

## Status

Landed. `7e647d6` on `chrome/paint-the-outer`, merged as **`ffbc6c5`** (PR #464), all five
checks green. Built and installed as `~/.local/lib/terminal-delight/td-ffbc6c5-main`, with
`~/.local/bin/terminal-delight` repointed at it. Nothing uncommitted from this thread.

**`origin/main` has since moved 4 commits past the installed binary** (#468, the notify glyph
— merged by another session while this one was finishing). The shelf binary does not carry it.

## What's done

- **The fix:** `|| theme::paint_mode(cx)` in the `warp::set_suppressed` list in
  `Workspace::render`. While the paint overlay is up the barrel pass is suppressed, so the wall
  of pane cards and the cabinet's card read flat and their tiles are clicked where they are
  drawn. *Verified by photograph* — before, after, and once more against the installed merge
  build (FIG 5 of the brief).
- **The declaration:** `Warped::FlatByDesign` now carries a `flag` beside its reason, and the
  paint wall and cabinet card have rows in `OVERLAYS_OVER_PANES`.
- **The gate:** `every_overlay_over_panes_decides_about_the_warp` looks each claimed flag up in
  the suppression list, reading the source **with `//` comments stripped**. *Verified by
  breaking it* — the entry was deleted and the test watched failing, twice: the first version
  (no stripping) passed, because the comment left behind still said `paint_mode`.
- **Suite:** 960 passed / 0 failed, `cargo fmt --check` clean, `clippy --all-targets -D
  warnings` clean.
- **Brief updated** with the photographs, the pipeline diagram, and a correction: its earlier
  "ACCEPTED — the card does not bend with the CRT warp" bullet was false when written.

## How to run / verify

```bash
cd /home/parker/Work/td-outer-paint/app && cargo test --bin terminal-delight
```
```bash
TD_SHOOT_OUT=/tmp td-shoot-surface after=/home/parker/.local/lib/terminal-delight/td-ffbc6c5-main
```

The second is the visual check: a `TD_SCRATCH` window floated on the monitor you are *not*
using, paint raised over its ctl socket, `grim` at the window's own rect, everything restored.
`TD_SHOOT_VERB` picks a different surface.

To watch the gate bite, delete `|| theme::paint_mode(cx)` from the suppression list and run
`cargo test --bin terminal-delight every_overlay_over_panes_decides_about_the_warp`.

## Not done / next

- **Issue #471** — the gate derives its overlay list from the `self.` field names in
  `close_popups`, so an overlay whose flag is an app global (as `theme::paint_mode` is) is
  still invisible to it. The new flag check only covers surfaces that have a row. Falsifiable,
  with an invalidation criterion; APES kanban mirror
  `make-the-glass-gate-see-an-overlay-whose-flag-is-an-app-global-mu53tixu`.
- **Shader cutouts** — the one genuinely different answer to "must the whole screen go flat?":
  holes in the tube map, so panes keep curving behind a flat overlay. Renderer-fork work in
  `crt_pass.wgsl` plus the tube packing. Recorded in the brief under NOT BUILT, ON PURPOSE;
  worth doing only for every overlay at once.

## Watch out

- **Flat is bought in exactly one place.** The warp is a screen-space post-pass: `fs_crt` tests
  a pixel against the registered rects and nothing else. A border, opaque fill, float shadow
  and radius change how a panel *reads*, never whether it bends. Adding any surface over the
  pane area: add the flag to the suppression list and a row to `OVERLAYS_OVER_PANES`.
- **The whole screen flattens while such an overlay is up.** That is the cost, it is visible,
  and every sibling overlay already pays it.
- **Do not stage a window onto the focused workspace.** The first capture attempt here did, and
  knocked Parker's `tdclip` window out of fullscreen. `td-shoot-surface` picks an empty
  workspace on the unfocused monitor and restores; use it rather than launching by hand.
- `/home/parker/Work/td-outer-paint` was left checked out on `main` (its branch is merged).

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-16-the-paint-wall-stops-bending.md`
- Brief: `~/Work/reports/2026-09-16-painting-the-outer.html` (FIG 5, FIG 6)
- Session package: `handoffs/2026-09-16-paint-overlay-goes-flat.cdx`
- lean-ctx: `architecture/warp-flat-is-bought-by-suppression`, `testing/source-gate-strip-comments`,
  `testing/td-photograph-a-surface-rig`, plus the session decision
- file-memory: `flat-over-the-glass-is-bought-in-one-place`, `a-gate-can-be-satisfied-by-its-own-comment`
- PR: https://github.com/parker-brown-family/terminal-delight/pull/464 · Issue #471
