# Handoff — Super+Ctrl-click reveal + uwsm launch route (2026-09-01)

## Status

Committed, **not pushed, no PR**. Isolated worktree `~/Work/td-reveal` on
`feat/super-ctrl-click-reveal`, two commits off `origin/main@2a8661e`:

- `3e6fea9` — Super+Ctrl-click on a path reveals it in the file manager
- `631b2f7` — Launch opened links the way a uwsm session expects

`origin/main` moved to `f3484fc` during the session, so the branch is **ahead 2,
behind 3** — rebase before opening the PR. `main` is merge-on-green (required
checks, 0 required reviews), so the PR can be landed by the agent that opens it.

## What's done

- **Super+Ctrl-click reveals** the path under the cursor: file manager opens with
  the item *selected*, via `org.freedesktop.FileManager1.ShowItems`; a desktop
  exporting no such interface gets the containing folder opened instead. Right-click
  gained **Reveal in folder ⌖**, shown only when the link is local.
  *Verified:* 413 tests (+3), and Parker confirmed live that Hyprland passes
  SUPER+CTRL through despite `SUPER + mouse:272` being bound to window-drag.
- **uwsm launch route:** `xdg-open` (the pre-existing Ctrl-click open) and the
  reveal fallback now run as `uwsm-app -- …` when
  `$XDG_RUNTIME_DIR/uwsm-app-daemon-in` exists, so the opened app gets its own
  systemd scope instead of the terminal's cgroup. Non-uwsm sessions unchanged.
  *Verified:* tests for both script branches; `strings` on the release binary
  confirms both new code paths are in the built artifact.
- Help overlay row, `info.html` line, CHANGELOG entries, strings in all 9 locales.
- **Outside the repo:** `~/.config/mimeapps.list` now pins
  `text/markdown` + `text/x-markdown` to `omawrite.desktop`. Parker's `.md` files
  had been opening in Chromium — `xdg-open` reads only `[Default Applications]`
  while GIO also reads `mimeinfo.cache`, so Nautilus was already right and the
  terminal was not. *Verified* by querying GIO directly, not by launching.

## How to run / verify

```bash
cd /home/parker/Work/td-reveal/app && cargo test --bin terminal-delight
```
```bash
cd /home/parker/Work/td-reveal/app && cargo clippy --bin terminal-delight && cargo fmt --check
```
```bash
/home/parker/Work/td-reveal/app/target/release/terminal-delight
```

Then Super+Ctrl-click a printed path (the reveal), Ctrl-click one (the open), and
right-click a link (the menu item). To check the MIME fix without launching
anything, query GIO for `text/markdown` from a `python3` **script file**
(`python3 -c` is blocked by the shell policy here).

## Not done / next

1. Rebase on `origin/main`, push, open the PR.
2. `#241 — Scratch and demo windows respawn inside the parent's cgroup instead of
   their own app scope` (label `follow-up`) — TD's own window respawns in
   `main.rs` still bypass `uwsm-app`. The issue carries the invalidation criteria;
   run them before fixing.
3. Not attempted: reveal from the FOCUS reader, or an "open with…" chooser.

## Watch out

- The **primary checkout** `~/Work/terminal-delight` is a *different* working
  tree, moved to `main` by a concurrent session mid-thread. Its untracked
  `assets/omarchy/`, `omarchy.html` and `handoffs/HANDOFF-2026-09-01-pane-grade-provenance.md`
  belong to that session — do not commit or clean them.
- `uwsm-app` must stay gated on the daemon FIFO. Calling it on a desktop without
  `wayland-wm-app-daemon` makes it restart units and block on timeouts.
- This handoff is untracked on purpose; the tie-off records, it does not ship.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-01-reveal-in-file-manager.md`
  (+ `.cdx` harvest beside it)
- APES kanban: `reveal-a-clicked-path-in-the-file-manager-with-super-ctrl-click-mtj6i863`
  (done) · `route-scratch-and-demo-window-respawns-through-uwsm-app-mtj6i61e` (todo, → #241)
- lean-ctx: session decision recorded (resume breadcrumb)
- file-memory: `two-mime-resolvers-disagree`, `omarchy-launches-gui-apps-via-uwsm-app`
