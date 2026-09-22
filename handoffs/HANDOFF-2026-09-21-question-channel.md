# Handoff — the workbench question channel (2026-09-21)

## Status

**Landed and installed.** Six pull requests merged — #660, #666, #669, #671,
#673, #676 — all nine commits verified present in `main` at tie-off. The crash
fix reached the launcher and was confirmed by ancestry, not assumed, after
another agent cut past the install. Nothing uncommitted; the worktree this was
built in (`~/Work/td-round`) has since been removed by another session, which is
fine — everything is in `main`.

## What's done

| PR | What | How it was verified |
|---|---|---|
| #660 | A round has exactly one ending; the badge holds through keystrokes; a folded screen reading retires its duplicate; one Submit on a one-question multi-select | 7 mutations, all caught |
| #666 | A round the agent walked away from ends, and says it does not know how | 4 mutations caught; rule falsified against 11 rounds in 9 panes first |
| #669 | Every pressable bench chip lights under the pointer | 2 of 3 caught; the third recorded as uncatchable |
| #671 | The review takes over the workbench instead of floating out of it | 3 mutations caught |
| #673 | A question opens where it will stay; nothing on the bench blocks the mouse; REVIEW ANSWERS survives being answered | 5 of 6 caught; the sixth is a call site a pure test cannot see |
| #676 | The body that is drawn is the only body that is built — the crash | gate asserts the shape |

Around twenty mutations across the session. **Three came back NOT CAUGHT and are
recorded as such** rather than omitted: the `ack_needs_input` wiring, the hover's
lit predicate, and `body_anchor`'s call site — each needs a live `TerminalView`
and this crate has no gpui test app.

## How to run / verify

```bash
cd ~/Work/terminal-delight/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

Two tests are red on every dev box here and green in CI — `paneident::tests::a_ledger_entry_binds_a_pane_in_a_crowd` and `a_real_child_named_like_an_agent_is_found_under_its_parent`. They read the machine's real process tree. Already filed as #605 and #592. **Anything else red is yours.**

To check what is actually running, read the process, never the symlink:

```bash
readlink /proc/$(pgrep -f 'bin/terminal-delight$' | head -1)/exe
```

## Not done / next

- **#661** — the `.ws` conversation record has no closing line for a refused round. APES ticket `write-a-closing-record-for-a-refused-question-round-see-terminal-delight-661-mucyzfrf`. Do **not** write it as `Rec::Answered` with a null map; that is the same collapse in a different place.
- **#663** — a wrapped question read off the screen alone is titled with its last row. APES ticket `title-a-screen-derived-question-card-with-the-whole-question-not-its-last-row-se-mucyzj0y`. Naively joining rows upward swallows the picker's header chip and any agent prose above it, which is why `6fd261d` fixed the matching rather than the reading.

## Watch out

- **The launcher is contested.** It was repointed by other agents three times during this session, twice putting the fleet behind `main`. The symlink records who touched it last, not what is newest.
- **`.occlude()` is banned on the bench** and there is a gate saying so. It is `HitboxBehavior::BlockMouse`, and the bench's controls are rectangles resolved by one listener on the pane root — occluding severs every control inside it from the only thing that can read it. That single line was three separate bug reports.
- **`benchdraw::sel` registers at CONSTRUCTION.** Never build a body you might discard; make the alternatives arms of one match. Another agent has since added a third arm and kept the shape.
- **A source gate that matches a spelling** will fail on a change that never broke its rule. The `only_the_body_that_is_drawn_is_built` gate was rewritten to assert the match's scrutinee for exactly this reason — read it before adding another.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-21-one-question-one-card.md`
- Session harvest: `handoffs/2026-09-21-the-question-channel.cdx`
- lean-ctx: `ctx_session` decision breadcrumb (this session)
- file-memory: `a-rejected-tool-fires-no-posttooluse`, `a-rendered-row-is-not-the-source-string`, `a-correct-handler-is-not-a-delivered-event`, `do-not-construct-what-you-will-not-draw`, `assert-the-property-not-the-spelling`
