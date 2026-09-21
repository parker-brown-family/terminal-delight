# Handoff — the overview shows your message (2026-09-19)

## Status

Landed on the branch, not on main. `4b6d583` on `bench/defaults-and-dials`,
pushed, folded into open pull request **#560** as its fifth section. A
concurrent session has since committed `f98c0e6` on the same branch; my commit
is an ancestor of the remote head (checked with `git merge-base --is-ancestor`).
**#560 is `DIRTY` against main and needs a merge before it can land — that
predates this commit.**

## What's done

The bench's OVERVIEW draws the person's own last message as its own indented
block above the reply standing in the room — `th.human` ink, pinned above the
card and outside its scroll.

| Piece | Where | What it decides |
|---|---|---|
| `ask_lines(shelf, stand_in, agent, how)` | `app/src/workbench.rs` | whether the block is drawn and how many lines: Overview shelf, stand-in only, agent pane, `Full` 4 / `Compact` 2 / `Summary` none |
| `Bench::standing_in()` | `app/src/workbench.rs` | the overview's stand-in vs a card opened off the rail — the gate that matters |
| `ask_clipped(lines, keep)` | `app/src/workbench.rs` | reads one line more than is drawn, ends a clipped message in an ellipsis |
| `benchdraw::asked(lines, …)` | `app/src/benchdraw.rs` | the block: `YOU`, the text, `.ml(px(18.))`, and a sentence when the message has left the scrollback |
| placement | `app/src/pane/bench.rs` | above the body scroll, after the agent strip |

Text comes from the existing `Pane::last_human_message`, so a turn typed at the
TERMINAL face counts the same as one sent from the composer, and there is no
second copy of the conversation.

Verified: 1,310 tests pass, two of them new
(`the_overview_captions_the_standing_reply_with_what_you_asked`,
`a_message_that_ran_on_says_it_ran_on`); `cargo fmt --check` clean; the clippy
gate CI runs is clean; photographed end to end on a throwaway session, twice
(before and after the indent Parker asked for).

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --bin terminal-delight ask_
```
```bash
cd /home/parker/Work/terminal-delight/app && cargo clippy --locked -- -D warnings
```

To SEE it, the rig is not `workbench-smoke.sh` (shells only — see
`terminal-delight#586`). Write a script named `claude` that writes a response
surface into `$XDG_STATE_HOME/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID/`,
prints a line behind `❯`, then sleeps. Then, each as its own single command:

```bash
setsid env TD_SESSION=wbask TD_MCP_WRITE=1 app/target/release/terminal-delight > /tmp/win.log 2>&1 < /dev/null & disown
```
```bash
app/target/release/terminal-delight ctl --pid <PID> adopt '{"cwd":"/tmp","run":"/tmp/fakebin/claude"}'
```
```bash
app/target/release/terminal-delight ctl --pid <PID> bench on
```
```bash
grim -g "<x>,<y> <w>x<h>" /tmp/bench.png
```

## Not done / next

- **#560 needs a merge with main** before it can go in.
- **`terminal-delight#585`** — an unsent draft in the input box can be drawn as
  the person's message (the agent wall's card has the same behaviour, older).
- **`terminal-delight#586`** — `scripts/fake-agent`, so the smoke rig can stage
  an agent pane instead of every session re-deriving one.
- **Captioning an OLDER reply** is deliberately not built: it needs the window
  to remember each turn's ask rather than reading the latest one.

## Watch out

- **Shared worktree.** `~/Work/terminal-delight` is worked by several sessions
  at once; HEAD moved under me mid-tie-off. Check `git log` before assuming your
  commit is the tip.
- **`cargo clippy --all-targets` is red** on ten pre-existing
  `assertions_on_constants` in `main.rs` test code. CI does not lint tests, so
  this is not the gate failing — do not report it as one.
- **A `setsid … &` sharing a Bash call with other statements exits 144** with no
  window on this box. Launch it alone, with `disown`.

## Where it's recorded

- APES episode:
  `~/BROWN-FAMILY-SPORTS/Software/apes/projects/terminal-delight/episodes/2026-09-19-the-overview-had-only-half-the-conversation.md`
  (with the `.cdx` harvest beside it)
- APES kanban: ticket closed with deliverable; two follow-up tickets opened and
  cross-linked to #585 / #586
- lean-ctx: `ctx_session decision` breadcrumb
- file-memory: `a-shell-pane-cannot-photograph-an-agent-feature`,
  `the-clippy-gate-does-not-lint-tests`
- Pull request: #560, section 5
