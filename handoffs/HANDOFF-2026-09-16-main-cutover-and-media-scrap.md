# Handoff — main cutover, branch sweep, and the filming pipeline scrap (2026-09-16)

## Status

Landed and pushed. `main` at `c6c03d8`, working tree clean. Installed binary is
`td-c6c03d8-main`, built from that exact commit, 915 tests green. **Parker must
restart his Terminal Delight window to pick it up** — the PTYs belong to the
systemd-parented session host, so agents survive the client restart.

## What's done

- **Left-bar keyboard traversal is in main and in the binary (#450).** `Ctrl+Alt+↑/↓`
  walks the tree on a ring, `→` opens a branch and steps in, `←` closes and climbs
  out, `1…9` jump to a top-level branch. Four commits that had never left
  `bar/tree-keyboard-traversal`; the installed binary was three branches behind.
  *Verified:* merge conflict in `main.rs` resolved by rerere and re-read by hand at
  `main.rs:12163` before committing; 904 tests at merge time, `tree::tests::
  the_cursor_walks_every_drawn_row_top_to_bottom_and_comes_back_round` among them.
- **Branch sweep (#452).** 24 worktrees, 40 branches claiming to be ahead of main;
  only 4 held content main lacked. Landed eleven reports and handoffs that were
  untracked across six worktrees. *Verified:* every branch re-checked with
  `merge-tree --write-tree` after the merges; only `bar/a-restart-moves-nothing` and
  `spine/pill-per-lane` remained, and both landed from other sessions as #439/#449.
- **The filming pipeline was landed (#451) and removed (#455).** It should never have
  gone to the public repo — see *Watch out*. Removal was scoped: the rig went, the
  three feature panels and the left-bar handoffs stayed. History deliberately NOT
  rewritten. *Verified:* `git cat-file -e origin/main:<path>` on every removed and
  every kept path.
- **The private rig is complete.** `~/Work/td-media-pipeline` (no remote) now also
  holds the run-of-show and the six recorded clips, which had lived nowhere but a
  working tree — the clips because a `.gitignore` rule kept them untracked.

## How to run/verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test --release
```
```bash
git -C /home/parker/Work/terminal-delight diff --stat c6c03d8 origin/main -- app/
```
```bash
ls -l ~/.local/bin/terminal-delight
```

The second must be empty before trusting the installed binary's label. If it is not,
rebuild and install under the *new* sha — never copy an old binary to a new name.

To re-run the sweep:

```bash
for b in $(git for-each-ref --format='%(refname:short)' refs/heads/); do t=$(git merge-tree --write-tree origin/main $b 2>/dev/null) && { d=$(git diff --shortstat origin/main^{tree} $t); [ -n "$d" ] && echo "$b |$d"; }; done
```

## Not done / next

- **#453** — the traversal keys ship with no document naming them, and the help
  screen is localised into nine languages, so an English-only row is not shippable.
- **#457** — the `td-clip` skill still points at `scripts/td-clip.sh` in this repo.
  Rewrite it against `~/Work/td-media-pipeline` or retire it. This is the surface
  that caused the mistake below.
- **#458** — prune the 24 worktrees and ~35 squash-merge residue branches.
- Two uncommitted Rust drafts left deliberately: queue hit-testing in `td-demo`
  (duplicates the shipped attention spine) and `hosted_by_default` in
  `td-cs-contract` (superseded — main settled the flip on `TD_NO_SESSIOND`).
- `stash@{0}` verified superseded (its test
  `the_favourites_verb_shadows_only_what_we_agreed_it_would` is in main) but left
  alone.
- **This handoff and `2026-09-16-main-cutover-and-media-scrap.cdx` are UNCOMMITTED**
  in `handoffs/`. Tie-off records, it does not ship.

## Watch out

- **Run `apes_orient` before substantive work here.** This session did not, and APES
  held a high-priority backlog ticket tagged `security`/`private` stating the
  filming pipeline *"must not exist in the public repository"*. That is the whole
  root cause of #451.
- **A dangling reference is a question, not a finding.** A skill pointing at a
  missing path never says which side is wrong, and in a public repo the absence is
  usually a decision.
- **Branch count is not work count.** `git log main..branch`, three-dot diffs and
  merge conflicts all over-report in a squash-merge repo. A CONFLICT is the *normal*
  shape of a stale merged branch.
- **`cdx-audit`'s `duplicate-action` matches a 60-character command prefix**, so any
  session writing several `git commit -F <scratchpad>` commits trips it falsely.
  Acknowledged in the baseline with that note.
- Seven worktrees still hold untracked copies of documents now committed to main.
  They are redundant, not loose work.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-16-the-cutover-and-the-pipeline-that-should-not-have-shipped.md`
- APES tickets: `…-td-clip-skill-…-mu4bbue0`, `prune-the-24-…-mu4bbwiq`
- GitHub: #450, #451, #452, #455 landed · #453, #457, #458 open
- lean-ctx: `ctx_session` decision, 2026-09-16
- file-memory: `branch-count-is-not-work-count`, `the-filming-pipeline-is-deliberately-out`, `a-dangling-reference-is-a-question`
- Session harvest: `handoffs/2026-09-16-main-cutover-and-media-scrap.cdx`
