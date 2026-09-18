# Handoff — the 2026-09-18 merge sweep, second cutover, and the launch cleanup pass

Follows `HANDOFF-2026-09-17-launch-and-cutover.md` (the first launch). This is the second day: the morning's work swept onto main, both windows cut over again, and the major post-launch cleanup.

## Status

**Landed and cut over.** `main` at `f3809e4`. Installed binary `~/.local/lib/terminal-delight/td-f3809e4-main` (`app/` byte-identical to origin/main), `~/.local/bin/terminal-delight` points at it. Both live windows run it: session `1` (workspace 1, pid `1214556`, 21 agents) and `tdclip` (workspace 6, pid `1207996`, 11 panes). Session hosts untouched — `td-fc6957f-main serve --session 1` and `td-d1f0a79-paint-the-outer serve --session tdclip` still own every terminal. **Zero open PRs.**

## The merge sweep

46 branches sat ahead of main; 44 were merged-PR residue and 2 held nothing new. The only live work was the bench author's stacked pair, tied off at 10:37:

- **#527 (window chords)** was based on **`bench/gauges-and-keys`**, not main — so merging it landed it into that branch, not main. Main was never touched; nothing lost. (Lesson: check `baseRefName` in a sweep — a stacked PR merges into its base.)
- **#522 (gauges + keys)** then needed main merged in. One conflict in `benchdraw.rs` — main's fold-with-a-press-target rendering kept, this branch's gauge-sized `micro()` kept. Gate: fmt, clippy `--all-targets`, **1,281 tests, 0 failed**. Merged as `f3809e4`, carrying both.
- **#529** rescued the instance-identity handoff that existed only as an untracked file.

Flagged, not fixed: 5 `clippy --all-targets` lints in test code (confirmed on main at `8dd7d7f`, not from #522) — CI runs clippy without `--all-targets`; evidence added to **#430**.

## The cutover (window-only, hosts kept)

`td-f3809e4-main` built and installed. tdclip relaunched first (it had no window — its host still held 11 panes) via `hl.dsp.exec_cmd(cmd, { workspace = "6" })`; then session 1's window bounced (killed, host survived, relaunched onto workspace 1). Both answer `whoami`; the WS 1 window verified rendering 22 panes with agents mid-turn.

**One degradation, filed:** the WS 1 bounce reaped 3 `[[closed]]` close-undo holds (panes 64/65/71) that still had 540–917 s of recovery window left — evidence added to **#412**. No live work lost (they were already-closed panes); the un-close grace period does not survive a window swap.

## The cleanup pass (major tie-off)

| What | Before | After |
|---|---|---|
| Worktrees | 26 | 8 |
| Local branches | 78 | 8 |
| Merged remote branches | ~34 | 0 |
| Installed binaries | 96 (~4.3 GB) | 11 (504 MB) |

**Kept:** the main checkout, the build tree (`td-outer-paint` on `main`), the shared detached checkout (`td-cutover`), the two live-agent trees (`td-workbench`, `td-agent-theme`), and two deliberately-parked scrap trees (`td-cs-contract`, `td-demo`). `shorts-pipeline` (private) preserved.

**Safety:** every untracked handoff/report across all worktrees was confirmed already on `main` before any worktree was removed. 6 local-only non-residue branches were archive-tagged (`refs/tags/archive/*`, pushed) before deletion. Binaries deleted only if not the current symlink target and not held open by any running process (`/proc/<pid>/exe`). `main` unchanged; both windows verified answering afterward.

Closed **#458** (prune the worktrees and branches) with the above as its deliverable.

## Verify

```
readlink ~/.local/bin/terminal-delight
```
```
git -C /home/parker/Work/terminal-delight diff --stat f3809e4 origin/main -- app/
```
```
~/.local/bin/terminal-delight ctl --pid $(cat /run/user/1000/terminal-delight/session-1.window) whoami
```
The second must be empty; the third must answer `ok 1 <pid>`.

## Watch out / not done

- **The release tag is still owed.** main is 30+ PRs past v0.2.1, `app/Cargo.toml` still says `0.1.0`, and the changelog is written (#502). `bash scripts/release-smoke.sh`, bump the crate in the tag commit, push `v0.3.0` — CI attaches the AppImage.
- **`hl.dsp.focus({ workspace = N })` no-ops.** To move a monitor to a workspace, focus a *window* on it. This cost time twice; recorded in file-memory `td-cutover-window-only`.
- `td-cs-contract` and `td-demo` hold deliberately-parked superseded drafts (uncommitted `app/src/main.rs`). Left as the 2026-09-16 handoff intended; zap when convinced.
- The memory index (`~/.claude/.../MEMORY.md`) is at 174 distinct lessons, over the soft 140-line target. Not auto-pruned — going lower is a curation call, since each remaining line is a live lesson.
