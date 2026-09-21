# Handoff — bench tenancy: the ledger half (2026-09-21)

## Status

**Landed, committed, not pushed.** Two commits in two trees:

- `d4ed5df` — `scripts/td-agent-ledger` + `scripts/td-agent-ledger.test.mjs`, on
  `fix/the-second-click-backs-out-to-your-branch` in `~/Work/terminal-delight`
  (1 ahead of its origin). **Installed and live** at `~/.local/bin/td-agent-ledger`.
- `b619646` — `app/src/tenancy.rs` + the `mod` declaration and `bindings` wiring,
  on `bench/ledger-tenancy` in `~/Work/td-tenancy` (1 ahead of `origin/main`).

The store half, the bench key, the sentinel and the four screens are **not mine**
— they belong to the agent holding `docs/plans/workbench-follows-the-agent/`,
whose Gate 2 is approved and whose Gate 3 is open for the store only.

## What's done

**The hook records why a session id changed.** Each ledger entry now carries the
`SessionStart` `source`, a `root` naming the conversation, a `seq` for how deep the
segment sits, the `prev` id it displaced, and a `join` of `declared` or `unknown`.
`lineage.jsonl` keeps every mint and every end after the per-pid file is deleted at
`SessionEnd`. Verified by 19 tests and by four deliberate mutations, each of which
failed the suite: a false `declared` join, a `/clear` that fails to break the
chain, an unvalidated `source` word, and a forged previous root.

**The reader turns an id into a conversation.** `tenancy_for(pid)` reads the live
entry; `tenancy_of(session_id)` walks the durable lineage for a conversation whose
process is gone. `Tenancy` is `Chained | Unchained | Unrecorded` — three variants,
because *the ledger does not name this id* and *there is no ledger here* are
different findings. Verified by 16 tests, `cargo clippy --locked -- -D warnings`
and `cargo fmt --check`, and by running against the live machine.

## How to run/verify

```bash
node --test /home/parker/Work/terminal-delight/scripts/td-agent-ledger.test.mjs
```
```bash
cd /home/parker/Work/td-tenancy && CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo test --manifest-path app/Cargo.toml tenancy::
```
```bash
cd /home/parker/Work/td-tenancy && CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target cargo clippy --manifest-path app/Cargo.toml --locked -- -D warnings
```
```bash
/home/parker/Work/terminal-delight/app/target/debug/terminal-delight bindings
```

The last one is the live instrument: every row now carries `tenancy`, `root`,
`seq` and `join`, and passing a session id answers for a conversation whose agent
has ended.

## Not done / next

- **The `/clear` question is instrumented and still unanswered.** Zero genuine
  `/clear` invocations exist in the 600 most recent transcripts on this machine,
  so nothing in the corpus can say whether it mints a new id. The hook now records
  it the first time anyone types one.
- **Neither commit is pushed**, and no pull request exists.
- **`cdx-audit`'s `duplicate-action` is broken** — matches a command's first line
  only. Filed as `parker-brown-family/context-delight#20`, acknowledged in the
  local baseline meanwhile.
- **Thirteen abandoned `http.server` processes** are listening across seven
  project trees, oldest up 5d19h. Listed for Parker; none were killed.

## Watch out

- **`~/Work/terminal-delight` is 65 commits behind `origin/main` and shared.**
  Several agents edit it concurrently. Build from `~/Work/td-tenancy` or another
  tree at main; never `git add -A` there.
- **`paneident` and the ledger answer different questions.** The first keys by
  shell pid, the second by agent pid, and one shell on this machine hosts two
  agents — so they disagree on that pane with both reporting `declared`. The
  store key must be the ledger's `root`, with `paneident::certain` as the fallback.
- **A clear makes a new root; only a compaction makes a sibling segment.** Any
  store layout that isolates segments from each other under one root will empty a
  live bench the moment it compacts.
- **The ledger is not an isolation boundary.** The directory is 0700 and the files
  0644, owned by the user, and every agent runs as that user — a same-user write to
  another agent's entry succeeds. Validate a root where it is read, not only where
  it was written.

## Where it's recorded

- APES episode: `…/apes/projects/terminal-delight/episodes/2026-09-21-the-ledger-says-why.md`
- APES tasks: one closed with a deliverable, two open (the store half, the audit defect)
- lean-ctx: `ctx_session(action="decision")` carries the resume breadcrumb
- file-memory: seven entries under `~/.claude/projects/-home-parker-Work-terminal-delight/memory/`
- The brief Parker annotated: `reports/2026-09-21-bench-tenancy.html`; his notes
  live in the `~/Downloads` copy, not the served one
- Session harvest: `handoffs/2026-09-21-bench-tenancy.cdx`
