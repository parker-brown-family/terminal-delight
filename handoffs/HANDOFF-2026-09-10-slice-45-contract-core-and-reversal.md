# Handoff — slice 4.5 contract core, its reversal, and the staged flip (2026-09-10)

## Status

Landed and pushed. Branch `client-server-split` at **`455cfcc`**, PR **#328** still draft. Suite **724 green across four consecutive runs**, `cargo fmt --check` clean, **nothing installed**.

**One thing is deliberately uncommitted**: the default flip to hosted, staged in `app/src/main.rs` in the worktree `~/Work/td-cs-contract` (branch `cs/contract-reversal`, otherwise identical to the branch tip). It is Parker's to sign on the measured numbers. Nobody else should commit it, and if that worktree is pruned the change goes with it.

## What's done

**Six commits, then three that reverse two of them.** In order:

| Commit | What | Verified by |
|---|---|---|
| `b6f5fdc` | window steal — *reversed by `8cd38b9`* | test failed against its parent |
| `5d8c7ee` | hello gate — *reversed by `a87187a`* | test failed against its parent |
| `a5594a3` | version-break: shutdown checkpoints, `HostProbe::Skewed`, `stand_down_at`, tier-3 routing | unit + fake-host tests, both parent-failing |
| `114e160` | bounds: 64 live panes refused by name, 16 most-recent corpses kept | both parent-failing (cap let a 65th through; sweep removed → 22 dead held) |
| `bf167fa` | barrier in the persistence race test | 10/10 fail with the writer lock removed; 10/10 pass with it |
| `a87187a` | hello optional again; peer-uid check is the boundary | inverted test fails against `9bd92bb` |
| `8cd38b9` | window steal deleted; two windows share panes per-pane | new test fails against `9bd92bb` |
| `8097190` | binding docs reconciled (protocol page, Gate 2 amendment, Gate 3 banner, status) | — |
| `455cfcc` | flip-gate numbers re-measured and recorded | the runs below |

**Flip gate, re-measured after the rework:**

- Echo bench, realistic 8-pane flood: **30µs** p99 attached-over-local (62→92µs), **0/1000** over 1ms. Quiet: 39µs, 0/1000. Saturating: 2741µs, 42/1000 — reported, not gated.
- Survival `gui-kill --cycles 20`: **20/20, `"losses":0,"failures":[]`**.
- Both written into `docs/plans/client-server/02-architecture.md` under "Re-measured 2026-09-10".

**Trackers:** #350 and #351 closed against the resolving commit; #353 decided (attach stays open to any connection — a same-uid check is theatre) and closed; **#354 filed** (the product doc's close section describes the post-flip iteration).

## How to run/verify

```bash
cd /home/parker/Work/td-cs-contract/app && cargo test
```
```bash
cd /home/parker/Work/td-cs-contract && ./scripts/td-echo-bench.sh
```
```bash
cd /home/parker/Work/td-cs-contract && ./scripts/td-survival-test.sh gui-kill --cycles 20
```

The survival leg **opens a real window per cycle on the display and takes focus** — announce before running it. Run it so its output streams; a wrapper that only redirects into a log gets reaped as idle and looks exactly like a stall at cycle 16 (it did, twice).

To see the staged flip: `git -C /home/parker/Work/td-cs-contract diff app/src/main.rs`.

## Not done / next

- **The flip is unsigned.** One change, one file, tested predicate, `TD_SESSIOND=0` opt-out. Parker's call on the numbers above.
- **The eyeball half of the gate is unrun** — "no visible lag at eight panes" is a person's judgement, not a number, and it is the one gate condition without evidence.
- **#354** — `01-product.md`'s close section describes deferred close, which the re-cut moved past the flip. The status file says which build is which; the product doc is Parker's to amend.
- Close-undo (held state, ctrl+shift+z) is the first post-flip iteration; the anchor check recommends an in-memory v1. Another agent has `cs/close-undo` open in `~/Work/td-cs-undo`.
- The floor-control survival leg was not re-run — it measures the serverless path this rework did not touch.

## Watch out

- **`ClientKind` gates nothing now.** It is descriptive. Anything that reads it as permission is a bug.
- **Pane-level supersede is not window steal** and must survive: the attachment `serial`, and a stream that ends when another client takes that pane, is what lets a window ask "ended or taken?" instead of reaping a live pane.
- **The wire is stateless on purpose.** A hello gate was built here and reversed within hours; the argument and its counter are in `02-architecture.md`'s amendment and the protocol page. Do not rebuild it without reading both.
- Six worktrees on this branch's family; `~/Work/td-cs-contract` is mine, `~/Work/td-cs-undo` is the close-undo agent's. Check `git worktree list` before assuming.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-10-slice-45-contract-core-and-reversal.md`
- APES kanban: `reverse-the-hello-gate-and-window-steal-and-re-measure-the-flip-gate-mtvuwt8c` (done), `say-which-build-01-product-md-s-close-section-describes-…` (backlog, mirrors #354)
- Session harvest: `handoffs/2026-09-10-slice-45-contract-core-and-reversal.cdx`
- file-memory: `client-server-split.md`, `check-the-rig-before-the-subject.md`
- PR #328
