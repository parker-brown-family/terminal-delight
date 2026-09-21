# Architecture: The workbench drives the agent

Read against `origin/main` at `4b1fb2b` (2026-09-21, after PR #621) in the
worktree `~/Work/td-decouple`, branch `plan/workbench-decouple-async`. Written
by the DECOUPLE-ASYNC pane with Parker AFK; every decision here is his to
overrule and is listed in `00-status.md` and drawn in
`reports/2026-09-21-decouple-async-the-agent-channel.html`.

## The three things, named the way Parker named them

> 1. terminal process 2. terminal pane 3. decoupled workbench VIEW of the
> terminal process that TALKS DIRECTLY TO THE TERMINAL PROCESS

| His word | What it is in this codebase | Owned by |
|---|---|---|
| **terminal process** | the agent (`claude`, `codex`) in a pseudoterminal, plus the shell it runs under | the HOST (`terminal-delight host`), which holds the pty and the authoritative grid (`host.rs:671`, `:684`) |
| **terminal pane** | `TerminalView`: a replica grid fed by VT bytes over the host socket, and the key handler that turns a keystroke into bytes for the pty (`pane.rs:7004 keystroke_bytes` → `pane.rs:4181 send`) | the WINDOW |
| **workbench** | `Bench` and its 24 sibling `wb_*` fields on `TerminalView` (`pane.rs:2083`), drawn by `benchdraw`, fed by `surfacefeed` and — until now — by `screenread` over the pane's grid, writing back through the pane's byte stream | the WINDOW, inside the pane |

**His crux, answered.** *"anything that COULD have gone through the terminal
PANE could be conveyed directly to the process (BUT I MAY BE WRONG ABOUT THE
ADVANTAGE GAINED BY DOING IT SPECIFICALLY THIS WAY) … maybe there is less of a
difference than I am feeling."*

There is a real difference and it is not the one that first appears. The bytes
that reach the process travel the same pipe either way: the pane's byte-stream
connection to the host, which is the only road to a pseudoterminal this
architecture has (nothing bench-related crosses `hostproto`; measured, §Fit).
What changes when the bench "talks to the process" instead of "through the
pane" is three things, and each one is a defect fixed:

1. **The unit.** A message, not a keystroke. The mirror sent every key as it
   was pressed, so the agent's line editor held a second copy of the draft and
   the two drifted (#614, #615, #540). One bracketed paste has no second copy.
2. **The owner.** The channel, not the key handler. A key on the workbench face
   no longer means what the terminal means by it — which is how `ctrl+c` in a
   text box ended a session.
3. **The record.** A journal line, not a screen echo. The bench used to learn
   what happened by reading the grid back (`READING_WINDOW_MS = 8_000` was a
   constant standing in for an observation, #619). Now it wrote the record
   first and the delivery is the second thing.

And in the other direction — process to bench — the difference is total: a
question arrives **whole, before any picker paints**, through the harness's own
hook, instead of one screen at a time off the grid (#619: 0 of 44 rounds
readable while pending). That half needed no cooperation from the model, only
from the harness, and that is what makes it work for *any* agent the harness
runs.

## Fit

| Module | Today | After |
|---|---|---|
| `channel.rs` — **new** | — | TDAC 0.1: the inbound record types and parser, the outbound records, `say_bytes`, `route_for`, the marker, the per-pane `State` (rounds, presses, cursors, the reply rule). No gpui, no pane. |
| `surfacefeed.rs` | sweeps `*.json` per pane, journals actions | also **tails `inbound.jsonl` by byte offset** (`Feed::tail_inbound`), writes the marker (`write_marker`) and the answer file (`write_answers`); `Arrivals` carries `events` beside `posts` |
| `surface.rs` | `Origin::{Unknown, FileDrop, Mcp, Derived, Person}` | `Origin::Hook`, precision equal to an own-agent MCP call, drawn on every card as *arrived through this agent's own hooks · whole* |
| `workbench.rs` | `Line` is a mirror: one string, a caret, select-all; `line_edit(key, ctrl, alt)`; `"enter" => Submit` | `Line` is a document: line breaks, up/down, undo (`UNDO_KEPT`); `line_edit(key, ctrl, alt, shift)`, `shift+enter`/`alt+enter` → `Newline`, `ctrl+z` → `Undo`; `Bench::with_question_mut` |
| `pane/bench.rs` | the talking branch sends every key down the pty, then applies the same edit locally; click = arrow keys; paste flattens newlines; send = `typed_line` | the talking branch edits the document; `ctrl+c` copies, `ctrl+x` cuts, `ctrl+g` interrupts; up on an empty draft recalls; **send = one bracketed paste + `\r`, journaled first**; hook-carried questions answer through `bench_hook_press`; `channel_events`, `bench_beacon`, `journal_out` |
| `pane.rs` | `needs_input` read off the screen at 120 ms | ORed with `wb_channel.has_open_question()`; the ask caption stops trusting the screen latch once a hook has spoken (`wb_asked_by_hook`); the channel resets on the agent-departed edge |
| `main.rs` | `deliver_surfaces` hands posts to panes; `sweep_live_questions` at 1 Hz | also hands `events`; also refreshes the marker |
| `scripts/td-agent-hooks` — **new** | — | the Claude Code adapter: five events → records; the blocking wait and the pre-answer |
| `scripts/install-agent-hooks.sh` — **new** | — | wires it into `~/.claude/settings.json`, `--uninstall` reverses |
| `host.rs`, `hostproto.rs`, `gridwire.rs` | — | **untouched.** The host only upgrades by dying, and nothing here needs it to. |
| `screenread.rs` | the only reader of the grid | unchanged, **demoted to the fallback**: its question becomes a cursor on the hook's card when the two describe the same question, and a card of its own only when no hook spoke |

What is deliberately NOT in this pass, and why:

- **`Bench` stays a field on `TerminalView`.** The 124-reference ownership
  move is the next slice, after the view coupling is gone (this pass) and after
  the tenancy pane's conversation key has landed on `Bench` — two agents
  editing `Bench`'s shape in one afternoon is how a merge eats a day. Once the
  bench neither reads the grid nor types keys, moving the struct out is
  mechanical and can be measured before it is done.
- **Codex and Gemini adapters.** Both have tool-lifecycle hooks; neither has
  been driven. An agent with no adapter is a screen-only agent and the bench
  draws it as one.
- **Screen-reader accessibility.** Out of scope by Parker's decision; a bench
  holding its own document is closer to it than a mirror was, and nothing here
  forecloses it.

## Endpoints

None over MCP and none over the host socket. Three files and one directory,
all in the pane's mailbox, all specified in `docs/spec/td-agent-channel.md`:

| Path | Direction | Writer → reader |
|---|---|---|
| `inbound.jsonl` | process side → window | the hook adapter (and later any CLI) → `Feed::tail_inbound` |
| `outbound.jsonl` | window → the record | `TerminalView::journal_out` → anyone |
| `answers/<tool_use_id>.json` | window → the waiting hook | `surfacefeed::write_answers` → `td-agent-hooks`, polling at 250 ms |
| `bench.json` | window → the hook | `surfacefeed::write_marker`, ~1 Hz → `td-agent-hooks`, before and during a wait |

The **inbox stays pane-addressed** (Gate 1's constraint): `TD_SESSION` and
`TD_PANE_ID` are in the agent's environment because the host stamped them at
spawn, and the hook is the agent's child, so it inherits both and needs to be
told nothing.

## Data

**Inbound records** (seven types; unknown kept and counted): `prompt`,
`question`, `waiting`, `released`, `answered`, `reply`, `notify`. The
`question` carries `tool_input.questions` **verbatim** — every question, every
option, every preview — which is the thing the screen could never see.

**Outbound records** (five): `say` with its `delivery` (`paste` / `flat` /
`held`), `answer` with its `route` (`file` / `keys` / `sentence`), `interrupt`,
`end`, and `keys` — the one impure verb, journaled with its bytes in hex so a
fallback is visible in the record and never silent.

**The answer file** is `{"answers": {"<question text>": "<label>"}}` — the
shape the harness's own answer channel takes. **Measured 2026-09-21**: an
interactive Claude Code 2.1.274, driven under a pseudoterminal from this pane
with a `PreToolUse` hook returning `updatedInput` carrying such a map, never
painted the picker and the model replied `ANSWER=Coffee` — the LAST option,
chosen deliberately so that a default-first pick would have shown. The
transcript of that run is in this session's scratchpad; the payload the hook
received is reproduced in the spec.

**Per-pane state** (`channel::State`): up to 16 rounds, each with what has been
picked per question (`None` unanswered, `Some([])` a multi-select nobody has
ticked — different facts), what the hook said about itself (`waiting_until_ms`,
`released`, `closed`), whether the bench has sent; the screen cursors supplied
for open hook questions; a count of unknown records; and whether the harness
has spoken at all (`heard`), which is what retires the screen latch for the ask
caption.

**The store is not here.** Where a conversation's record lives on disk, and
which conversation a bench shows, is `docs/plans/workbench-follows-the-agent/`.
This channel writes into the mailbox; that plan files from it. The `turns.jsonl`
it proposes gains exact `prompt` and `reply` lines from this channel for free
the day it lands — `Turn::Ask` can carry the hook's words instead of the
scrollback latch's, with `via: hook`.

## Flow

**A person sends a message.** Composer `enter` → `bench_send` → the draft is
taken → `channel::say_bytes(text, bracketed)` where `bracketed` is the replica
terminal's own `BRACKETED_PASTE` mode → `journal_out(Say { delivery })` → the
sent history gains it → `bench_deliver(bytes)` under the three visibility rules
(on screen, an agent in the pane, no dial press in flight), which writes through
`session.notifier.notify` to the host's pty, or holds it — and the record says
`held`.

**A person types.** Every key on the workbench face goes to `Line` and nowhere
else. `ctrl+c` copies the draft; `ctrl+x` cuts it; `ctrl+g` journals an
`interrupt` and sends one `0x03`; `shift+enter` breaks a line; `ctrl+z` undoes;
up on an empty draft recalls the last send. A click seeks the caret locally and
drops the selection. A paste keeps its line breaks. Nothing here touches the
grid or the pty.

**The agent asks.** Claude Code calls `AskUserQuestion` → the `PreToolUse` hook
fires with the whole `tool_input` → `td-agent-hooks` appends `question` to
`inbound.jsonl` → reads `bench.json`:

- **bench open and fresh** → appends `waiting`, polls `answers/<id>.json` at
  250 ms, re-reads the marker each turn → on the file, appends
  `released:answered` and prints the harness's decision JSON
  (`updatedInput` = the tool input plus `answers`) → **the picker never
  paints**, the model receives the labels.
- **otherwise** → appends `released:no-bench` and exits 0 at once → the picker
  paints exactly as it did before this protocol existed.

Meanwhile the window's sweep (1 Hz, `main.rs` surface loop →
`Feed::tail_inbound`) delivers the `question` to the pane → `channel_events` →
`State::take` → one `question` surface per question, `Origin::Hook`, the round
drawn on each → `present`. `needs_input` goes true through the OR, so the tab
badge and the strip say *asking* with no picker on screen.

**A person answers on the bench.** A chip press → `bench_choose` →
`bench_act(Choose, nav)` → the intercept sees the card is the channel's →
`bench_hook_press` → `State::press` records the pick, then `route_for`:

1. **file** — the hook is waiting and its deadline is ahead: when the round is
   complete, `journal_out(Answer { route: file })` then `write_answers` (beside
   and renamed). The hook finds it inside 250 ms.
2. **keys** — the picker has painted and the screen reader supplied a cursor
   (merged onto the hook's card in `live_questions` instead of presenting a
   second card): `journal_out(Keys { bytes_hex, why })` then the same arrow keys
   as before through the same gate.
3. **sentence** — neither: the TDSP §7 line, as before.

Then the round's cards are re-presented with what was pressed. When the tool
returns, `PostToolUse` appends `answered` with the harness's own record, which
closes the round and — if the person answered in the terminal instead — names
what they chose there.

**The agent replies.** `Stop` appends `reply` with `last_assistant_message`.
If the agent presented no `response` surface of its own since the last
`prompt` (`State::saw_response` counts them, `TerminalView::present` reports
them), the reply becomes a `response` surface with the text as its plain brief,
`Origin::Hook`, so the overview feed works for an agent that never heard of
TDSP. If it did present one, the hook's copy is not wanted and is dropped.

**The person's own words.** `UserPromptSubmit` appends `prompt` with the exact
text → the overview caption; once a pane has heard one, the screen latch
(`latch_asked`) no longer overwrites it.

**An agent leaves.** `set_mode`'s departed edge resets the channel state and
the hook flag; the sent history is the person's and stays.

## External

- **Claude Code hooks** — `UserPromptSubmit`, `PreToolUse`/`PostToolUse`
  matched on `AskUserQuestion`, `Stop`, `Notification`; the `updatedInput`
  decision shape; the 600-second default command-hook timeout that bounds the
  wait (`TD_ASK_WAIT_S`, default 540). Read off the 2.1.274 binary and driven
  once under a pseudoterminal; the public reference does not document the
  pre-answer.
- **Environment names**: `TD_SESSION`, `TD_PANE_ID`, `TD_TAG` (stamped by the
  host, unchanged); `XDG_STATE_HOME` (the mailbox root); `TD_ASK_WAIT_S` (the
  hook's own bound).
- **`jq` and `flock`** on the hook's path; absent either, the hook exits 0 and
  the bench is a screen-only bench for that agent.
- **No webhooks, no network, no MCP change.**

## The third cost, answered

Gate 1 left three answers open and asked that the choice be made on purpose:

1. Keep a narrow synchronous path.
2. Give the live-question gesture up.
3. Make the agent ask over the channel.

**Chosen: 3, with 1 kept as the named fallback.** The hook makes 3 possible
without the model's cooperation, and the pseudoterminal run above makes it
measured rather than argued. But an older window, a bench nobody has open, a
harness with no adapter, or a person who walks over to the terminal are all
real, and in each of them the picker paints — so the keys road stays, through
the one gate it always used, and every trip down it is a `keys` line in the
journal with its bytes and its reason. Option 2 was rejected because ending the
agent from the bench is the gesture that starts the tenancy feature, and taking
a working gesture away in the name of purity is the wrong trade when the
impurity can be made visible instead.

## The two features the mirror gave away

- **History recall** — the bench keeps its own sent log (`wb_sent`, 32 deep);
  up on an empty draft walks it. Exact, and it does not need the agent.
- **Slash-command completion** — not offered. Slash commands still *run*:
  Claude Code parses `/model opus` at submit whether it was typed or pasted,
  which the dials rely on already. The menu that completes them is the
  terminal face's, and the composer does not pretend to have one. Parker's
  mockup drew the first of Gate 1's three shapes (ask the agent for
  candidates); it needs a channel verb the harness does not offer a hook for,
  and is left for the day one exists.

## Least confident decisions

1. **`ctrl+g` as the composer's interrupt chord.** Readline's abort, unbound
   on the bench, easy to reach. It is a chord Parker has not pressed; the
   strip's stop control is the primary path and the chord is a convenience.
2. **A multi-select answer is the chosen labels joined by `", "`.** The
   single-select map is measured; the multi-select join is inferred from how
   the tool describes its own result, and has not been driven.
3. **The bench holds a picker only while it is the face on screen.** Switching
   tabs mid-question releases the hook and the picker paints; the card keeps
   working by keys. The alternative — holding for a bench nobody is looking
   at — keeps an agent's menu hostage to a tab.
4. **A hook-sourced `prompt` retires the screen latch for good, per pane.** If
   the adapter is later removed the caption goes stale rather than falling
   back. The cheap fix, if it bites, is a timestamp on the hook's last word.
5. **The reply surface fires only when no `response` arrived since the last
   `prompt`.** An agent that presents its response *after* the `Stop` hook
   fires — which is the order a TD-launched agent would take if it wrote its
   file last — gets both for that turn. Measured order: the file drop happens
   during the turn, before `Stop`; not measured across every harness.
6. **Typing into a recalled draft keeps history mode on**, so up/down keep
   walking history and discard the edit. A person who recalls, edits and
   presses up loses the edit. Small, and noted here rather than solved.
