# Handoff — the plain brief is the reply the overview opens on (2026-09-21)

## Status

**Landed and installed.** PR
[620](https://github.com/parker-brown-family/terminal-delight/pull/620) merged as
`7d41cb5`, all four CI checks green. Branch `bench/the-plain-brief-is-the-reply`
left on the remote, not deleted; its worktree `td-plain-brief` was removed.
Release binary installed as `~/.local/lib/terminal-delight/td-7d41cb5-main` and
`~/.local/bin/terminal-delight` repointed at it.

**One thing deliberately not done:** the running windows are still on
`td-fc6957f-main`. Parker took the restart himself — *"leave the restart to
me then."* Nothing here has been looked at on a screen.

## What's done

| Change | How it was verified |
|---|---|
| `layman` is the required, always-shown register (`Response::brief`); it titles the row and is the row's subtitle | `a_response_folds_its_aliases_…`, `the_seeded_reply_opens_on_its_reading_group_…` |
| `Register::Tldr` and `Register::Eli5` removed; `Layman` inherits the never-in-`sections` role | `every_register_has_exactly_one_group_and_the_table_says_which`, now exhaustive over six |
| A legacy `tldr` fills the brief when no `layman` came, loses when both arrive, never becomes a section | `a_legacy_gist_fills_the_plain_brief_and_never_becomes_a_register`, both map orders |
| `eli5` files under `Other` labelled `ELI5` | asserted in the aliases test |
| Briefing + MCP blurb name `layman` and no dead key | two new gates reading the **returned string**; each broken on purpose first |
| Spec, agents-md snippet, and machine-global `~/.config/agents/AGENTS.md` rewritten | prose; the two gates cover the machine-readable halves |

`cargo test --locked --no-fail-fast` on the merged tree: 1395/1403 plus 9+4+8+9
in the integration binaries. `clippy --locked -- -D warnings` clean.
`fmt --all --check` clean.

## How to run / verify

```bash
cargo test --locked --no-fail-fast
```
```bash
cargo clippy --locked -- -D warnings
```
```bash
strings -n 40 ~/.local/bin/terminal-delight | grep "A response is a"
```

That last one is the install proof — it must say ``A response is a `layman`
(required)`` and must not mention `tldr` or `eli5`. Run every cargo command from
`app/`; there is no `Cargo.toml` at the repo root.

## Not done / next

- **Nobody has looked at the card on a screen.** Close and reopen a Terminal
  Delight window (the session host keeps the PTYs — never restart the host) and
  the reply card should open on the plain brief with the technical brief one chip
  beside it.
- **Agent panes keep writing `tldr`** until they are relaunched, because the
  briefing lives in their context. That is read, not drawn, and is the whole
  reason the legacy spelling was not refused.
- **The one decision to argue with**, flagged amber throughout: reading a `tldr`
  rather than refusing it. Reversing it is one match arm and its test.
- **Follow-up filed:** the shared `CARGO_TARGET_DIR` correctness hazard — see
  Watch out. GitHub
  [#526](https://github.com/parker-brown-family/terminal-delight/issues/526) plus
  the APES ticket `…-mubeqjdw`.

## Watch out

- **A shared `CARGO_TARGET_DIR` across worktrees serves one worktree's test
  binary to another, and cargo calls the build fresh.** Two tests were in
  `app/src/surface.rs` at named line numbers and absent from the binary; `--list`
  counted 1417 where the source had 1403. Every gate run before that was noticed
  was green and measuring the wrong tree. Cross-check before quoting a result:

  ```bash
  cargo test --locked --bin terminal-delight -- --list | grep -c ': test$'
  ```
  ```bash
  git grep -c '^\s*#\[test\]' HEAD -- 'app/src/*.rs' | awk -F: '{s+=$3} END {print s}'
  ```

  Fix is `cargo clean -p terminal-delight` (5891 files, 13.5 GiB; rebuild under a
  minute).
- **`paneident`'s two process-tree tests are red on this box and green in CI.**
  The file is byte-identical to `origin/main`; one of them expects the pid it
  spawned and finds a live agent process. Already #592 and #605 — do not "fix"
  them here.
- **The primary worktree is shared.** `~/Work/terminal-delight` is on a sibling
  agent's branch and moved twice during this session. This work was done in its
  own worktree for that reason; do not switch the primary's branch.
- **The two new gates must keep reading the returned string**, never the source
  file — an assertion its own source satisfies never fires.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-21-the-plain-brief-is-the-reply.md`
- APES tasks: `…-mubch195` (done, with deliverable) · `…-mubeqjdw` (todo, mirrors #526)
- lean-ctx: `ctx_session` decision, 2026-09-21
- file-memory: `a-retired-key-lives-on-in-the-briefing.md` (new) ·
  `a-fresh-worktree-is-a-cold-gpui-build.md` (corrected — its old "it does not
  corrupt anything" line was wrong)
- Session harvest: `handoffs/2026-09-21-plain-brief-is-the-default.cdx`
