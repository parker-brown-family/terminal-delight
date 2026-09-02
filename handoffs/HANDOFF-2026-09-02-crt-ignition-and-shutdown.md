# Handoff — CRT ignition & shutdown (2026-09-02)

## Status

**Landed and deployed.** `origin/main` = `9872d26`, deployed as
`~/.local/lib/terminal-delight/td-9872d26` with `~/.local/bin/terminal-delight`
repointed.

- [#249](https://github.com/parker-brown-family/terminal-delight/pull/249) → `713012b` — the ignition
- [#252](https://github.com/parker-brown-family/terminal-delight/pull/252) → `9872d26` — the shutdown + a contrast fix

Verified by **content**, not by filename: the deployed binary differs from
`td-713012b` and carries `crt-shutdown`, which `td-713012b` does not. (The first
attempt at this deploy shipped the wrong bytes — see **Watch out**.)

## What's done

**A tube fires when it opens.** A new pane whose barrel warp is on plays a 300ms
ignition over its screen: phosphor floods in from the top and bottom edges, the
flood collapses to one scan line, the line pinches to a four-point star that
fades as the terminal comes up behind it. A flat pane plays nothing.

**And goes dark when it closes.** Same arc from the collapse onward, on pane
close and tab close. A closing pane is dropped immediately — that drop IS the
close, it releases the PTY — so it cannot animate itself. It leaves a `Ghost`
(last painted rect, curvature, glare) and the workspace plays the effect over
the vacated space, re-registering that rect as an overlay tube so the dying
screen bends as the pane did.

**A fresh window opens at contrast 0.** `house_outer` shipped `contrast: 0.21`,
which the DISPLAY tray prints as `−29`. Now neutral. Brightness (`−12`) still
carries the darkening.

*Verified:* 422 tests, clippy clean, four required CI checks green on both PRs.
*Not verified:* nobody has SEEN either animation.

## How to run / verify

```bash
cd ~/Work/terminal-delight/app && cargo test
```
```bash
TD_IGNITION_FREEZE=0.55 terminal-delight
```

`TD_IGNITION_FREEZE` pins the ignition at any instant `0..1` for as long as the
app is up — `0.15` bloom, `0.5` collapse, `0.8` star. The **shutdown has no
equivalent**: it plays on a pane that no longer exists, so there is nothing to
hold still. Check it by closing a pane in a warped tab.

Redeploy after a merge:

```bash
cd ~/Work/terminal-delight/app && cargo build --release
```
```bash
cp app/target/release/terminal-delight ~/.local/lib/terminal-delight/td-<sha> && ln -sfn ~/.local/lib/terminal-delight/td-<sha> ~/.local/bin/terminal-delight
```

## Not done / next

- **Look at both effects.** Neither has been seen. This is the whole open item.
- [#240](https://github.com/parker-brown-family/terminal-delight/issues/240) — the new-window notification burst. **Now runnable**: post-fix binaries are live (3 windows on `td-f3484fc`, 2 on `td-9872d26`), where before the burst could only be observed on a build predating the fix.
- [#250](https://github.com/parker-brown-family/terminal-delight/issues/250) — `td-demo`'s headless capture is dead, and its obvious workaround leaks the desktop.
- [#236](https://github.com/parker-brown-family/terminal-delight/issues/236) — the synthetic-state seam. `TD_IGNITION_FREEZE` is the proven template; generalise it.

## Watch out

- **Do not deploy while a background `cargo build` is still running.** The first
  `td-9872d26` deploy copied the previous artifact mid-compile and named it from
  a `git rev-parse` taken afterwards. Hash the new binary against the previous
  deploy — byte-identical across two commits means you shipped the old one.
- **`grim -w <address>` does not fail on an unresolvable address — it captures
  the whole screen.** It photographed Parker's real desktop once this session.
  Check a throwaway shot's dimensions before pointing any region capture at a
  screen with real content.
- The tree is shared. `~/Work/td-reveal` and `~/Work/td-themes` are other agents'
  worktrees; `handoffs/HANDOFF-2026-09-01-pane-grade-provenance.md` is not mine.
  Never `git add -A`.
- The animations are gated on barrel warp on purpose. A flat pane has no tube to
  fire and the flash reads as a glitch.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-02-the-tube-fires.md`
- Harvest: `episodes/2026-09-02-the-tube-fires.cdx`
- APES kanban: `…-mtjqc5e5` (ignition + shutdown), `…-mtjvdt0p` (contrast), `…-mtjve3vp` (#250)
- lean-ctx: session decision recorded
- file-memory: `a-deploy-can-race-its-own-build`, `a-captures-fallback-is-a-privacy-boundary`
- Earlier arc, same session: `episodes/2026-09-01-per-agent-tab-badges.md`
