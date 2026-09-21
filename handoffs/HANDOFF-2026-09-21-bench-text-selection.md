# Handoff — bench text selection (2026-09-21)

## Status

**Landed and merged.** PR #621 → `main` (`4b1fb2b`). Five commits, authored as
Parker, no AI trailers. Branch `bench/text-selection` and worktree
`~/Work/td-textsel` are kept (not deleted) with a private `target/` excluded via
`.git/worktrees/td-textsel/info/exclude`.

## What's done

Press inside text on the bench, drag, release: the characters between highlight
across element and region boundaries; release publishes to PRIMARY;
`ctrl+shift+c` and the right-click tray take the clipboard. `ctrl+c` untouched.

| Piece | Where | Verified by |
|---|---|---|
| `sel()` + `collecting` RAII guard + `Drawn` | `benchdraw.rs` | compiles; live inventory below |
| `region_probe` × 3 (body, rail, composer) | `pane/bench.rs` | `regions=2` observed with the composer hidden |
| `atom_probe` → paint-phase `resolve` | `benchdraw.rs`, last child of the tree | zero panics across six live launches |
| `Region`/`Atom`/`Caret`/`Sel`/`atom_at`/`spans`/`copy_text`/`on_boundary`/`still_valid`/`region_of` | `workbench.rs`, gpui-free | 18 table tests + 5 mutations |
| press deferral (`Hit::Arm`, `Hit::OpenRow`, 5px slop) | `pane.rs` mouse handlers | compiles; **not** pointer-tested |
| highlight overlay from the previous frame's list | `pane/bench.rs` `bench_highlight` | **not** looked at |

Gates on the merged tree: **1414 tests**, `clippy --locked -- -D warnings`
clean, `cargo fmt --check` clean.

## How to run / verify

```bash
cd ~/Work/td-textsel/app && CARGO_TARGET_DIR=~/Work/td-textsel/target cargo build --release
```

A **shell pane proves nothing** — it draws the empty bench. `classify_foreground`
calls a pane a Shell whenever the foreground pgid IS the shell, so the stand-in
must be a CHILD:

```bash
mkdir -p /tmp/rig && printf '#!/usr/bin/env bash\nwhile :; do read -r _ || sleep 1; done\n' > /tmp/rig/claude && chmod +x /tmp/rig/claude
printf '#!/usr/bin/env bash\n/tmp/rig/claude\n' > /tmp/rig/shellwrap && chmod +x /tmp/rig/shellwrap
setsid env TD_SESSION=selrig TD_WORKBENCH_DEMO=1 TD_SELDEBUG=1 SHELL=/tmp/rig/shellwrap ~/Work/td-textsel/target/release/terminal-delight
```

Then move it off the operator's screen and turn the bench on:

```bash
hyprctl dispatch movetoworkspacesilent "6,address:<addr from hyprctl clients -j>"
~/Work/td-textsel/target/release/terminal-delight ctl --pid <pid> bench on
```

Expected inventory on a real response card:

```
[bench-sel] runs: body=5 rail=2 composer=0 chrome=0 regions=2
```

with the card's title, origin line, two register tabs, gist, and both rail row
titles. **`body=1` means only `micro()` is registering** — that was the bug.

## Not done / next

- **#623 — the drag has never been exercised by a real pointer.** Highlight
  rectangle alignment is completely unchecked. This is the real remaining gate.
- **#622 — the selection wash is faint.** Measured 1.47:1 on `quiet-command`,
  1.77:1 on `hacker`, composited over `panel`. Root cause shared with #599.
- Shift-motion selection in the composer remains unbuilt (#541 covers it).
- Tooling idea not built: `scripts/bench-sel-smoke.sh`, since the rig above was
  re-typed seven times by hand. `workbench-smoke.sh` stages only shell panes and
  cannot reach any agent-pane feature.

## Watch out

- **Do not build from `~/Work/terminal-delight`** for this — it was 66 commits
  behind `origin/main` with another session's 444 uncommitted lines in the same
  files. That is why a separate worktree exists.
- **Do not share `CARGO_TARGET_DIR` with another worktree.** It served a stale
  test binary here: a green 1411 became 1397 with all sixteen new tests absent.
  Confirm with `cargo test -- --list | grep <a test you just wrote>`.
- **Do not `pkill -f <pattern>`** — the pattern is in your own command line and
  the kill lands on the command issuing it (exit 144, work after it silently
  skipped). Filter `/proc/*/exe`.
- The composer is included **knowing it is throwaway** — the decouple
  (`docs/plans/workbench-drives-the-agent/`) replaces the mirror. Parker's call:
  *"do the work now even if it is throwaway."*
- `benchdraw` may not read `std::env` — `a_renderer_contains_no_decisions`
  enforces it. Pass flags in.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-21-the-tracer-found-what-the-tests-could-not.md`
- Kanban: `make-the-workbench-s-card-and-rail-text-selectable-and-copyable-mubfl0d6` (done), plus two follow-up tickets mirroring #622 and #623
- lean-ctx: session decision recorded (`ctx_knowledge` was not bound this session)
- Session package: `handoffs/2026-09-21-bench-text-selection.cdx`
- Plan + brief: `docs/plans/text-on-the-bench-is-selectable/00-status.md`, `reports/2026-09-21-text-on-the-bench-is-selectable.html`
- PR: https://github.com/parker-brown-family/terminal-delight/pull/621
