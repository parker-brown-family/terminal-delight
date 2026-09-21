# Handoff — chrome legibility, and the bare number on the header (2026-09-19)

## Status

Two threads, one landed and one in flight.

- **LANDED** — PR #555, merged as `c63d95b`. Verified in main with
  `git merge-base --is-ancestor bb8b457 origin/main`, not by trusting the merge
  output. Installed as `td-bb8b457-chrome`; `~/.local/bin/terminal-delight`
  points at it. Parker has not relaunched his window yet.
- **IN FLIGHT** — branch `chrome/no-bare-number-on-the-header` in
  `~/Work/td-bench-count`, off `569f452`. Committed? **No — uncommitted at
  handoff time.** `cargo fmt --check` and `cargo clippy --locked -- -D warnings`
  are clean; `cargo test --locked` was still compiling a cold debug profile when
  this was written. **Run it before committing.**

## What's done

### PR #555 — four chrome fixes (merged)

| change | verified by |
|---|---|
| `Skin::panel` carries its own `pad_x`/`pad_y` inset; 7 of 14 bench panels had none and drew their border through their own text | `a_panel_holds_its_contents_off_its_own_border`, broken on purpose → `left: 0px, right: 6px` |
| 42 sites move off the palette's `faint` role onto the skin's meta ink; that ink 0.45 → 0.60 | `the_benchs_meta_ink_is_readable_on_every_builtin_palette`, broken on purpose → `quiet-command: a card's small print is 2.88:1 against its ground`; an independent calculation off the theme TOMLs gives the same 2.88 |
| `Skin::halo` split out of `Skin::ring`; `verb_button` adds only size; `strip_button` takes a control-scale bloom | **no mechanical gate** — a gpui `BoxShadow` list cannot be read back off a built element, and a source-grep test in this repo has already been satisfied once by the comment explaining the line it guarded |
| (already on the branch) tab/chip legibility, `ink_lit` / `edge_rest` / `face_rest` | `a_tab_label_is_legible_on_every_builtin_palette` |

Full suite 1305 + 30 green, CI green (Rust checks 5m18s).

### The bare `2` (uncommitted)

`app/src/pane.rs` — the unlabelled unseen count beside the TERM ⇄ BENCH slider
is gone, along with its `let unseen = …` binding. `app/src/pane/bench.rs` —
`Pane::bench_unseen` deleted; it had one caller. `app/src/workbench.rs` —
`Bench::unseen_total` is now `#[cfg(test)]`, because clippy proved the aggregate
had no other consumer in the shipped binary. The labelled sibling (`2 answers
waiting` / `N held · no agent in this pane`) is untouched.

## How to run / verify

```bash
cd /home/parker/Work/td-bench-count/app && cargo fmt -- --check && cargo clippy --locked -- -D warnings && cargo test --locked
```
```bash
cd /home/parker/Work/td-bench-count/app && cargo build --release && install -m 755 target/release/terminal-delight ~/.local/lib/terminal-delight/td-<sha>-header && ln -sfn ~/.local/lib/terminal-delight/td-<sha>-header ~/.local/bin/terminal-delight
```
```bash
/home/parker/.local/bin/terminal-delight skin --theme /home/parker/.config/terminal-delight/theme.toml
```

That last one is the headless instrument: it prints every resolved ink for a
palette, so a legibility claim is a number rather than a screenshot.

## Not done / next

- **Commit, push and open a PR for `chrome/no-bare-number-on-the-header`** once
  the suite is green. The commit message is drafted at
  `/tmp/claude-1000/-home-parker-Work-terminal-delight/ecf702f9-*/scratchpad/msg2.txt`
  (it will not survive a reboot — rewrite it from the episode if it is gone).
- **#590** — `option_button` paints a second border over the chip's own, the
  shape `verb_button` shed in #555. Falsifiable, labelled `follow-up`, mirrored
  as an APES ticket. **Try to disprove it first**: the `chosen` arm may be
  load-bearing, in which case only the `primary` arm is a duplicate.
- **`ink_faint` at 0.60 sits 0.10 from `ink_dim`.** Two tokens that close
  together are arguably one. The floor put it there; on palettes this dark there
  may be room for only one readable rung below the body text. Not filed — it is
  a design question, not a defect.
- **The overseer (pane 91) was sent the #555 correction and has not visibly
  acted on it.** Its brief still says `1 conflict, in one file` and pins main at
  `fcfbaa6`. Queued, not delivered.

## Watch out

- **`~/Work/td-phosphor` is not mine any more.** I created it at `5f05ca6`; a
  `Merge main…` commit I never made appeared in it, and within the hour another
  pane had switched it to `chrome/theme-ux-pass`. Everything survived, but only
  because it was checked. The brief was copied to `~/Work/reports/` for that
  reason — a `file://` into any worktree can rot before the human clicks it.
- **A screenshot is of a process, not a repository.** Parker's window was running
  `td-7e52557-main`, three installs behind the launcher, so two of his five
  complaints were already fixed. `readlink -f /proc/<pid>/exe` before diagnosing.
- **`ctx_read` is jailed to the session's cwd**, but `ctx_shell` takes an `env`
  map: `LEAN_CTX_EXTRA_ROOTS=/home/parker/Work/td-bench-count` on every call makes
  `sed`, `grep` and `cargo` work in this worktree. Native `Read`/`Edit` are
  unjailed and are what the Edit tool needs.
- **Run `cargo fmt` once, at the end.** It invalidates every prior Read and the
  next Edit then fails with "File has been modified since read".

## Where it's recorded

- APES episode — `apes/projects/terminal-delight/episodes/2026-09-19-furniture-is-not-ink.md`
- Session harvest — `handoffs/2026-09-19-chrome-legibility-and-the-bare-number.cdx`
- Brief — `~/Work/reports/2026-09-18-four-style-bugs-on-a-card.html`
- lean-ctx — `ctx_session` decision, this date
- file-memory — `a-palette-role-is-not-a-text-ink`, `a-screenshot-is-of-a-process-not-a-repository`, plus updates to `the-worktree-and-the-symlink-are-contested` and `the-root-jail-follows-the-cwd-not-the-repo`
- house-style — a dated entry on drawn bug-fix write-ups and reconstruction figures
- PR #555 · issue #590
