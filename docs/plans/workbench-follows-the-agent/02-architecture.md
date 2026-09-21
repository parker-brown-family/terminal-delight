# Architecture: The workbench follows the agent, not the pane

Read against `origin/main` at `033a7ce` (2026-09-21). Nothing here was read from
the working tree at `~/Work/terminal-delight`, which is 58 commits behind.

## Fit

| Module | Today | After |
|---|---|---|
| `surfacefeed` | sweeps `surfaces/<window session>/<pane id>/*.json`, hands each pane its arrivals | unchanged as a transport. That directory becomes explicitly a **mailbox**: a place agents write to and the window empties |
| `paneident` | binds every pane in one pass, labels the bond, `certain()` filters to what a reader may attribute | the **fallback** filing key, for a machine with no ledger hook. It answers *which conversation is this PANE in*, which is the wrong question when one shell holds two agents |
| new — `tenancy` | — | the primary filing key, read from the agent's own ledger entry by pid: *which conversation is this PROCESS in*. `Chained` / `Unchained` / `Unrecorded` |
| new — `benchstore` | — | owns the per-conversation record on disk: the turn log, the filed surfaces, the notes. **The only writer.** |
| `workbench::Bench` | holds surfaces, keyed by nothing | gains the conversation it is showing; changing it clears and reloads |
| `pane.rs::set_mode` | acts on the agent-arrived edge | acts on both edges. Departure clears the bench; arrival resolves and loads |
| `pane.rs::latch_asked` | keeps the newest human turn, for paint | also opens a turn in the store, so every reply can be captioned later, not only the standing one |
| `benchdraw::asked` / `workbench::ask_lines` | drawn only on the Overview shelf, only for the standing reply | drawn for whatever reply is being read, from that reply's own turn record |

The `Bench` doc comment — *"Per pane, deliberately. A surface belongs to the
conversation that made it, and a shared store would put one agent's diagram on
another agent's bench"* — keeps its second sentence and loses its first. The
reasoning was always right; the key was wrong.

## Endpoints

None new. `present_surface` keeps its signature, `ctl surface` keeps its
arguments, and the briefing every agent is launched with keeps naming the same
drop directory. An agent cannot tell this happened, which is the point: the
contract published in `~/.config/agents/AGENTS.md` and in every agent's system
prompt does not move.

One read path is added for the window's own use (`benchstore::load`), not exposed
over MCP.

## Data

Two directories with two different owners. The split is the isolation.

The key is a **root and a segment, nested rather than concatenated** —
`conversations/<root>/<seq>/`.

**A `/clear` mints a new ROOT. `seq` counts continuations.** Read out of
`scripts/td-agent-ledger` rather than paraphrased, because an earlier draft of
this page had it exactly backwards and then built a rule on the inversion:

```
clear | startup    ->  root=$sid         seq=0          a conversation BEGINS
compact | resume   ->  root=$prev_root   seq=seq+1      a conversation CONTINUES
same id again      ->  root unchanged    seq unchanged  only the clock moved
unknown source     ->  root=$prev_root   seq=seq+1      join marked `unknown`
```

So the only things ever sibling under one root are compactions and in-pane
resumes — the two mints that carry a conversation on. A cleared conversation
lives in a **different root directory** entirely, which is where the guarantee
actually sits.

**Load the whole root, every segment, in order.** `<seq>/` is a sequence within
one conversation, not a boundary, and nothing that should be hidden is ever a
sibling. An earlier draft had the bench load only the current segment, which
would have emptied it **on compaction** — an agent works all morning, compacts at
noon, and its bench goes blank in front of the person still talking to it, with
the load succeeding and the directory read and simply the wrong directory
chosen. Caught by the pane on tab 3 before anything was written.

**Sort `seq` numerically, not by name.** `10` sorts before `2` lexically, so
"name order" is right for nine segments and wrong afterwards, and a nine-segment
conversation is an ordinary week for a long-running agent.

Both halves come from `scripts/td-agent-ledger`, which as of 2026-09-21 records
the SessionStart `source` word (`startup` / `resume` / `clear` / `compact`, all
four confirmed as literals in the 2.1.274 bundle) plus a root/seq/prev chain in a
`lineage.jsonl` that outlives the process. Naming pattern and instrument both
come from the pane on tab 3; adopted rather than re-invented.

**Whether `/clear` mints a new id is UNMEASURED on this machine** — not inferred,
and not assumed either way. There is not one genuine `/clear` in the 600 most
recent transcripts here, so the feature is unexercised and nothing observed says
which way it goes. The `seq` segment is correct under both answers, which is why
it can be built before the measurement lands, and the ledger's source chain
answers it mechanically the next time anyone clears anything.

A correction that cuts the other way, also from tab 3, and worth carrying in its
corrected form rather than its first one. The first report said four of sixteen
live agents had their session id re-minted under a live process. Measured
properly, they had not: SessionStart fired again at 10.0, 33.6, 34.2 and 47.7
hours in, but taking each agent's current ledger id, finding the transcript it
names, and asking for that file's birth time gives a file born **2 to 176 seconds
after the process started** — while the process has run another 67 hours since.
An id minted at the ten-hour mark would name a file born at the ten-hour mark.
Pid reuse is ruled out because the process started before the ledger wrote.

So **a Claude session id looks stable for the life of its process**, on all
sixteen agents measured here. That makes the key simpler than either pane
assumed, and it demotes the lineage chain from a rescue to insurance — which is
the right place for it while `/clear` stays unmeasured.

The ledger's entry shape, already built and backward compatible (`session_id`,
`pid` and `ts` unmoved, so `session::ledger_session_for` keeps working untouched):

```jsonc
{"session_id":"…","pid":123,"ts":…,"root":"…","seq":1,"source":"compact","prev":"…","join":"declared"}
```

`prev` and `join` are **absent** on a first mint rather than empty — three
states, because a join nobody evidenced is not a join nobody needed. An
unrecognised source word keeps the root, increments `seq`, and marks the join
unknown, which errs toward a bench that kept too much: that can be cleared by
hand, and one that emptied during a compaction cannot be got back.

```
$XDG_STATE_HOME/terminal-delight/
  surfaces/<window session>/<pane id>/          MAILBOX  — agents write, window empties
      <surface id>.json
      actions.jsonl                             (unchanged)

  conversations/<root>/<seq>/                   STORE    — window writes, nobody else
      turns.jsonl
      surfaces/<surface id>.json
      notes.jsonl
```

**Nested, and not `<root>-<seq>` as an earlier draft had it.** A session id is a
uuid and uuids contain dashes, so a concatenated name has to be parsed
right-to-left with "the last field must be an integer" — and the moment a
directory arrives without a segment, `470a6cfd-92ed-44b0-ab30-baaa8123780d`
splits into root `470a6cfd-92ed-44b0-ab30` and seq `baaa8123780d`, which is not a
number, so it falls through to a special case. Two conventions in one directory,
told apart by whether the tail happens to parse. A wrong root is a misattribution
at the directory level, which is this feature's own defect reproduced in its
filing system.

Nesting answers that, and the boundary it isolates is `<root>/` — not `<seq>/`.
*A cleared conversation never shows what you cleared* is structural because a
clear mints a new root, so the cleared work is in another directory that is never
opened. That was true of the flat form too; the nesting is bought for the parsing
and the ambiguity, and keeps everything the pane on tab 3 proposed: one directory
per conversation, one `readdir`, no name parsing, no root-with-segment ambiguity,
retention on the root.

Parker's own sketch was `{agent-id}-n.wb`, with *"if you have a pattern SIMILAR
AND BETTER — 100% go with yours"*. This is that pattern with the separator turned
into a directory boundary; `seq` also stays in the surface's record, where it
costs nothing and is read only when somebody asks which segment a surface arrived
under.

**The store is not under a window session, and that is deliberate.** A
conversation outlives the window that hosted it. Resume it tomorrow, in a
different window, in a different pane, and the record is found by the only name
that stayed the same.

`turns.jsonl`, append-only, one line per event:

```jsonc
{"t":"ask","n":7,"at_ms":1758400000000,"lines":["...what the person typed..."]}
{"t":"surface","n":7,"id":"wfta-gate1-response","at_ms":1758400042000,"bond":"declared"}
```

`n` is the turn ordinal. A surface line carries the turn it answered, which is
what lets any card — not only the newest — be captioned with the ask above it.
`bond` records how strongly the pane was bound when the surface was filed, copied
from `paneident`, so a reader can tell a filing made under `Declared` from one
made under `Birth`.

**Unknown is a value in all three files.** A surface that arrives when the pane
has no certain binding is not written to any conversation. It stays in the
mailbox with a marker beside it, and the bench draws it as a witnessed orphan for
the conversation that was live when it landed — never as that conversation's own
work. A surface found on disk with neither a conversation nor a witness is
counted and folded, never drawn by default and never attributed.

## The isolation guarantee, and where it stops

Parker's requirement: *"a UNIQUE ID key for the agent, and no other agent can
access that workbench file."*

What holds by construction:

- **No agent ever writes into `conversations/`.** Agents write to a mailbox and
  the window files from it. There is no code path by which an agent's bytes
  reach another conversation's store, because there is no code path by which an
  agent's bytes reach *any* store directly.
- **The key is never the agent's to claim.** It is read from the process, never
  from the payload. An agent asserting a session id in the JSON it writes changes
  nothing, because the field is not read — the same rule `paneident`'s own doc
  states about `--resume` strings, where a value synthesised elsewhere is never
  laundered into a claim.

### The key is the agent's ledger root, with paneident as the fallback

**This was the wrong way round in an earlier draft of this page**, and the case
that shows it is live on this machine right now.

`paneident` keys by `shell_pid` (`PaneFacts.shell_pid`, and `agent_under(shell)`
resolves *one* agent beneath it). The ledger keys by the agent's own pid. Most of
the time that is a distinction without a difference — 16 of 17 live agents with a
ledger entry have `paneident`'s `session` equal to the ledger's `root`. The
seventeenth is the one that matters:

```
shell 1320940 (bash)
  agent 1384250  session=470a6cfd…  root=cfa9eefd…  bond=declared
  agent 1390176  session=470a6cfd…  root=470a6cfd…  bond=declared
```

Verified here from `/proc` rather than taken on report: both pids are `claude`,
both have `PPid` 1320940, and `ps --ppid 1320940` lists exactly those two.

**Neither instrument is wrong** — both sides read `declared`, the strongest rung.
They answer different questions. `paneident` answers *which conversation is this
PANE in*; the ledger answers *which conversation is this PROCESS in*. A bench
keyed by the pane's binding therefore puts the second agent's surfaces on the
first agent's bench, silently, with every reading saying `declared` — which is
the exact defect this feature exists to remove.

So the order is:

1. **The agent's own ledger entry**, keyed by its pid. `root` is the
   conversation, `seq` the segment.
2. **`paneident::certain`**, for a machine with no hook installed.
3. **Neither** — no filing. Unchanged, and still the honest failure.

`main.rs:4326` already documents the underlying condition in prose — *"two agents
writing one transcript"* — and `bindings_cli` says two agents under one shell are
one pane to every reader in the window. Nobody had drawn the consequence for a
store. Reproduce with `terminal-delight bindings`: compare `session` to `root`,
grouped by `shell_pid`.
- **No binding, no filing.** `certain()` is the gate. A pane bound only by
  `Guess` files nothing under a conversation, so the failure mode is losing an
  attribution, never inventing one.

**The ledger is not an isolation boundary, and now that it is the PRIMARY key
that has to be stated rather than implied.** Its directory is `0700` and its
entries are `0644`, owned by the user — and every agent on this machine runs as
that user, so any process running as Parker can forge any entry. `/proc` cannot
be forged that way, so inverting the precedence traded forgery-resistance for
accuracy. That is the right trade against the threat actually measured — two
honest agents confused by one shell — and it is a trade rather than a free win.

There was a real amplification in the first version, found and fixed by the pane
on tab 3: the hook reads the previous entry back out of that same user-writable
file to compute the next chain, and read `root` unvalidated. A forged root did
not merely sit there, it propagated into every later legitimate mint and would
have been handed to the reader **as a store directory name**. `prev_sid` and
`prev_root` are now filtered on the way in with the same rule as the payload
(`scripts/td-agent-ledger:114–116`), and a test writes `../../etc/passwd` into an
entry between two legitimate mints; removing the filter fails it.

**Both ends validate, on purpose.** The hook filters on the way in and every
reader of a `root` validates it again before building a path from it. A guard at
one end only is a guard the third reader assumes the first one already applied.

What does **not** hold, and must be said plainly:

- **The mailbox is still writable by any process running as this user.** That is
  what makes file-drop work, and it is not being closed. What changes is that a
  stranger's file can no longer become a conversation's record — it can only
  become an orphan. The blast radius shrinks from *misattributed forever* to
  *unattributed, and drawn as such*.
- **Two panes can still share one conversation.** An agent can run two harnesses
  under one shell, and a conversation resumed in two panes at once is two panes
  with one id. Both would show one store. This is correct — it is one
  conversation — but the mailboxes are separate, so arrival order across the two
  is whatever the sweep saw.

The 2026-09-18 jumbling Parker is remembering had two distinct causes, and both
are already fixed on main; naming them keeps this from being read as their
repair. One was the resolver answering `newest_jsonl` for every pane in a shared
directory, closed by `paneident`. The other was two writers of one surface id
racing on a temp file named only after the id, where the loser died with `ENOENT`
and an error message pointing at the wrong cause — closed by naming the temp file
after the writer with a monotonic counter rather than a timestamp. This feature
inherits both fixes and adds a third property: a file that loses its writer's
identity can no longer be adopted by whoever is standing nearby.

## Flow

**A surface arriving.**

1. The sweep loop resolves bindings once per pass — `paneident::certain(&facts, &home)`,
   already computed at `main.rs:5535`.
2. `surfacefeed::Feed::sweep` takes the mailbox files and returns per-pane arrivals.
3. `deliver_surfaces` routes each arrival to its pane, as today.
4. The pane files it: bound conversation → `benchstore::file(conversation, turn, surface)`,
   which writes the surface and appends the `surface` line. No binding → mailbox
   marker only.
5. `Bench::apply` shows it, as today.

**An agent ending.**

1. `set_mode` sees the agent→not-agent edge.
2. `Bench::clear()`. The store is untouched; nothing is deleted.
3. The strip's verb flips to `LAUNCH AGENT`, and the empty panel carries the one
   line naming what the departed conversation left and the press that brings it
   back (Gate 1, treatment B).

**An agent starting, or being resumed.**

1. `set_mode` sees the not-agent→agent edge and the sweep binds the pane.
2. `Bench::set_conversation(id)` — clears whatever was there, then
   `benchstore::load(id)` up to `PANE_HISTORY_CAP` (64), newest last.
3. A conversation that never presented anything loads nothing, and the bench
   draws no rows, no separator and no fold — Gate 1, screen F.

**A human turn.**

1. The pane latches the ask off the scrollback, as it already does
   (`latch_asked`, `screenread::ASKED_LINES` = 6).
2. On the same edge it appends an `ask` line to the bound conversation's
   `turns.jsonl` and increments the turn ordinal.
3. Surfaces filed afterwards carry that ordinal, so opening any card later finds
   its own ask instead of the newest one.

## A shell pane keeps a pane-keyed bench

A pane with no agent in it still has a bench, and it is the one a script writes
to. `terminal-delight surface`, `seed_demo`, the smoke rigs and every "drop a
file and see it appear" path put JSON into a pane's directory with no
conversation anywhere in the picture. Keying the whole bench by conversation
would take that away, and the demo and photography rigs are how this surface gets
looked at at all.

So the rule is per-pane-mode rather than global. **An agent pane's bench is
keyed by conversation. A shell pane's bench stays keyed by pane**, as today, and
nothing about it changes. The transition between the two is already the funnel
this feature uses — `set_mode`'s arrived and departed edges — so a shell pane
that becomes an agent pane swaps keys once, at the edge, and never holds both.

Carve-out from the pane on tab 3. Without it, slice 1 would have broken every rig
that exists to verify slice 1.

## No migration. Old data is not read at all

**Settled by Parker on the annotated brief, note `ask-2-what-happens-to`:**

> Treat them all as essentially old data that will never be migrated… WE DO NOT
> WANT GARBAGE CODE THAT IS TAKING OUT OF DATE AGENTS / PANES AND THEN
> MAINTAINING THAT CODE… so all old (pre-whatever is current!) panes and agents
> will simply take this as an unknown and give a BIG MESSAGE: "this session is
> from a stale version of TERMINAL DELIGHT, suggest starting a new session."

and on the matching table row, `row-every-surface-already-on`:

> Essentially disregard before the change and treat as unknown. We do not want to
> shoehorn migrations of old transcripts etc.

An earlier draft of this page folded pre-change surfaces behind an openable
count. That is withdrawn, and the objection lands exactly where it should:
*visible on demand* requires a reader for the old shape, and that reader is the
maintained code being refused. There is no fold, no count and no parse. The files
are left where they are and collected by retention in the ordinary way.

**What gets built instead:** a pane whose bound conversation has no store, in a
mailbox holding surfaces, is a pre-change session and draws one screen —
`mockups/unknown.html`, screen G. Nothing on disk is opened to decide that; the
absence of a store is the whole test.

**What this does NOT overturn.** Gate 1's below-the-line section stays, because it
covers a different population: a file that arrives *while this conversation is
live* and cannot name its writer. The window witnessed that arrival, which is a
fact it holds without reading anything old. Screen E keeps its one row and loses
its fold.

### The window does not currently hold "witnessed", and has to be taught it

Raised by the pane on tab 3 against the paragraph above, and verified here
against `origin/main` before accepting it.

`Feed` is `{ seen: HashMap<PathBuf, Stamp> }` (`surfacefeed.rs:156`), and `seen`
is in-memory only — `#[derive(Default)]`, touched at 227 and 245, persisted
nowhere. `sweep_pane` calls a file new when `seen` holds no matching stamp. So on
the first sweep after any window start, **every pre-existing file in an inbox is
indistinguishable from one that landed a second ago.** The test that proves it is
`a_fresh_window_reads_the_whole_directory_back`, and its own comment says why
this is deliberate: *"A new window is a new `Feed` with nothing recorded, so this
is the property that makes that true rather than a separate restore path."*

Screen E as specified would therefore draw pre-change data as live unattributable
arrivals on **every fresh window** — which is the occasion a person is most
likely to be looking at it — with a plausible count, no error, and nothing to
tell them. That is the front door, not a back one.

**The amendment is one bit.** Mark the first sweep of each pane directory: a
`HashSet<PathBuf>` beside `seen`, set after the first pass. Files present at that
moment were already there; files appearing on any later sweep were genuinely
witnessed. The two populations then separate by **mechanism** rather than by
rule:

| | What it is | What it costs |
|---|---|---|
| Present at first sweep, nothing to file it under | Screen G | one `read_dir` for a count. Never parsed, never format-checked. No knowledge of the old shape anywhere in the binary — **a count is not a reader**, which is what keeps this on the right side of Parker's refusal |
| Appearing later, writer unnameable | Screen E's row | current format by construction: this version was running when it landed |

It degrades correctly in the case that looked dangerous. A window restarting
mid-conversation puts live surfaces in the "already there" bucket — but those are
attributable, so they load from the conversation store and reach neither screen.
Only unattributable ones are affected.

### The mark has to be durable and exact, not durable and temporal

The version of this amendment above used an in-memory `HashSet` set on each
window run. Recorded here as a least-confident decision, then worked through by
the pane on tab 3 and found wrong, in a way worth keeping written down because
the failure is the shape this whole feature exists to prevent.

An unattributable surface lands at 10:00 in a running window and screen E draws
its row. The window restarts at 11:00. The new run's first sweep finds the file
already sitting there, marks it pre-existing, and moves it to screen G: *"this
session is from a stale version of TERMINAL DELIGHT, suggest starting a new
session."* That sentence is false — the file was written an hour ago by the
current version, possibly by an agent still running in the pane. It is worse than
a wrong count, because it does not merely mislead: it **instructs a person to
abandon a live session on a fabricated cause**, and it does so cheaply, in the
right form, with nothing on screen to argue with.

The bit answered *did this run see it arrive*. Screen G asks *was this written
before the change*. Those are different questions and only one of them has a
stable answer.

**The sentinel.** On the first sweep of a pane directory by a version carrying
this feature, write a file listing the `.json` filenames present at that moment.
Written once, never rewritten. "Pre-existing" is then membership in that list,
and it is the same answer on every run forever.

| | Classified as | Screen |
|---|---|---|
| Named in the sentinel | pre-existing | G |
| Not named, unattributable | witnessed arrival, permanently | E |

A timestamp was considered and rejected: comparing a file's mtime against the
sentinel's races a surface landing *during* the first sweep, and inherits every
question about an mtime being older than the file's arrival. A name list has no
race and no clock in it, and `PANE_DISK_CAP` bounds it at 512 names.

Four properties, each of which someone will otherwise ask about at Gate 4:

1. **It is still not a reader.** The sentinel is one `read_dir` of filenames.
   Nothing is opened, nothing knows the old format, and nothing has to keep
   knowing it. *A count is not a reader* survives intact.
2. **It is invisible to every existing path, provided it is not named `*.json`.**
   Verified on `origin/main`: `sweep_pane` (211) and `cap_pane` (931) each
   `continue` on any extension that is not `json`, and `prune` works at session
   level. A pane directory already holds non-surface entries — `actions.jsonl`,
   and the `pastes/` subdirectory at `pane/bench.rs:1757`. So `.swept` is safe
   and `swept.json` would be read back as a surface by the very sweep it exists
   to inform.
3. **Write once, never rewrite** — stated as a rule because deleting or
   rewriting it makes everything then present look pre-existing, which is the
   per-run failure again, rarer and harder to see. If the sentinel is absent and
   cannot be written, the directory's history is **unknown**: neither screen, and
   say so. Three states, not two.
4. **Every remaining error points the safe way.** A surface wrongly called
   pre-existing is hidden behind a recoverable screen. One wrongly called
   witnessed shows an extra row. Neither invents a conversation for it, which is
   the property the whole feature exists for.

**The sentinel is written into the agent's drop box, and that is a contract
change worth saying out loud.** The published briefing tells every agent that
directory is theirs to write into; a file appearing there that no agent wrote is
exactly the surprise a future agent files a bug about. It is one dotfile, it is
never read by anything the agent can see, and the alternative — a parallel
directory tree keyed by pane — costs more to keep in step than the surprise costs
to document.

### Filing removes the inbox copy — an invariant that does not exist today

The first-sweep bit is only meaningful if the inbox drains. Today the sweep
deletes nothing: the bench is rebuilt by re-reading the whole directory, which is
the durability promise above. If filing leaves the copy behind, then *still in the
inbox* stops distinguishing anything within a day, this version's own filed
surfaces start classifying as pre-existing, and the inbox grows against
`PANE_DISK_CAP` — which is 512 and is a runaway-writer bound, not a retention
policy.

So: **file into the store, then remove from the inbox. Never the reverse.** The
file has to survive until the store write lands, or a crash in between loses the
surface outright.

The primitive already exists — `surfacefeed::retire_surface(dir, name)`
(`surfacefeed.rs:413`), whose doc comment records the same mechanism from the
other direction: *"a retire that only reached the live window would come straight
back on the next restart, because the restore path reads this directory."* One
correction to how this reached me: the sweep never deletes, but the module does
have a delete-one-surface path, so Gate 3 is wiring an existing primitive rather
than writing one.

Retention already exists (`prune`, `DEAD_SESSION_DAYS`, `PANE_DISK_CAP` = 512)
and gains a sibling rule for `conversations/`, keyed on the conversation's own
last activity rather than a window's.

## External

None. No network, no third-party API, no new environment variable. `$TD_SESSION`
and `$TD_PANE_ID` keep their current meanings, which is what keeps the agent-side
contract unchanged.

## Gate 3 boundary between the two panes

The ledger's **writer** is built (tab 3): `scripts/td-agent-ledger`, 18 tests,
mutated three ways to prove they bite. Its **reader** is that pane's too, since
it wrote the writer, and the shape it proposed is taken as-is:

```rust
pub enum Join { Declared, Unknown }

pub struct Chain { pub root: String, pub seq: u32, pub join: Option<Join> }

pub enum Tenancy {
    /// The ledger names the conversation this id belongs to.
    Chained(Chain),
    /// A ledger entry exists and records no chain — a pre-lineage entry.
    Unchained,
    /// No lineage at all: the hook is not installed on this machine.
    Unrecorded,
}

pub fn tenancy_for(pid: u32, home: &Path) -> Tenancy;
pub fn tenancy_of(session_id: &str, home: &Path) -> Tenancy;
```

`Tenancy` rather than `Option<Chain>`, deliberately, and the reasoning is this
machine's own rule: an `Option` invites `unwrap_or_else(|| Chain::root(id))` at
the call site, which collapses *no ledger entry* and *this id is a root* into one
value that nothing downstream can take apart.

**Three states, not two, and the third paid for itself on its first real
call.** `Unrecorded` was the decouple agent's amendment. Running
`terminal-delight bindings <this session's own id>` on this machine returns
`unrecorded`, because the new hook is written and not yet installed, so
`lineage.jsonl` does not exist. A two-state design would have answered
`Unchained` — *this id is its own root* — which is a fabricated finding, produced
on the very first genuine invocation. **Built, not proposed:** `app/src/tenancy.rs`
on `bench/ledger-tenancy` in `~/Work/td-tenancy`, cut from `origin/main`, 16
tests, both CI gates clean. Nothing committed to main yet.

Its own module rather than `session.rs`, which is already 1210 lines and whose
job is synthesising a resume command, not walking a chain.

Everything else at Gate 3 — the store, the bench key, the first-sweep bit, the
screens — is written here.

## The decoupling — what this feature gives it, and what it owes it

**The workbench is being separated from the terminal view.** Parker's decision,
annotated on `~/Downloads/2026-09-21-workbench-text-entry.html` (111,018 bytes,
08:03:58) and planned in `docs/plans/workbench-drives-the-agent/` at 9/10:

> BEFORE WE PROCEED — there is a CRITICAL decision we are making to DECOUPLE
> workbench from the terminal surface — workbench will operate on an API AGAINST
> THE TERMINAL PROCESS SEPARATE from the terminal view… BIG HARD DECISION… but
> let's start it now and rip off the bandaid!

**Provenance, because two of these are not the same kind of statement.** The
block above is an annotation on the brief, verified in the download. The next one
is not — it is a message Parker typed **into that agent's terminal**, quoted in
its status file and confirmed in its transcript (session `469a4074`), opening
*"MAKE SURE ALIGNMENT: WE WILL DECOUPLE WORKBENCH FROM THE TERMINAL!"*:

> Tie off any work in progress, but ultimately … yea don't step over our bounds
> because a re-work will be ultimately necessary.

An instruction to one pane is weaker evidence that it governs *this* plan than an
annotation on a document would be, and only Parker can say whether he meant it to
reach here. Neither string appears in the annotated brief — checked, zero hits.

**Conversation-keying is a prerequisite of that work, not a casualty of it.** A
workbench that is no longer drawn inside a pane has no pane to belong to, so
something has to say which conversation a bench is showing. This plan is that
something. If it did not exist, the decoupling would have to invent it. Said out
loud here so nobody re-derives it from scratch.

**What this feature owes the decoupling, in three parts:**

1. **The inbox cannot become conversation-keyed, ever — not even after the
   split.** `$TD_SESSION` and `$TD_PANE_ID` are stamped at pane spawn, *before
   any agent exists to be named*, and the contract is published in the machine
   agent file, the launch briefing, the MCP catalogue and the surface CLI. So the
   mailbox stays pane-addressed while the store is conversation-addressed. That
   asymmetry is the whole design and it survives the decoupling unchanged.
2. **`bench_deliver` writes raw bytes into the pane's pseudoterminal, and that is
   an open question for the new API rather than an assumption here.** Reading and
   drawing both become an API without much argument; answering a live picker is
   inherently synchronous with a terminal. The four screens do not need the PTY —
   but **`END SESSION` does**, since `bench_end_agent` sends `0x03 0x03`, and that
   is the gesture that *starts* this whole feature. A decoupling that drops the
   PTY path takes it with them.
3. **The seam is 124 references to `self.bench`** across `pane.rs`,
   `pane/bench.rs`, `main.rs` and `theme.rs`; within `pane/bench.rs` the bench
   reaches the pane's `mode` 39 times, the composer 29, `bench_deliver` 9, and
   `pane_id` and the focus handle 4 each. Measured on `origin/main` by the pane on
   tab 3.

**What is immune either way:** the ledger hook and its lineage (bash, `jq`,
coreutils, `$HOME` — four prose comments are its only mentions of a pane or a
window), and the two reader functions, which touch no gpui, no pane and no
window.

**The consequence for Gate 4 — decided by Parker, 2026-09-21.** The slices divide
along that line. The store, the key, the sentinel, the file-then-remove invariant
and the ledger reader are all below the UI and are the half the decoupling needs;
they are built now. The four screens are drawn inside a pane that is about to
stop being where a bench lives, and are **held** until the new API has a shape.
Nothing is discarded — the screens are designed, approved and waiting.

### Reworked against the premise, 2026-09-21

> **The workbench is no longer a direct mirror of the terminal. It uses an API to
> convey human interaction to the terminal process.** — Parker

This is now a premise rather than a forecast, so this plan is designed *inside*
it rather than braced against it. Three consequences, and the first one is the
largest single improvement to this feature since Gate 1.

**The ask stops being scraped, and becomes authored.** Today `latch_asked` reads
the pane's scrollback, keeps exactly one message, and everything downstream
inherits that: `ask_lines` is gated on `standing_in()`, so opening any card off
the rail removes the person's own words entirely, and Gate 3's largest open doubt
was that a latch the window misses shifts every later turn ordinal. Under the
premise the workbench **holds the draft and sends it**, so the turn record is
exact by construction. `turns.jsonl` stops being an observation of a terminal and
becomes a record of what this window did. Scraping survives only as the terminal
face's fallback.

That retires least-confident decision 1 outright, and it is what turns *every
reply carries the ask that prompted it* from best-effort into a property.

**Nothing in the store changes.** `benchstore`, `ConvKey`, the sentinel and
file-then-drain are all below the UI and touch no pane, no window and no gpui.
They are the same code under either architecture, which is exactly why this half
was the half to build first.

**The raw-byte path needs a shape, and the generic version of it is a trap.**
Two gestures put bytes into the pseudoterminal for reasons that have nothing to
do with composing: answering a picker the agent is blocked on, and ending the
agent. The decoupling plan lists three answers and names the cost of option 1 as
*"the one impure verb is the one everyone will reach for."* That is true of a
verb called `keys`. It is not true of verbs named for a situation:

| Verb | When it is legitimate | What retires it |
|---|---|---|
| `interrupt()` | end the turn, or the agent | a host `Request::KillForeground` — already written up as rung two of `docs/plans/bench-kill-and-relaunch`, deferred only because it needs the host upgraded |
| `answer_prompt(choice)` | the agent is blocked on stdin showing a picker | option 3's reply path, the day a question carries a reply address |

Nobody reaches for `answer_prompt` to do something else, because its name
describes a *situation* rather than a capability. A generic keys verb invites
every later feature to type at the terminal and rebuilds the mirror inside the
API. Two situational verbs each know what would replace them, which makes option
1 two temporary things rather than one permanent compromise.

**This matters to this plan specifically** because `END SESSION` is the gesture
that *starts* it — the departure edge that clears the bench is reached by ending
an agent. The decoupling plan is the first document to say so.

*"No more fixes to the mirror"* is a phrase worth quarantining rather than
repeating: it is the decouple plan's own conclusion at its `00-status.md:39`,
drawn from Parker's line and written by that agent, not a sentence of his. It is
a fair paraphrase and it is not a quote, and an earlier draft of this page cited
it as though it were his.

## Least confident decisions

1. **The turn ordinal comes from the window watching the terminal, not from the
   harness.** It is a count of asks the window saw, so a turn it missed shifts
   every later number. An id from the harness would be exact and does not exist
   at this boundary today.
2. **`conversations/` sits beside `surfaces/` rather than inside it.** It buys
   the outliving-a-window property and costs a second retention rule.
3. **Orphan markers live in the mailbox rather than the store.** Keeps the store
   containing only attributed work, at the cost of the mailbox no longer being
   purely append-and-sweep.
4. **A witnessed orphan is scoped to the conversation that was live when it
   arrived, and disappears with it.** The alternative — keeping it as a pane fact
   across conversations — is what screen F exists to forbid, but it does mean a
   file dropped for you by a script is gone from the bench once that agent ends.
5. ~~The first-sweep bit is per window run, not per machine.~~ **Resolved** — it
   was wrong, and the sentinel above replaces it. Kept visible rather than
   deleted because the reasoning is the useful part: a mark that answers *did
   this run see it arrive* cannot answer *was this written before the change*,
   and substituting one for the other put a false instruction on a screen.
6. **The sentinel is a file this feature writes into a directory the published
   contract gives to agents.** One dotfile, invisible to every reader an agent
   has, but it is still the window putting something in the drop box.
