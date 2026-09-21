# Handoff — the workbench groupings, and three cutovers before them (2026-09-18)

## Status

**Landed and merged.** Four commits across three pull requests, all ancestors of
`origin/main` (verified with `git merge-base --is-ancestor`):

| PR | Commits | What |
|---|---|---|
| [543](https://github.com/parker-brown-family/terminal-delight/pull/543) | `14fce42`, `25bcf67` | ctrl+a selects the draft; the bench strip becomes three buttons; the tab strip follows the scope chip |
| [545](https://github.com/parker-brown-family/terminal-delight/pull/545) | `6998c0f` | A window opens on the whole session; a stranding scope is refused twice |
| [554](https://github.com/parker-brown-family/terminal-delight/pull/554) | `a2bb520` | The response card is group tabs over a chip row over one body |

Built, installed and cut over three times during the session. **The launcher has
since moved on** — `~/.local/bin/terminal-delight` now points at another agent's
build, which is the normal state of this repo on a busy evening. Nothing to undo.

Working branch `cutover/666834a` in `~/Work/td-award` is a throwaway pointer at
the merge commit; the worktree can be removed whenever.

## What's done

- **`ctrl+a` selects the whole draft** in the bench composer. `Edit::SelectAll` +
  a `marked` flag on `Line`; the `\x01` still reaches the agent (putting its caret
  at column zero) and a replacing keystroke is prefixed with
  `workbench::replace_bytes()` — one `ctrl+k` — so both sides lose the same text.
  *Verified:* unit tests, and photographed selecting and replacing on a real GPU
  render.
- **The bench strip is `CLAUDE · HIGH · END SESSION`**, three bordered
  `strip_face` buttons. Values resolve dial → the pane's own launch command
  (`--model`, `--effort`, `model_reasoning_effort`) → the harness; chosen values
  draw as text, inferred ones faint. `Dial::unknown` and the stop emoji are gone.
  *Verified:* photographed on an agent pane.
- **The mother bar's strip follows the scope chip** (`tree::shown`), the scope is
  no longer persisted, restore calls `ensure_scope_shows`, and `shown()` falls
  back to every tab rather than answering empty. *Verified:* reproduced the empty
  strip on the installed build with a hand-written state file, then photographed
  the same file drawing four tabs on the fix.
- **The response card is tabbed.** `surface::Group::of(Register)` (pure, total,
  table-tested, derived not sent), `workbench::tabbed()` as the single bucketing
  authority, `resolve_tab` / `resolve_leaf`, `draws_strip` / `draws_chips`,
  `Hit::PickTab` + `Hit::PickRegister`, selection state as two maps forgotten on
  retire. `emphasis::shelf()` deleted with its argument. *Verified:* 1294 tests,
  clippy `-D warnings`, rustfmt, and two photographs.

## How to run / verify

```bash
cd /home/parker/Work/td-award/app && cargo test --locked && cargo clippy --locked -- -D warnings && cargo fmt --check
```
```bash
bash /tmp/claude-1000/-home-parker-Work-terminal-delight/e0accca2-d7cf-428c-ba9e-8ca8aee2abf6/scratchpad/verify2.sh
```

The second stages a throwaway session, makes its pane an agent pane with a copy
of `sleep` named `claude` (no tokens spent), puts the bench up, types a draft,
sends ctrl+a and a replacing keystroke, and photographs each step.

## Not done / next

- **[#601](https://github.com/parker-brown-family/terminal-delight/issues/601) —
  the card's strip and chip row have not been looked at in a narrow tiled pane.**
  The fifth slice of the approved plan. Worst case is four tabs over four chips,
  both `flex_wrap`.
- **[#602](https://github.com/parker-brown-family/terminal-delight/issues/602) —
  three worktrees hold uncommitted diffs from ended sessions**, snapshotted to
  `refs/attempts/stranded-2026-09-18/{td-attention,td-cs-contract,td-demo}`.
- **[#544](https://github.com/parker-brown-family/terminal-delight/issues/544) —
  the one-window-per-key test flakes** under the parallel run, passes alone.
- The style brief from the top of the session
  (`reports/2026-09-18-chrome-reads-dated.html`) is **unread**: five tokens carry
  the dated look, and there is no `signal` skin file yet.

## Watch out

- **"Tabs across the top" is ambiguous here.** The window's tab strip and the
  response card's groupings are different surfaces. Two correct fixes shipped to
  the wrong one before the third asking. Ask which.
- **A staged demo window takes the operator's clicks.** The first photograph of
  the finished card showed the `next` tab open on a window nobody had pressed —
  a stray click from the desk it borrowed. Assert the at-rest state in a test.
- **`benchdraw` has a guard test that refuses thresholds** in that file
  (`a_renderer_contains_no_decisions`). A `len() > 1` in a renderer will fail CI;
  put the rule in `workbench` as a named predicate with a table test.
- **The lean-ctx root jail follows cwd, not the repo.** Editing in a sibling
  worktree costs a native `Read` per `Edit`, and `cargo fmt` invalidates every
  prior one — run it once, at the end.
- Main moved ~28 pull requests during this session. Re-read before assuming any
  of this is still the newest word on a surface.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-18-the-groupings-were-never-the-outer-tabs.md`
- APES kanban: register-tabs ticket closed with a deliverable; two follow-up tickets opened, cross-linked to #601 / #602
- lean-ctx: `ctx_session decision` breadcrumb (the four changes, the two traps, the two next issues)
- file-memory: `tabs-across-the-top-names-two-surfaces`, `a-demo-window-receives-the-operators-clicks`, `ask-the-busy-sibling-before-claiming-approved-work`
- Session harvest: `handoffs/2026-09-18-workbench-groupings-and-cutovers.cdx`
- Reader-facing report: `reports/2026-09-18-award-cutover.html` (nine figures)
