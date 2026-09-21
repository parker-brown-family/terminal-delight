# Slices: the workbench follows the agent — store half

Built on `bench/conversation-store` in `~/Work/td-bench-store`, cut from
`origin/main` at `4b1fb2b`. Share the primary's target dir or a full gpui build
is cold:

```bash
CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo test --bin terminal-delight
```

Gates CI actually runs, and the only ones that count:

```bash
CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo clippy --locked -- -D warnings
```
```bash
cargo fmt --check
```

## The slices, in build order

| # | Slice | Ends in |
|---|---|---|
| **1** | **Tracer bullet — an agent leaving clears its bench.** `set_mode`'s departed edge calls `Bench::clear_surfaces`. No store, no sentinel, no key. | A pane whose agent ends shows an empty rail and offers `LAUNCH AGENT`. Visible in one gesture, on the running build. |
| 2 | `ConvKey` and `benchstore` — `dir()`, `file()`, `ask()`, `load()`, `asks()`. Pure std, scratch-directory tests. Nothing calls it yet. | The store is correct in isolation, including the compaction case. |
| 3 | The key resolver — `tenancy_for` first, `paneident::certain` second, neither means no filing. | `two_agents_under_one_shell_do_not_share_a_bench` passes. |
| 4 | The sentinel and the drain, together. They are one fact about the inbox and separating them ships a half-state where filed copies linger. | A restart classifies correctly and the inbox drains. |
| 5 | Wire it: `present` files then drains; the arrived edge loads; `Bench` carries the key. Shell panes untouched. | End to end on a real pane. |
| 6 | Retention for `conversations/`, keyed on the conversation's own last activity. | `prune` has a sibling that cannot delete a live conversation. |

Slice 1 is the tracer because it is the whole gesture, needs nothing new on
disk, and can be driven and photographed on the build that is running now.

## Held, and why

Screens E, F and G and the "11 surfaces · bring it back" line are drawn by a
bench that does not yet have a place to draw them — `docs/plans/workbench-drives-the-agent/`
is at Gate 1 and unapproved. They are designed, approved at Gate 1 here, and
specified. They land with that plan rather than ahead of it.

The ledger reader (`tenancy_for` / `tenancy_of`) is the other pane's, built on
`bench/ledger-tenancy`. Slice 3 consumes it; it is not rebuilt here.

## One task, one pull request

Two panes have planned this feature and a third owns the architecture it sits
inside. Whatever lands does so as **one** pull request.
