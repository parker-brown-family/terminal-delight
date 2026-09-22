# Handoff — one card for the whole round (2026-09-22)

## Status

**PR #691 open, green, MERGEABLE, not merged** — Parker's call. Branch
`bench/one-card-for-the-whole-round`, worktree `~/Work/td-qround`, 3 commits,
pushed, nothing uncommitted. Installed as `td-eff2abc-qround2` and the launcher
repoints to it; **a window bounce is needed to see it** and is safe (the four
wire files are untouched against the running host `td-7d41cb5-main`).

## What's done

An `AskUserQuestion` round is now ONE thing everywhere.

| Change | Verified by |
|---|---|
| One rail row per round (`fold_rounds`, keyed on the round's first step id); `counts()` reads the same function | tests + photographed |
| The review gallery deleted — modal, keys, button, `Peel::Gallery`, `reviewed()`, `review_page` | tests + a guard on the absence |
| Tabs at `Step::Body` under a `QUESTIONS` label, at the top of the card body | photographed |
| SUBMIT is the navigator's **last tab**, one click, no confirmation | tests |
| An answered question stays changeable until the round goes (`settled`, not `answered`) | tests |
| The round opens as a card instead of pinning to the bottom (`round_lands_on`) | tests |
| Card and pin can never both draw (`draws_waiting_block` + `in_same_round`) | tests |
| The duplicate card fixed in `channel::matching` | test reproducing the file road |
| `Answered::Skipped` for a deliberate blank; `answers_so_far` omits rather than sending `""` | tests |
| Auto-send on the last answer removed for any round with a navigator | tests |

1594 tests pass. `clippy --locked -- -D warnings` clean, `cargo fmt --check`
clean, CI green on all four checks.

## How to run/verify

```bash
cd /home/parker/Work/td-qround/app && cargo test --locked --bin terminal-delight
```
```bash
cd /home/parker/Work/td-qround/app && cargo clippy --locked -- -D warnings && cargo fmt -- --check
```

To see it: bounce a Terminal Delight window, then have an agent ask a
multi-question round. To revert the launcher:

```bash
ln -sfn ~/.local/lib/terminal-delight/td-f817439-hindsight ~/.local/bin/terminal-delight
```

To stage it without an agent — seed a real round through the hook's own
transport into `<pane_dir>/inbound.jsonl` (a `question` record plus a `waiting`
record; the `waiting` is what puts it on the file road). **Read the launch log's
third line first** — see Watch out.

## Not done / next

- **The third build's three fixes are not photographed.** Parker tested build
  two and accepted build three for review on the strength of the tests. The
  SUBMIT tab's layout in a narrow pane is unseen.
- **No pointer tool on this box** (`wlrctl`/`ydotool`/`dotool` all absent), so
  any chip that is not an option press cannot be script-clicked. `ctl bench
  choose <n>` covers options only.
- `MEMORY.md` for this project is at 24K against a 24.4K read limit and wants
  compacting — not done, because other sessions append to it concurrently and a
  rewrite would clobber their lines.

## Watch out

- **A staged throwaway window can launch a real `claude --model opus`**, open
  full-size on the output in use, take focus and eat keystrokes. It did, this
  session: Parker typed into it believing it was his pane. The first throwaway
  came up as a SHELL pane, so it is not deterministic. Read the log's third line
  for `launching — claude` before seeding anything, and tear down the process,
  the surfaces dir AND `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`.
- **`ctx_read` refuses sibling worktrees.** The error names the fix
  (`LEAN_CTX_EXTRA_ROOTS=<dir>`); without it you fall to native `Read` and lose
  the re-read discipline — 58 native reads of 5 files this session.
- **Two `paneident` tests fail here and pass in CI.** Not this change; they walk
  the live process tree. Evidence on `terminal-delight#592`.
- `main` has moved since this branch was cut (`bb6a196` → `bc8f12f`); the PR is
  still MERGEABLE, but re-check before landing.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-22-one-card-for-the-whole-round.md`
- APES task: `make-an-askuserquestion-round-one-card-on-the-rail-changeable-and-submittable-mucyyhsp` (done)
- lean-ctx: session `decision` + `finding` recorded
- file-memory: `a-staged-window-is-a-process-on-the-desk`, `the-unit-of-identity-is-what-the-person-sees`, `a-gate-on-the-selection-misses-the-pinned-copy`, `seed-a-real-round-to-photograph-the-question-card`
- Harvest: `handoffs/2026-09-22-one-card-for-the-whole-round.cdx` (288.5K, 555 secrets redacted)
- PR: https://github.com/parker-brown-family/terminal-delight/pull/691
