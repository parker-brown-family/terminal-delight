# Handoff — per-agent tab badges + the stuck HEY blinker (2026-09-01)

## Status

**Landed and deployed, one question still open.**

- #238 (per-agent badges + the anchored blinker) → `ab9e750`
- #239 (a bare BEL is no longer a finished turn) → `f3484fc`

`origin/main` = `f3484fc`, deployed to `~/.local/lib/terminal-delight/td-f3484fc`
with `~/.local/bin/terminal-delight` repointed. Symbol-probed: the new binary
carries `agent_badge` / `tab_agent_badges` / `prompt_sentence`; `td-2a8661e`
carries none.

**Nothing is running it yet.** At handoff: 11 windows on `td-2a8661e`, 2 on
`td-ab9e750`, 1 on `td-e82dd33`, 0 on `td-f3484fc`.

## What's done

**1. The HEY blinker no longer pins on (`app/src/pane.rs`).**
Parker's hands-on verdict on #231 came back failed: the blinker stayed lit after
he answered. The cause was the trigger, not the animation. `row_wants_human`
tested `t.contains("do you want to")`, and Claude Code's own sign-off — *"What do
you want to work on?"* — contains it. Since `needs_input` is recomputed every
120ms over `recent_lines(14)` and a **finished** reply never scrolls away, the
row never left the window.

Question forms are now anchored: the phrase must open the row and the row must
end in `?`, after `prompt_sentence` strips the U+2500..257F box drawing a real
dialog is framed in. `>` / `❯` are deliberately **not** stripped, so a line Parker
typed can never read as the CLI asking him something. Footers ("enter to select",
"esc to cancel") stay substring matches — the CLI prints those only under a live
picker.

*Verified:* the regression cases (taken verbatim from the failing screenshot) are
pinned in `interaction_prompts_are_detected_and_working_footers_are_not`; that
test was also run alone to confirm it isn't a phantom green.

**2. One badge per agent, not one per tab (`app/src/main.rs`).**
`tab_has_working_agent` / `tab_needs_input` / `tab_has_bell` / `tab_bell_blocked`
— four `any()` folds — collapsed into `tab_agent_badges(i)`, which walks the tab's
leaves in tree order and returns a badge **per pane**. Pure `agent_badge()` picks
the loudest state (NeedsInput > Working > Done/Blocked; `None` for an agent with
nothing to say). Idle agents get no badge; the strip caps at `MAX_TAB_BADGES = 4`
with a faint `+N`, tied to `MAX_PANES` by `const _: () = assert!(…)`. Animation
keys carry `(tab, slot)` — a shared key makes gpui drive every badge off one
animation.

*Verified:* 411 tests (409 at session start; +2 — the precedence table and the
overflow arithmetic), `cargo clippy --all-targets` clean, `cargo fmt` applied, and
`git diff -U0 | grep ^@@` confirmed only this session's hunks (no stray reformat).

**3. Incidental:** removed an orphaned doc comment above `pulse_alpha` that
described a function no longer in the file.

## How to run / verify

```bash
cd ~/Work/terminal-delight/app && cargo test
```
```bash
cd ~/Work/terminal-delight/app && cargo clippy --all-targets
```
```bash
cd ~/Work/terminal-delight && git diff -U0 app/src/main.rs | grep "^@@"
```

`cargo test` exceeds the 110s `ctx_shell` foreground cap and auto-detaches — pass a
larger `timeout_ms` and let it block rather than polling `background_action`.

## Not done / next

- **Run the #240 discriminating test.** Relaunch onto `td-f3484fc` (confirm with
  `readlink /proc/<pid>/exe`) and open a new window that restores resumed agent
  panes. Burst gone ⇒ #239 covered it, close #240 `invalid`. Burst persists ⇒ the
  cause is the bare `"interrupt)"` substring in `agent_is_thinking`
  (`app/src/pane.rs:2568`), whose spurious match sets `think_since` and so
  *satisfies* #239's guard rather than tripping it.
- **The 4-badge strip is still visually unverified.** No way to render a
  synthetic agent state ([#236](https://github.com/parker-brown-family/terminal-delight/issues/236)),
  which now blocks verifying #239 and #240 too. Highest-leverage item here.
- [context-delight#10](https://github.com/parker-brown-family/context-delight/issues/10)
  — `cdx-audit` counts native `Read`/`Edit`/`Write` as `ctx_*`-substitutable.

## Watch out

- **Don't tighten `wants_human` without evidence.** A false "come interact" cries
  wolf; a **miss** silently kills the feature, which is worse. Strictness moves
  only on a real transcript, never on a hypothetical.
- **`needs_input` is not latched and needs no ack.** If it looks stuck, the
  predicate is matching something still on screen — read the pane's last 14 rows
  before touching the ack paths. #232's ack work is a red herring here.
- The repo is a **shared worktree**; other agents commit into it. Check
  `git status` before building, and never `git add -A`.
- Green tests have now been necessary-but-not-sufficient for three consecutive
  mother-bar changes. Treat a renderer change as unverified until someone looks.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-01-per-agent-tab-badges.md`
- Harvest: `apes/projects/terminal-delight/episodes/2026-09-01-per-agent-tab-badges.cdx`
- APES kanban: `…-mtj4tzjt` (#236), `…-mtj4u8t4` (#237), context-delight `…-mtj4ubgc` (#10)
- lean-ctx: session decision recorded (resume breadcrumb)
- file-memory: `a-live-predicate-over-a-still-screen-is-a-latch`,
  `a-counter-example-test-only-protects-the-axis-it-varies`
- Predecessor episode: `episodes/2026-09-01-tabs-learn-to-talk.md`
