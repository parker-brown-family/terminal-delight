# Brief — front-end redesign of the left bar

**For:** the incoming agent redesigning the left bar's front end.
**Base:** branch off `main` (@ `36317b5` or later). It is CI-green and carries the
left bar, the client-server split, and the hosted-mode hardening together — and it
is already installed and live (`td-a3cdfa5-main`), so you are refining a shipped
feature, not an unreleased one.

## Where the left bar came from
The mother bar (the horizontal tab strip) ran out of attentional space — a dozen
tabs on one row is a dozen titles competing for one glance, and the strip's answer
was to wrap onto a second row, which stacks the same problem. The left bar is the
*other axis*: a collapsible vertical tree, **PROJECT → INITIATIVE → TASK**, where
the tabs you are not working on today fold away behind the branch they belong to.
The initiative layer IS the existing tab group given a `project` field — not a new
concept beside it.

## The one invariant a redesign must not break
**Clicking a branch scopes the MOTHER BAR to it; the tree itself never narrows.**
That scoping is the entire point — a bar that merely mirrors the strip costs width
and buys nothing. When the bar is hidden the strip keeps a `⟩` handle where it was,
so the way back is discoverable without the chord.

## Code map (model / view split — keep it)
- **Model — `app/src/tree.rs`** (pure, 25 tests incl. shape sweeps): rows, roll-up,
  scope rules, drop landings. No gpui. This is where behaviour is tested; keep logic
  here, not in the view.
- **View — `app/src/main.rs`**: `render_left_bar` (11524), `bar_row` (10991),
  `branch_row` (11086), `task_row` (11286), `roll_badges` (10942 — the 🤖/✅/❌/📌
  roll-up glyphs), `left_bar_visible` (732). Width in `left_bar_w`.
- **Toggle**: `ctrl+shift+B`. `ToggleLeftBar` is emitted from `pane.rs:4314` (the
  focused terminal takes the keystroke first) and handled at `main.rs:2700`.

## Constraints
1. **Do not disturb the hosted-mode hardening.** `window_pid_path` (hostproto.rs +
   ctl.rs, the #355 MCP-relay spy fix) and the `repaired_at` divergence-guard
   cooldown (main.rs, #356) share `main.rs`/`host.rs` — leave them alone.
2. **i18n discipline.** Every translatable chrome string lives in `app/src/lang.rs`'s
   `Strings` struct; the compiler forces all **nine** languages (En/Es/De/Fr/Ru/Zh/
   Ja/Ko/Hi) to fill any new field. A row added in English alone is worse than no row.
3. **CI gates run more than build+test:** `cargo fmt -- --check` and
   `clippy --locked -D warnings` both block merge. Run them locally before pushing.

## Known touchups on deck
- **#361 — help-modal row for `ctrl+shift+B`.** The shortcuts view lists every other
  workspace chord but not the left bar's. Needs a new `Strings` field + all nine
  languages + one render row. This belongs to whoever owns this UI — i.e. you.
- **#364 — DONE.** Shipped and installed as `td-a3cdfa5-main`; the live fleet cut
  over 2026-09-10 (17 shells / 11 agents, zero panes lost). The left bar is live now.
- **#319 — the feature issue** can close as merged.

## Open visual touchups from Parker's screenshot
**TBD** — Parker flagged "some final touchups" against a screenshot the overseer
session could not see (the relay spy MCP was not bound). Fill this in from his
description before starting.

## How to look at it without touching anything
Screen capture from an agent shell is unavailable (grim hangs). Launch the release
binary with `XDG_CONFIG_HOME` pointed at a throwaway config and `TD_NO_SESSIOND=1`
to see the bar in isolation; verify a GUI change by what it writes back, then park
the window for a human eye.
