# Handoff — chrome legibility + the theme UX pass (2026-09-19)

## Status

**Landed.** Both pull requests merged into `main`:

- **#555** — the chrome fix (`main @ c63d95b`). Picked up a fifth commit from
  another agent on its way through: panel insets, 42 `faint`-as-text repairs,
  and the APPROVE bloom dialled back.
- **#562** — the measured pass over all six palettes + its report.

Work was done in the worktree **`~/Work/td-phosphor`**, not the shared
`~/Work/terminal-delight` (which concurrent agents keep switching branches on).
That worktree is clean, on `chrome/theme-ux-pass`, fully pushed, and its HEAD is
an ancestor of `origin/main`.

Installed build: `~/.local/lib/terminal-delight/td-c63d95b-glow`, launcher
repointed. **The symlink is contested** — it moved twice during this session.

## What's done

| change | verified by |
|---|---|
| `Skin::ring` — crisp spread-ring deleted, `glow_a` 0.41 → 0.11 (deco 0.45 → 0.13) | `skin --theme hacker` reports `glow_a: 0.11` out of the shipped binary |
| `Rest::{Bare,Face}` — rows stay bare, tabs keep a quiet bordered face | before/after photographs of all four surfaces |
| `ink_lit` / `edge_rest` / `face_rest` — the model-effort strip's own numbers, as tokens | `a_tab_label_is_legible_on_every_builtin_palette`, run red against the old inks first |
| `Skin::slider` + `slider_half` — TERM ⇄ BENCH is one bordered track | photographed |
| the measured pass + report | `terminal-delight skin --theme <id>`, six palettes × three skins |

Numbers that matter: resting tab labels went **1.32–1.62:1 → 6.2–7.4:1** on every
palette; the lit label on `quiet-command` **1.48 → 13.57**. 1.0 is two identical
colours.

## How to run / verify

```bash
cd /home/parker/Work/td-phosphor/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```
```bash
./app/target/release/terminal-delight skin --theme quiet-command | grep -E '"ink_lit"|"edge_rest"|"face_rest"|"glow_a"'
```
```bash
readlink -f ~/.local/bin/terminal-delight
```

Photograph the chrome under every palette without stealing the focused screen —
the rig is a scratch script, worth promoting beside `scripts/workbench-smoke.sh`:
it launches a seeded bench per theme, parks it silently on the unfocused monitor
with `movetoworkspacesilent`, and grabs the window's own rect.

## Not done / next

Three follow-ups, each a GitHub issue with the `follow-up` label and an APES
kanban mirror:

- **#598 — blocked on Parker.** *Is a clickable glyph a mark or a word?* The tab
  strip's close `×` is `th.faint` at ~1.2:1. The answer decides whether the
  `text_color` gate is a straight ban or needs per-site opt-outs. Writing the
  gate stopped here on purpose: a check that cries wolf gets switched off.
- **#599 — ready.** The marking is dark-calibrated. `select` derives at a fixed
  lightness of 0.76 and measures 1.39:1 on `quiet-command`; `ok` 2.02, `warn`
  1.90. The words hold on every palette now; the thing saying which tab is lit
  does not.
- **#600 — ready.** The eleven desktop palettes (tide, retro, last horizon…)
  arrive through the paint/dynamic path, so `skin --theme` cannot reach them and
  no in-repo test gates them. Either make them reachable or prove
  `apply_dynamic` cannot lower a passing base palette.

## Watch out

- **The symlink, not the build.** If a new window looks like the old chrome, run
  the absolute binary path before concluding anything. Nine minutes was the
  record for a cutover surviving this session.
- **Never use contrast ratio to ask "are these the same colour."** It is
  luminance only and put `mark`/`select` at 1.10:1 when they are 107 ΔE apart.
  Use CIE ΔE for collision, WCAG contrast for legibility.
- **`~/Work/terminal-delight` belongs to other agents right now** — another
  branch, uncommitted `app/src/tree.rs`. Nothing here touched it.
- The new register tab strip (#554) adds `border_b_1` in the tint **on top of**
  the ring's full border — two devices on one tab, which the skin's own doc
  argues against. Left alone; not mine to change inside a merge.
- **This handoff and the `.cdx` beside it are untracked.** Tie-off records, it
  does not ship.

## Where it's recorded

- APES episode — `projects/terminal-delight/episodes/2026-09-19-marks-not-words.md`
- APES kanban — one closed ticket with a deliverable, three follow-ups (one blocked, two todo)
- lean-ctx — `ctx_session(decision)`; `ctx_knowledge` was **not bound this session**
- file-memory — `the-skin-verb-is-a-headless-instrument`, `contrast-is-luminance-only`, `dont-poll-a-background-job`
- session harvest — `handoffs/2026-09-19-chrome-legibility-and-theme-pass.cdx`
- reports — `reports/2026-09-18-figure-and-ground.html`, `reports/2026-09-18-marks-not-words.html`
