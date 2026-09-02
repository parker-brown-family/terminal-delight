# Handoff — pane grade provenance is now per channel (2026-09-01)

## Status

**Landed and pushed.** `2d0d8b5` (PR #226) on `main`, superseding `d45c48a`
(PR #224). Working tree clean; `main` level with `origin/main`. Built and
installed as `~/.local/lib/terminal-delight/td-2d0d8b5`, with
`~/.local/bin/terminal-delight` symlinked at it.

**Not yet live.** The running window is pid 3283 executing `td-09e6880`. The new
model and its one-shot state migration take effect on the **next TD launch**.

## What's done

**The rule:** `outer` is the live default for all thirteen grade channels. A pane
overrides a channel only where something explicitly set *that* channel on *that*
pane — one slider drag, or one named field in an MCP `set_pane_config` patch.
Everything else follows `outer` forever, new panes included.

- `GradeChannel` — names all thirteen dials. `crawl` and `tracking` had no
  addressable identity before this.
- `GradePins` — the set a pane owns; a `u16` serialized as a name list
  (`pins = ["brightness", "gamma"]`). An empty set writes nothing; an unknown
  name from a newer build is dropped rather than failing the load.
- `PaneTheme::effective` resolves the grade **per channel** over `outer`. The
  theme group still resolves as a group.
- `pin_grade(grade, channels)` replaces `set_grade` on every write path.
  `apply_config_patch` returns the channels the patch named.
- `PaneTheme::house()` pins the green terminal THEME against the warm cabinet and
  **no** grade channel.
- `house_outer()` carries `HOUSE_SCALE 0.80` / `HOUSE_TEXT_SIZE 0.75`. PR #224 had
  put those in `house_terminal()`, which was a second birth-copy; they were moved.
- RESET and "follow outer" remain distinct: RESET pins all thirteen at neutral;
  the toggle parks the pin set and hands back exactly those channels on re-detach.

**Migration** (`PaneTheme::migrate_legacy_grade`, driven from `Workspace::build`
via `SavedNode::migrate_grades` once `outer` is resolved and before any pane is
built): an old `inherit_grade = false` is folded into pins by releasing every
channel that equals `outer` **OR** still equals the birth stamp
(`house_terminal().grade`). Only the intersection stays pinned. The legacy field
is consumed and never written back.

**Verified:** 400 tests pass (8 new), `cargo clippy --release --all-targets`
clean, all four required CI checks green. One test walks all thirteen channels
asserting that pinning one moves exactly one, so a channel missing from
`ALL`/`name`/`copy` cannot silently never pin.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --release
```
```bash
cd /home/parker/Work/terminal-delight/app && cargo clippy --release --all-targets
```

After the next TD launch, confirm the migration ran:

```bash
grep -c 'inherit_grade' /home/parker/.config/terminal-delight/sessions/1.toml
```
```bash
grep -n 'pins = ' /home/parker/.config/terminal-delight/sessions/1.toml
```

Expect `inherit_grade` to be **gone** (0) and `pins` lines to appear only on panes
that genuinely diverge. On the session this was built against, eleven panes sat at
`brightness = 0.5` against an `outer` of `0.38`; those should now render at
`0.38`, and the four hand-dialled panes (`0.332`, `0.341`, `0.368`, `0.489`)
should keep theirs. `scale = 0.80` stays pinned on fourteen panes — that was set
explicitly over MCP, and explicit sets are sticky by design.

Live behaviour check: open a new pane, change `outer`'s brightness, watch the pane
follow. Drag that pane's brightness once, change `outer` again — only brightness
should hold.

## Not done / next

- **#233** — `set_pane_config` can pin a channel but never release one. An agent
  can dress a pane and cannot undress it; the only release is the human "follow
  outer" button, which clears everything.
- **#234** — `get_pane_config` reports effective values with no provenance, so a
  reader cannot tell an inherited value from a pinned one. Pairs with #233.
- **#235** — `Grade::is_neutral` counts `scale`/`text_size` as paint grades, so a
  size-only scope loses the `resolve` fast path. **Measure the `resolved_theme`
  memo first** — it may well close `invalid`.

All three carry the `follow-up` label and are mirrored as `terminal-delight` APES
kanban tickets.

## Watch out

- **Installed ≠ running.** `~/.local/bin/terminal-delight` points at `td-2d0d8b5`;
  pid 3283 is still on `td-09e6880`. Nothing described here is observable until
  the app is restarted, and a restart kills every agent pane.
- **The percent trap is unchanged** (issue #211). `menu_bar` and `text_size` take
  the slider-track fraction, not the gauge percentage: 80% is `11.111`, 75% is
  `10.714`. Posting `80` writes stored `1.42` and shows 142%.
- **`outer` is now load-bearing.** It is what every undressed pane inherits, so a
  drag on the outer scope's sliders moves panes too. Its menu bar had drifted to
  74.86% mid-session and was set back to 80%.
- **Migration is one-shot and inference-based.** It cannot distinguish a human who
  deliberately dialled a channel back to exactly its birth value from one who
  never touched it; that reads as untouched. One drag restores it.
- **This is a shared worktree.** Three PRs from concurrent sessions landed on
  `main` on top of this work during the session. Check `git status` before
  building.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-01-outer-is-the-default.md`
- Session harvest: `…/episodes/2026-09-01-outer-is-the-default.cdx`
- APES kanban: ticket `make-outer-the-live-default-for-a-pane-s-grade-owned-per-channel-by-provenance-mtj1zph4` (done), plus three backlog follow-ups
- lean-ctx: session decision recorded
- File-memory: `a-copied-default-is-an-override`, `my-own-summary-is-not-a-source`,
  `gh-pr-checks-watch-not-a-poll-loop`
- PRs: #226 (this), #224 (superseded)
