# Handoff — the round navigator, and whether Codex can ask one (2026-09-21)

## Status

**Landed.** Three PRs merged to `main`, gates green throughout, nothing of mine
uncommitted or unpushed.

| PR | Merge | What |
|---|---|---|
| [#634](https://github.com/parker-brown-family/terminal-delight/pull/634) | `5145830` | The navigator, auto-advance, and the `waiting_question` ordering fix |
| [#638](https://github.com/parker-brown-family/terminal-delight/pull/638) | `45dc332` | The steps redrawn in the response card's register vocabulary |
| [#647](https://github.com/parker-brown-family/terminal-delight/pull/647) | `35d299d` | The hindsight pass: a guard, a lying comment, a stale spec, a mute control |

## What's done

- **`surface::Round.current` and `surface::Step.id`**, filled per card by
  `channel::Round::surfaces`, so a card knows which step it is and how to reach
  its siblings. The screen-read path sets both `None` and does not guess — the
  picker marks its current step with a colour, which does not survive a
  character grid. *Verified:* `every_card_of_a_round_knows_which_step_it_is…`.
- **`round_progress` draws named, pressable tabs above the question**, in the
  register vocabulary (`emphasis::facet` `Active`/`Reading`, `sk.tpx(5.)`,
  `Step::Note`, underline — never a border). One deliberate divergence from a
  register: a tick on an answered step. *Verified:* the 12 `benchdraw` source
  guards, plus the new one below.
- **Auto-advance** in `bench_hook_press`, reading the freshly built cards rather
  than the bench's copy. A completed round moves nobody. *Verified:*
  `answering_a_step_marks_it_done_and_names_the_next_one_still_open`.
- **`waiting_question` prefers the selection**, then resolves a round to its
  **first open step**. It used to take the last arrival — correct while questions
  arrived singly, wrong for a round, which opened a round of two on question two
  with question one unanswered. *Verified:*
  `a_round_opens_on_its_first_open_step_not_its_last_arrival`.
- **`a_round_step_is_drawn_the_way_a_register_tab_is`** — mutation-proved:
  behavioural tests stay green under a reverted bordered chip, and this one fails
  by name. **Its first mutation test passed** because the `perl` replaced the
  first of two identical lines and landed in another function; the doc comment
  records that, because it is the part a future reader needs.
- **Codex established**, and the spec corrected to say so (§8).

## How to run/verify

```bash
cd ~/Work/terminal-delight/app && cargo test --locked --bin terminal-delight
```
```bash
cd ~/Work/terminal-delight/app && cargo clippy --locked -- -D warnings && cargo fmt --check
```

**Cross-check the test COUNT, not the exit code** (1463 on this tree). A shared
`CARGO_TARGET_DIR` serves another worktree's binary and reported 1421 passing with
an entire module absent — see `a-shared-target-dir-serves-a-stale-test-binary`.

```bash
python3 ~/Work/terminal-delight/reports/_askhook.py     # what a Claude hook was handed
```
```bash
python3 ~/Work/terminal-delight/reports/_codexask.py    # the same for Codex; control decides
```
```bash
python3 ~/Work/terminal-delight/reports/_askscan.py     # is a pending ask ever on disk?
```

Both readers use `~/.lean-ctx/context_radar.jsonl`, a **ring buffer that rotates
in minutes**. Zero records is the expected steady state and is not a refutation.

## Not done / next

1. **Widen Codex's `PreToolUse` matcher** — the blocker, and it needs Parker.
   Editing `~/.codex/hooks.json` re-prompts Codex to *trust its hooks* (running
   them outside its sandbox). That is a permission grant, not a config edit, and
   answering it by keystroke on his behalf was refused.
2. **`td-agent-hooks` should match `request_user_input`** alongside
   `AskUserQuestion`.
3. **Key the answer map by question `id`** where one exists — Codex keys by id
   (`{"colour":{"answers":["Red …"]}}`), Claude by question text.

**Do not build 2 and 3 first.** Without 1 no question arrives, so both would be
untested code on a path nothing can reach.

- **#648 — nothing has ever rendered this navigator.** Three merged PRs, no image.
  The only claim in this work with no measurement behind it.
- **`brown-family-sports#23`** — `mcp-health.sh` emits plain text where Codex
  requires the JSON envelope, so every Codex prompt fails a `UserPromptSubmit`
  hook. Unrelated, long-standing, invisible because Claude accepts bare stdout.

## Watch out

- **Codex's `PreToolUse` payload has never been captured.** That it carries
  `tool_input` is inferred from Codex using Claude's event names and envelope
  elsewhere. Everything in item 1 rests on it.
- **`codex exec` is not a substitute for a TTY** — `request_user_input` is refused
  there and `PreToolUse` does not fire. `Stop` **does**, so the note in
  `~/.codex/hooks.json` claiming exec fires "no hooks at all" is too strong.
- **`[tools.experimental_request_user_input]` was left enabled** in
  `~/.codex/config.toml` (a struct, not a boolean), commented, backups beside it.
  It is what makes the capability exist; deleting the block reverts it.
- The tool is **Plan mode only**; `default_mode_request_user_input` reports
  *under development* at OpenAI.
- **`paneident`'s two tests are environment-dependent, not broken.** Three
  environments gave three answers on one commit: 2 failures here, 0 in another
  worktree, green in CI. See #592/#605.
- **Another agent owns `~/Work/td-retro`** (`bench/the-selection-rules-have-one-door`).
  I worked there briefly by accident and `cargo fmt` ran in their tree; their file
  is fmt-clean now. Their `Line::seek`→`place` refactor is theirs to land.

## Where it's recorded

- **APES episode:** `…/apes/projects/terminal-delight/episodes/2026-09-21-the-jam-arrived-by-a-different-road.md`
- **Harvest:** `handoffs/2026-09-21-round-navigator-and-codex.cdx`
- **Briefs:** `reports/2026-09-21-multiple-questions-jam-the-bench.html`,
  `reports/2026-09-21-can-codex-ask-a-round.html`
- **Issues:** #630 (Codex adapter, carries the measurements), #648 (never
  rendered), `brown-family-sports#23`; #619 closed.
- **lean-ctx:** session decision + three knowledge facts.
