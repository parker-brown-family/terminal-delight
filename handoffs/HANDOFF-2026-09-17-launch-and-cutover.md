# Handoff — the launch: everything onto main, the Workbench checked, both windows cut over (2026-09-17)

## Status

Landed. `main` at `1529420`; every pull request that was open at 15:00 is merged and nothing is open. The installed binary is `~/.local/lib/terminal-delight/td-70226ea-main`, built from the commit that carries everything but the changelog (`git diff --stat 70226ea origin/main -- app/` is empty), and `~/.local/bin/terminal-delight` points at it. Both live windows run it: session `1` (workspace 1, pid 3629435, 19 panes) and session `tdclip` (workspace 6, pid 3603370, 11 panes). Their session hosts were not touched — `td-fc6957f-main serve --session 1` (pid 876777) and `td-d1f0a79-paint-the-outer serve --session tdclip` (pid 714673) still own every terminal, which is why nothing an agent was doing stopped.

The Workbench paragraph is in `~/.config/agents/AGENTS.md` (appended at the end, with a dated comment naming the build it was pasted after; a backup sits beside it). Every agent session that starts from now on is told about the bench.

## What landed, in order

| When | What | How |
|---|---|---|
| 15:19 | #465 pane-mode reconciliation, #473 unfiled-rows divider | squash |
| 15:2x | #470 trays stop at the render — the change the desktop's binary had been carrying off-branch since last night | squash via the REST merge endpoint, after `gh pr merge` sat on `UNKNOWN` mergeability |
| 15:29 | #469 instance identity — after a `rustfmt` commit and a `trim_split_whitespace` clippy fix on the branch | merge commit |
| 15:38 | `workbench/surface-protocol` pushed for the first time — 54 commits that had never had a remote ref | `git push -u` |
| 15:52 | **#493 The Workbench** — the branch merged with main (four conflicts, one working-detection rule kept for both sides), plus three landing commits from the branch's own adversarial review | merge commit `858b3e7` |
| 15:59 | #494 eleven handoffs/reports that existed only as untracked files, plus three briefs that existed only in the stale td-glyph checkout | squash |
| 16:0x | main built and installed as `td-858b3e7-main`; the Workbench checked on a window of its own (below) | — |
| 16:4x | **#498 provenance** — `workbench/provenance` (five commits: the bench signs what it types, verb previews, origin on every card, on-screen-or-queued typing, the reach dial) merged with main (five conflicts; this branch's richer versions of the three landing fixes won, the bench's #489 focused-pane rule and its `composer_follows` kept) | merge commit `70226ea` |
| 16:5x | #502 CHANGELOG catches up with the forty pull requests of the week | squash |
| 17:0x | `td-70226ea-main` built from main and installed; WS 6 cut over (canary), then WS 1 | window-only bounce |

Wave 3 also ran: three dead or redundant branches archived as tags (`archive/keepalive-verify-before-send`, `archive/spine-slice-1-tracer`, `archive/install-passdown-trays`) and deleted; 14 worktrees that held nothing main lacked removed (34 → 20); 24 local branches that were true ancestors of main deleted (78 → 54). Protected on purpose: `td-outer-paint` (the build worktree on `main`), `td-workbench` and `td-provenance` (the agents' own trees), `td-cutover` (the detached main checkout other sessions `cd` into).

## The Workbench check

`~/Work/reports/wbcheck-2026-09-17-run2/RESULTS.md` — 18 checks, 18 passed, on a window of its own (`TD_SESSION=wbcheck`, demo surfaces seeded through the real file transport), every gesture through `ctl` or the keyboard, every photograph taken only after asserting the active window was ours. Verified: the two faces; six kinds rendered from files (the unknown kind shown as `unclassified`); three more surfaces dropped by a second process picked up within a sweep; Down selects a row, Enter opens the card (and, on a decision, approves it); `ctl bench choose 1` reaches the open decision and the answer is journaled and typed into the pane (`[workbench] approve on surface demo-decision`, `[workbench] choose on surface demo-decision ·`, both visible on the terminal face); `present_surface`, `declare_deliverable` and `surface_catalogue` over `ctl mcp rpc`, each answer stamped with the answering instance; window and host alive at the end, no panic.

**The first run of the same script was wrong and looked right.** Every photograph was of the bench agent's floating test window, which sat over workspace 5 on the laptop panel; the pixel comparisons "passed" on its cursor blinking. Read the images, not the RESULTS line — the second run's shots carry `(active window verified ours)` because the script now refuses to photograph anything else. The trap is already filed as #491.

Not verified here: any mouse gesture (this shell has no pointer), the composer against a real agent pane (the check window's first pane is a shell), and the signed `[workbench:<tag>]` line end to end — the check ran on `td-858b3e7-main`, before provenance landed; the installed build has the strings (`Reading your answer`, the tag form) but nobody has pressed a verb on it yet.

## Follow-ups filed

- #499 — an answered decision still reads WAITING on the rail; `choose` on a decision journals an empty target.
- #500 — a changeset whose hunk id carries control characters never reached the bench and nothing said why (fixture in the wbcheck scratch; the spec promises a journaled refusal).
- #501 — a floating window died when the compositor was asked to move it to a workspace that did not exist; its host survived. Hunch, one observation.

## How to verify

```
readlink ~/.local/bin/terminal-delight
```
```
~/.local/bin/terminal-delight ctl --pid $(cat /run/user/1000/terminal-delight/session-1.window) whoami
```
```
git -C /home/parker/Work/terminal-delight diff --stat 70226ea origin/main -- app/
```
The second must answer `ok 1 <pid>`; the third must be empty before the installed label is trusted.

## Watch out

- **`hl.dsp.focus({ workspace = N })` returns `ok` and switches nothing.** Focusing a *window* by address is what moves a monitor to that window's workspace. `hl.dsp.exec_cmd(cmd, { workspace = "N" })` places a new window; `hl.dsp.window.move({ window = "address:…", workspace = N })` moves an existing one and drags the workspace onto the focused monitor — `hl.dsp.workspace.move({ workspace = N, monitor = "eDP-1" })` puts it back.
- A grim shot by geometry is whatever sits in that rectangle (#491). Assert `hyprctl activewindow` is yours before every shot.
- The bench2 window (the workbench agent's floating test window) is gone — it died on a `window.move` to a non-existent workspace (#501). Its host has since exited too. Relaunch with `TD_SESSION=bench2` if it is wanted.
- `install/passdown-trays`, `spine/slice-1-tracer` and `keepalive-verify-before-send` exist only as `archive/*` tags now. The APES backlog item asking for a slice-1 PR is answered by #460 and should be closed.
- The tag is still yours: v0.2.1 is the last release, `app/Cargo.toml` still says 0.1.0, and #502 wrote the notes a `v0.3.0` would ship. `bash scripts/release-smoke.sh`, bump the crate version in the same commit as the tag, push the tag, and CI attaches the AppImage.
