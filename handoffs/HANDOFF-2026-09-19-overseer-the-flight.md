# Handoff — the overseer pass and the self-generating test doc (2026-09-19)

## Status

**Landed and pushed.** The board went from seven open pull requests to zero
during the pass. At the moment that pass closed, `main` was `569f452` and the
window, the installed binary and `main` were identical trees.

**It did not stay that way, and that is the normal state here.** Before this
handoff was finished `#582` had merged (`main` → `4e3a1d7`), another agent had
installed `td-4e3a1d7-fold`, and a new pull request was open. Do not read the
shas above as current — read them as *what was true when the pass closed*, and
run `bash reports/_audit_clean.sh` for now.

Nothing of this session's is uncommitted. Four local branches from it are all
fully contained in `main` and were left in place rather than deleted.

## What's done

| Landed | Verified by |
|---|---|
| #549 #553–#556 #558–#562 #566 #571–#576 | merged, each `git merge-base --is-ancestor` checked rather than trusting `gh pr merge` |
| **#567 fixed** — `present_surface` now reaches the disk | two tests; the origin-precision guard proven red with the rule removed |
| **#573** — dropped `194×50` and the `·2` doubt badge | Parker's own two reports |
| **#571** — folded the file drop + launch defaults past the changelog | 1368 tests, clippy `-D warnings`, fmt |
| **#580** — swept 19 loose files out of the shared worktree | checked for credentials and size first |

Two things nobody had noticed, found while merging:

- **`main` had `composer`'s doc comment attached to `note_box`** — #566 inserted
  a function between a doc block and the function it documented, so `composer`
  had no doc at all. Repaired in #571 by moving `note_box` below `composer`.
- **A semantic conflict neither branch had alone** — #566 added a test asserting
  every bench corner goes through the skin; the file drop's hover pill was
  rounded with a bare `px(3.)`. The test was right.

## How to run/verify

```
python3 reports/_assemble_rodeo.py        # regenerate the live-test checklist
bash    reports/_audit_clean.sh           # is anything loose? branches, worktrees, binaries
python3 reports/_pane_peer_join.py        # pane pid -> SendMessage peer name, whole window
cd app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

The checklist is **generated, not written**. Edit `reports/_rodeo_items.json` to
add or improve a test step; never edit the HTML, it is overwritten. A pull
request with no entry there still appears, labelled auto-derived.

## Not done / next

- **#587 — `CHANGELOG.md` is the only file anything conflicts on.** Nine
  collisions in one day, none of them a real disagreement.
  `reports/_changelog_union.py` is the stopgap and should be deleted by the fix.
- **#588 — five orphan branches** hold real unlanded work with no pull request
  and no owner, three days to three weeks old. `sticky-note` is the likely
  `invalid`.
- Surfaced, not actioned, because they are keep-or-kill calls: **1.3 GB** of
  prunable installed binaries (34 of them), four dirty worktrees on branches
  119–373 behind, one stash from 2026-09-15, thirty-two dead `ctl-*.sock`.

## Watch out

- **The session host is a THIRD build and almost nothing reports it.**
  `serve --session` is systemd-parented, owns every PTY, and only upgrades by
  dying — it was **76 merges behind** while the window was byte-identical to
  main. `pgrep -f 'terminal-delight serve'` **finds nothing**, because the
  binary is `td-<sha>-<label>`; the `.*` matters. There is one host per session
  (`1`, `tdclip`, `attention` were three different builds), so never take the
  first hit. `_assemble_rodeo.py` now lists all three.
- **Peer names are not unique.** `terminal-delight-81` resolved to two different
  panes. Resolve before messaging; fall back to `leave_note(pid)`, which cannot
  be ambiguous.
- **The shared worktree** `~/Work/terminal-delight` is on `bench/defaults-and-dials`,
  50 behind main, with untracked duplicates of what #580 committed. Several
  agents share it — never `git add -A` there.
- **A mutation harness needs three outcomes, not two.** Mine reported three good
  guards as useless because a mutation that fails to *compile* also prints no
  `test result: FAILED`.

## Where it's recorded

- **Episode:** `apes/projects/terminal-delight/episodes/2026-09-19-the-overseer-and-the-checklist-that-writes-itself.md`
- **Harvest:** `handoffs/2026-09-19-overseer-the-flight.cdx` (390K, 7 facts, 1116 secrets redacted)
- **APES kanban:** `stop-changelog-md-being-the-only-file-anything-conflicts-on-mu81jwyy`,
  `disposition-the-five-orphan-branches-…-mu81jztj` — both mirrored to #587 / #588
- **lean-ctx:** session decision recorded (`ctx_knowledge` was not bound this session)
- **file-memory:** `the-window-and-the-host-can-be-different-builds`,
  `a-mutation-that-does-not-apply-reads-as-a-dead-guard`,
  `pids-and-peer-names-do-not-join`, `a-durable-round-trip-is-three-invariants`,
  `launching-a-td-instance-is-always-allowed`, `raf-does-not-fire-in-a-backgrounded-tab`
