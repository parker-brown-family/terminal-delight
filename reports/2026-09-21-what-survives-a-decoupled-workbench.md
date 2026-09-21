# What survives a decoupled workbench

A note for whoever takes on decoupling the workbench from the terminal view, and
for the agent building *bench tenancy* alongside it. Written 2026-09-21 against
`origin/main`, which this worktree is 65 commits behind — every number below was
read with `git show origin/main:` rather than off disk here.

The short version: **the two pieces of work want the same thing.** Keying a
workbench by the conversation that filled it is not a change a decoupling has to
survive — it is a thing a decoupling needs done first, because a workbench that
is no longer drawn inside a pane has no pane to belong to.

## What cannot break, because it is not in the application

| Piece | Where it lives | Depends on |
|---|---|---|
| The agent-session ledger + lineage | `scripts/td-agent-ledger`, a Claude Code hook | `jq`, coreutils, `$HOME` |
| The conversation's identity chain | `~/.local/state/terminal-delight/agent-ledger/` | nothing that compiles |
| The ledger reader (`tenancy_for` / `tenancy_of`) | its own module, planned | `std`, a path |

The hook is 190 lines of bash. Its only mentions of a pane or a window are four
prose comments; it would keep working if Terminal Delight were rewritten in
another language, and it is already recording facts no UI change can invalidate.
Eighteen tests, run with `node --test scripts/td-agent-ledger.test.mjs`.

## What the decoupling actually has to move

Measured on `origin/main`: **124 references to `self.bench` across four files** —
`pane.rs`, `pane/bench.rs`, `main.rs`, `theme.rs`. Inside `pane/bench.rs` the
bench reaches for the pane's composer state 29 times, its `bench_deliver` (bytes
into the pseudoterminal) 9 times, its `mode` 12 times, its `pane_id` and focus
handle 4 times each. That is the size of the seam.

> **Corrected.** This said `mode` 39 times until the agent doing the decoupling
> failed to reproduce it and said so. The command was `git grep -c mode`, which
> counts *lines containing the substring* — and the substring catches `model`
> twelve times, `Dial::Model` four, `wb_model` three. The pane's own mode field
> appears 12 times: seven `is_agent()`, three bare, one `label()`, one in a
> closing paren. The inflated number landed in the one category this note argues
> separates cleanly, which is the worst place for it to have been wrong.

Three of those categories separate cleanly and one does not:

- **Drawing** — `mode`, focus, the composer. Presentation, moves with the view.
- **Reading** — what is on the bench. Already a store on disk; becomes an API.
- **Writing back** — `bench_deliver` puts raw bytes into the pane's
  pseudoterminal, which is how a button answers a menu the agent is showing right
  now. **This one is inherently coupled to a terminal**, and an async workbench
  has to decide whether it keeps a synchronous path to the PTY or gives that up.
  Answering a live picker is the feature that depends on it.

## The one contract at risk

Agents address their surfaces by `$TD_SESSION` and `$TD_PANE_ID`, stamped into
every pane's environment at spawn. That contract is published — in the machine
agent file, in the launch briefing, in the MCP catalogue, in the
`terminal-delight surface` command — and every agent on this machine uses it.

It survives a decoupling only if panes keep being spawned with those variables.
If the workbench stops being a pane's child, somebody has to decide whether an
agent still addresses a *pane* or addresses a *conversation*. Note that the
variables cannot simply become conversation-keyed: they are stamped when the
shell starts, before any agent exists to be named. The inbox has to stay
pane-addressed even when the store does not.

## The boundary worth agreeing on now

Both pieces of work need one answer to one question: *which conversation is this
workbench showing?* Proposed as the API between them:

```rust
pub enum Tenancy {
    /// The ledger names the conversation this id belongs to.
    Chained(Chain),
    /// The ledger was read and does not name this id. It is its own root.
    Unchained,
    /// There is no ledger on this machine — nobody has looked. Not a root.
    Unrecorded,
}
pub fn tenancy_for(pid: u32, home: &Path) -> Tenancy;
pub fn tenancy_of(session_id: &str, home: &Path) -> Tenancy;
```

Not `Option<Chain>`, deliberately: an `Option` invites
`unwrap_or_else(|| Chain::root(id))` at the call site, which makes "no ledger
entry" and "this id is a root" the same value and leaves nothing downstream able
to tell them apart.

`Unrecorded` is that same rule one level down, and it is the decoupling agent's
amendment rather than mine. *The hook is installed and has never seen this id* is
a finding about the conversation. *There is no hook on this box* is a finding
about the instrument. A reader that cannot tell them apart draws a root for a
machine where nothing has been measured.

Neither function touches gpui, a pane or a window, so a decoupled workbench calls
them exactly as the current one would.

## What is already decided, and by whom

The five rulings in `reports/2026-09-21-bench-tenancy.html` are Parker's, recorded
against the questions that asked them. The plan of record is
`docs/plans/workbench-follows-the-agent/`, whose Gate 1 is approved and whose
Gate 2 is awaiting approval. Re-deciding any of it is wasted work; the brief
carries the evidence under each answer.

The one thing still unmeasured anywhere: whether `/clear` mints a new session id.
There is not one genuine `/clear` invocation in the 600 most recent transcripts on
this machine, so the corpus cannot say. The hook records the answer the first time
anyone types one.
