# Handoff — the workbench's parallel transcript, and replay idempotence (2026-09-21/22)

## Status

**Landed, merged and installed.** Two pull requests, both green on their own head
before merge:

- **#655** → `b7fd57b` — `conversations/<root>.ws`, the transcript itself.
- **#668** → `9ab1e9c` — record identity, the `Writer`, the replay harness.
  Closes **#658**.

Also merged this session, on Parker's instruction, across two consolidation
waves: #639, #626, #608, #591, #589, #640, #642, #644, #651, #652, #653 and then
#656, #657, #654.

`~/.local/bin/terminal-delight` → `td-9ab1e9c-main`. Main has since moved to
`2e40db4` under five open sibling pull requests, so the symlink is one wave
behind again. That is ordinary drift, not a loss.

Nothing of mine is uncommitted. Nothing was committed or pushed during the
tie-off itself.

## What's done

| Piece | How it was verified |
|---|---|
| One append-only JSONL file per conversation, keyed by the ledger's root — `segment`, `ask`, `said`, `asked`, `answered` lines | 1544 unit tests; a live demo window filed all three of segment, ask and surface |
| `origin` (`hook`/`screen`) and `kind` (`person`/`system`) on every ask | Classifier measured against 114 real prompt records: 62 machine envelopes. Fleet-wide afterwards: 128 asks, all `hook`, 57 person / 71 system |
| Records held on the pane until the conversation is bound, then shifted | Found by a live window (the first ask was being lost); test asserts the relative numbering survives the shift |
| `Rec::identity()` + `Writer` refusing a name already in the file | Five mutations, each caught by the test named for it |
| `replayed()` harness — any journal, a fresh writer twice, both records back | Five cases sit on it |
| `terminal-delight conversation <root>` reads a record with no window | Driven against real records on this box |

## How to run/verify

Use a private target dir — a shared one served a stale test binary to a sibling
session this week.

```bash
cd /home/parker/Work/td-ws/app && CARGO_TARGET_DIR=/home/parker/Work/td-ws/target cargo test --bin terminal-delight benchstore
```
```bash
cd /home/parker/Work/td-ws/app && CARGO_TARGET_DIR=/home/parker/Work/td-ws/target cargo clippy --locked -- -D warnings
```
```bash
terminal-delight conversation cc7b69b2-d36e-4104-9d25-69a0b2d72b3b
```

To see the whole fleet's records and the classifier's split on live data:

```bash
jq -r -s '[.[] | select(.t=="ask")] | "person=\([.[] | select(.kind=="person")] | length)  system=\([.[] | select(.kind=="system")] | length)"' ~/.local/state/terminal-delight/conversations/*.ws
```

## Not done / next

- **#674 — drain the mailbox behind a first-sweep sentinel.** Today a filed
  surface stays in the mailbox and is re-swept on every restart. The record
  ignores the repeat now, but the mailbox grows and a retired surface can come
  back. Needs a sentinel to tell an already-filed file from one that predates the
  build.
- **#675 — rebuild question cards on a resumed bench.** The `asked` and
  `answered` lines are in the file and `load` returns only `said` surfaces, so a
  conversation resumed in a new pane comes back without its questions.
- Both are mirrored as APES tickets and carry the `follow-up` label.

## Watch out

- **Both mailbox feeds are replayed in full after a restart.** `Feed.offsets`
  and `Feed.seen` are in memory and nowhere else. Any new consumer that writes to
  disk needs an identity from the record's own stable fields. The hook's `at_ms`
  is stable; `now_ms()` from the reader is not.
- **A pane's first prompt beats its binding by about a second.** `paneident`
  cannot identify an agent process that has just started. Anything filed only
  when the key exists loses the beginning of every conversation.
- **`Bond` must stay `certain`-only for filing.** Two agents under one shell bind
  to one identity and nobody is certain, which is exactly when nothing may be
  written.
- **`~/Work/terminal-delight` is shared and moves under you.** It was on three
  different branches during this session, with other panes' untracked reports in
  it. Never `git add -A` there.
- **`~/Work/td-ws` is outside the lean-ctx root jail**, so `ctx_*` and `sed -n`
  refuse it while `Read`/`Edit` pass. That is why the session is full of python
  heredocs; see `apes#39`.
- Two `paneident` tests fail on any box with agents running. Pre-existing, filed
  twice as #592 and #605.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-21-a-conversation-is-one-file.md`
- APES tickets: `drain-the-mailbox-…-mucbt3fz`, `rebuild-question-cards-…-mucbt3r5`
- Harvest: `handoffs/2026-09-21-parallel-transcript-and-replay.cdx` (838 KB, 7 facts)
- lean-ctx: session decision recorded; `ctx_knowledge` was not bound this session
- Memory: `a-store-with-no-caller-is-a-format-still-open`,
  `a-record-written-by-a-sweep-starts-late`,
  `a-replayed-feed-needs-idempotent-consumers`,
  `the-census-is-stale-by-the-time-you-act-on-it`
- Briefs: `reports/2026-09-21-the-parallel-transcript.html`,
  `reports/2026-09-21-through-the-eye-of-the-needle.html`
