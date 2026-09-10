# Status: Client-server split

- Gate 1 — Product: APPROVED 2026-09-08 — via annotated brief (13 notes,
  reports/2026-09-08-client-server-gate1.html); "approve with flags", all
  flags ingested into 01-product.md same day
  (out-of-scope section expanded 2026-09-08 on Parker's note: the two refusals now
  carry why-now, cost-to-reverse, and the constraint each puts on Gate 2)
  **AMENDED 2026-09-09 — close semantics.** A close is now deferred rather than
  immediate: one pane held an hour, two or more held four hours, counted in
  panes rather than verbs, capped at the ten most recent, with the processes
  still running throughout and ctrl+shift+z to reopen. Approved in chat
  ("Concur - good work. Send it!") after the fresh-agent review found the final
  close never reaching its save. The amendment is in 01-product.md under "What
  closing means"; Gates 2 and 3 do not yet carry it, and the slice below is
  where it lands.
  **TIMING, 2026-09-10 (the re-cut).** That section of 01-product.md describes
  the iteration *after* the flip, not the build that flips. Close keeps today's
  immediate meaning until the held state and the reopen key land, which is how
  the meaning still ships exactly once — the reason the amendment was ordered
  before the flip in the first place. Anyone reading "What closing means" and
  implementing from it should know it is the next iteration's spec, not this
  one's. The product doc itself is Parker's to amend and has been left alone.
- Gate 2 — Architecture: APPROVED 2026-09-08 — via annotated brief (5 notes,
  reports/2026-09-08-client-server-gate2.html). Trunk: per-session PTY host +
  client-side replica Term, one binary. All five decision-round answers
  recorded in 02-architecture.md; the one amendment: per-window pane cap
  drops to 4 ("8 is nonsense for one task"), legacy over-cap layouts must
  still load without losing running panes. COORDINATION: the main session
  owns these docs — check for a newer note here before editing from another
  session.
- Gate 3 — Program Design: APPROVED 2026-09-08 — "Fully concur all in the
  doc" (chat). 4 domain authors → integrator → 2 adversarial verifiers, 45
  checks; all 6 not-ok findings folded in before approval, notably the
  alt-screen encoder redesign — the original mechanism would have erased
  vim's live grid, caught by source verification before any code existed.
  Decision round recorded in 03-program-design.md. Panel record:
  research-gate3-panel.md.
- Gate 4 — Slice plan: APPROVED 2026-09-08 — Parker: "Go full send", building
  authorised while he is away. Plan in 04-slices.md.

## Slices

- [x] Slice 0 — DONE 2026-09-08 (`3d50f96`). The dispatch allowlist: an
      unknown verb exits 2 instead of opening a window and mutating session
      state (#314); an existing-directory positional claims the reserved
      open-here slot. Five unit tests plus three that run the real binary
      against a throwaway HOME; two of the three fail against the parent
      commit, where `sevre` reaches `open window` at main.rs:18569. Evidence
      posted to #314; not closed, because nothing is merged or installed.
- [~] Slice 1 — mostly done, zero behaviour change so far:
      - [x] `gridwire` (`884faaf`): snapshot encoder + cross-process grid
            hash, 12 round-trip tests. Mutation-tested — seven deliberate
            breakages, six caught (two only after the tests were strengthened),
            the seventh provably equivalent.
      - [x] `socketpty` (`227ddf3`): a socket driven by alacritty's own event
            loop, 4 tests over a socket pair. Resolves the design's
            least-confident #10. Two design errors found and fixed here — the
            dispatch tokens are crate-private (filed as #325) and a hung-up
            socket is never read at all, so the planned EOF detection could
            not have worked.
      - [ ] `attach_in` building a `Session` from a socket — deliberately held
            for slice 3: a `Session` has a `shell_pid`, and an attached pane
            has no local pid. Making that field honest (`Option<u32>`, since
            unknown is not zero) ripples into pane.rs, which belongs with the
            attach work rather than ahead of it.
      - [ ] `SavedNode.pane_id` — held for slice 4, where it is first read.
- [x] Slice 2 — DONE 2026-09-09 (`a3f1227`). `serve` verb (joining the
      allowlist in the commit that gave it a handler), pane table with durable
      ids, session-keyed socket with a peer-uid check, seven control verbs with
      truthful outcomes, per-pane byte streams, the lease-fenced
      snapshot-then-tee handover, SIGHUP close, and env stamping. Eight unit
      tests over real pseudoterminals plus four driving the shipped binary over
      a real socket — including a terminal outliving the window watching it.
      Three things measured rather than reasoned: the plan's lease+lock pairing
      deadlocks (the unfair lock is the one to pair with a lease); a superseded
      client used to tidy away its successor's stream, found only by the
      end-to-end test; and forking tests made unrelated lock tests flake, which
      is filed as #331 because the mechanism also exists in production.
      Still to come here: the protocol contract doc under `docs/protocol/`, the
      relocated mode watcher and checkpoint, and the detached backoff.
- [x] Slice 3 — DONE 2026-09-09. GUI attach behind `TD_SESSIOND=1`: the
      first-ranked live-host tier in session resolution (a running host nobody
      is watching outranks the newest file on disk), `term::attach_in` building
      a `Session` from a socket, orphan adoption, the divergence guard running
      against the host's `grid-check`, and `scripts/td-survival-test.sh`.
      Slice 1's two held items landed here as designed: `Session.shell_pid` is
      `Option<u32>` — with the ripple absorbed honestly at each call site
      rather than by a zero — and `SavedNode.pane_id` came a slice early
      because the bind path is what first reads it.
      **The metric, measured rather than argued:** `gui-kill --cycles 20`, four
      separate runs, losses 0/20 every time — same shell pids, same neovim, same
      btop, same agent pane, first and last line of three thousand still in the
      scrollback, the sticky note still in the layout, and (from the fourth run
      on) the editor's own screen in what a client arriving cold is sent, which
      is the alt-screen case 03 called its least-proven. `floor-control
      --cycles 20` reads 20 losses of 20, so the instrument can see the loss it
      is looking for; the alt-screen assertion was likewise broken on purpose
      and reported its loss before being trusted. `host-kill --cycles 3`
      recovers to exactly today's floor: new pids, the layout read back from the
      file, every program started again. The last two runs were against the
      merged host — the socket-race fix, then the watch verb and the alt-screen
      heal. A fourth leg, `pane-exit`, closes the gap every other leg shared:
      they all measure work SURVIVING, so a window that had stopped hearing
      about endings would have passed all of them while showing dead terminals
      forever. It ends one terminal and requires the window to outlive it, then
      ends the rest and requires the window to close; 0 losses over 3 cycles,
      and proven by breaking it — a window deafened to its replicas' exits
      reports "still up after every one of its terminals had ended". Suite 714
      green (703 unit, 4 dispatch, 7 host socket), with the five-consecutive-run
      stability check taken at 664 before the merges; `cargo fmt --check` and
      `clippy -D warnings` both clean.
- [x] Slice 4 — DONE. The persistence redirect landed first, ahead of the rest.
      The host became the session file's single writer host-side, and for a
      while the branch had two processes writing one file: measured, a window
      that adopted a running terminal wrote three tabs and the host's own
      checkpoint put back the two-tab copy it seeded at boot — without pane ids,
      so the next window started fresh terminals for both leaves and then
      adopted the three already running. Five terminals where three existed; the
      session duplicating itself through the file rather than through a pid. An
      attached window now hands its layout to the host and writes nothing; a
      serverless one is untouched. The hosted flock went with it (#337): while a
      hosted window runs, the session lock is free and the host's own is held, a
      second window is no longer refused and takes the panes while the first is
      told it lost them and stays up, and after both windows are killed the
      session is still owned — by the host, which still has the terminals. And a
      leaf that would respawn now binds an unclaimed pane running exactly its
      resume line, so a layout that has fallen behind cannot start a second copy
      of an agent that is already running. The pane cap is in too: a new split
      stops at four (`MAX_PANES`), while what a tab may HOLD stays at eight
      (`LEGACY_PANE_CEILING`) because layouts written under the old number
      exist and the warp draws eight tubes. Nothing in the load path consults
      either — a loader enforcing the new cap would open a saved session with
      terminals missing — and a `legacy-load` harness leg hand-writes an
      eight-pane tab with no pane ids and requires all eight to come up.
      Slice 4 is complete.
- [x] Slice 4.5 — **the contract core**. Added 2026-09-09 after the fresh-agent
      review (`reports/2026-09-09-client-server-review.html`) and Parker's
      amendment to Gate 1; **re-cut 2026-09-10** by Parker through the overseer,
      on the anchor check (`reports/2026-09-10-slice-45-anchor-check.html`).
      The re-cut: the close-undo half — the held state and the reopen key —
      moves to **after the flip**, and close keeps today's immediate meaning
      until it lands, so the meaning still ships once. What stays here is the
      contract, because it is what everyone meets the moment the default flips.
      - [x] Window steal, per the design that was approved at Gate 2 and written
            out at `03-program-design.md:706-752` and then silently left the
            plan: one window slot, a `window` hello takes it and the previous
            window's pane sinks, and the loser may ask questions and change
            nothing. A tool's hello takes nothing. **One departure from the
            sketch, deliberate:** the loser's control connection stays open. The
            sketch dropped it and had the client read the EOF as `Lost`; the
            client that got built asks the host whether a stream ended or was
            taken, and a window that cannot ask reaps panes that are still
            running — the bug slice 3 fixed. Refusals are per verb instead.
      - [x] `hello` becomes a state machine: a control connection that has not
            negotiated cannot spawn, close, save or shut down. Four verbs, not
            all of them — a launch probes every candidate and questions cost
            nothing. The carve-out is decided and built: a peer refused for
            version skew may still say "checkpoint and stand down". The
            integration test at `app/tests/host_socket.rs:169-188` was the
            thing demonstrating the gap and changed in the same commit.
      - [x] The version-break handoff: a shutdown checkpoints before it stops, a
            probe that is refused for its version reports `Skewed` rather than
            `Unresponsive`, and a launch meeting a skewed host for the session
            it is opening stands it down and waits before starting its own. The
            routing was worse than the review saw — a skewed host read as
            unresponsive, so tier 3 skipped that session and the launch opened a
            stranger, leaving the work in a process nothing could reach.
      - [x] Bounds: `spawn_pane` refuses a sixty-fifth live terminal by name
            (Gate 3's number), and exited panes are kept to the sixteen most
            recent, oldest first. The dead are kept at all because a window that
            has lost a stream asks whether the pane ended or was taken. **Not**
            the held-for-a-person third state — that is the close-undo work's
            spot and is left empty for it.
      - [x] Regressions for all of it, each proven against the behaviour it
            claims to test rather than against a green run: the steal, the gate
            (three tests), the stand-down, both bounds. The one that stays
            honest about itself is the persistence barrier — with the writer
            lock removed, the old shape also failed ten of ten on this machine,
            so the barrier buys determinism rather than a demonstrated
            detection rate.
      - [ ] **Deferred to after the flip, by the re-cut:** the held-close state
            (a third state in the saved layout with a deadline, cap at ten) and
            reopen (ctrl+shift+z, first candidate at relaunch, the F1 line and
            the first-close hint). The anchor check recommends an in-memory v1
            first — held panes in the host's table only, persistence into the
            layout as the follow-up.
- [ ] Slice 5 — flip the default, gated on the harness numbers Parker signs.
      **Both halves of the gate now have numbers, and both pass.**
      Survival: lost sessions = 0, five legs, repeatedly. Latency
      (`scripts/td-echo-bench.sh`, release, 1000 samples a condition): quiet, an
      attached keystroke costs 95µs more at p99 than a local one; at eight panes
      under a realistic load — two thousand lines a second each, in bursts, well
      above a talkative build — it costs 75µs, with not one keystroke in a
      thousand over a millisecond. Both inside the 1ms gate and below what
      anybody can feel.
      At SATURATION (`yes` at full rate, the original measurement) it costs
      2.6ms and 52 keystrokes per thousand cross a millisecond. That is measured
      and printed but not gated: the attached path needs two processes, two
      terminals and two parsers for one stream, so a machine with no spare core
      pays the shortage twice. The host agent's probe settles which it is — the
      noise floor alone moves from 201µs saturating to 2µs realistic, at which
      point the instrument is reporting the scheduler. #340 was filed when only
      the saturating number existed and is closed by its own invalidation
      criterion. Still not run: the eyeball half — no VISIBLE lag at eight
      panes, which is a person's judgement rather than a number.
- [ ] Slice 6 — tie-off: the follow-up issues 03 commits to.

## Where it stands, 2026-09-09 (evening)

Slice 3 is in. A window attaches to a session host behind `TD_SESSIOND=1`, and
the thing the feature exists for is now a number rather than a claim: twenty
kills, nothing lost, twice — against a control leg that loses everything twenty
times out of twenty on today's path.

Two defects were found after the slice was measured, both by pulling on the
host agent's question about exit events, and both are fixed rather than filed. A
superseded window did not freeze — it deleted the panes it lost and quit when it
had lost them all, because a stolen stream and a dead shell arrive as the same
EOF; the window now asks the host which one happened. And the host now closes a
pane's stream when its child exits, which is what makes that question worth
asking for the case that matters. Neither was in the metric, which is why
`pane-exit` exists.

Three things worth carrying forward. The divergence guard is live, not wired for
later: every thirty seconds an attached window asks the host what its own grid
hashes to at a stated point in the stream and compares under the replica's own
fence — and `TD_GUARD_FORCE_MISMATCH=1` proves the repair by running it on a
pane that agrees. The client's own 800ms foreground watcher is gone (#336):
the host says what changed on a listening connection the window opens beside its
control one, and a pane with its own pseudoterminal still asks the kernel
directly. And a hosted window still claims the
session flock, which is the host's job in 02 — the host does not claim it yet,
and until it does the flock is what keeps two windows from writing one session
file; filed as #337, and invalid the moment the Save verb makes the host the
file's writer.

## Where it stood, 2026-09-09 (morning)

Seven commits on `client-server-split`, open as one draft pull request (#328 —
one PR for the task, so a rollback is one revert). Suite green at 614, from a
570 baseline, and verified stable over sixteen consecutive runs after a flake
was tracked down rather than retried.

The host runs. A terminal can be started without a window, attached to,
detached from, and re-attached to, and it survives the window that was
watching it — proven against the shipped binary over a real socket.

Nothing is installed and nothing is merged: the symlink holds another agent's
favourites-shelf build, and installing over it would drop their work (#327,
filed as possibly-nothing with the conditions that would make it real).

Next: slice 5 — the flip, which is now a decision rather than an engineering
task: both halves of the gate have numbers and both pass. Slice 1's two held items belong there
too — an attached pane has no local process id, and making that field honest
ripples into pane.rs.

## Where the work happens

Implementation runs in a git worktree at `~/Work/td-client-server` on branch
`client-server-split`, cut from `main`. The live tree `~/Work/terminal-delight`
is NOT used for this: another agent holds uncommitted work there (the paint
favourites shelf) and its build is the one currently installed. One task, one
branch, one pull request — losing attempts stay out of the review surface.

## Notes for a fresh session

- The feature: split terminal-delight into a long-lived server owning live
  sessions (PTYs, panes, state) and attachable clients (GUI, CLI, MCP).
- Gate 1 answers (Parker, 2026-09-08): problems = sessions survive the GUI +
  headless-first automation (phantom-window class dies); the ONE metric =
  lost sessions = 0 under a scripted kill-relaunch test; work lands in the
  live tree; v1 reach = this machine only; multi-attach explicitly not a v1
  goal (must not make v1 harder).
- Approval amendments (annotated brief, 2026-09-08): scrollback and
  non-resumable foreground programs (vim/htop) count in the metric; close
  semantics decided (pane/tab close kills — intent; app close preserves —
  server keeps everything); server-crash fallback is today's TOML recovery,
  recovery follows the agent session. **Close semantics were amended
  2026-09-09** — the kill is deferred behind a one-hour / four-hour undo
  window; read 01-product.md's "What closing means" before implementing
  anything in the close path.
- Spun off, highest priority, NOT in this feature: the left-bar / workspace
  overhaul (vertical tabs+groups bar, tabs-as-tasks, project→epic hierarchy)
  — issue #319. v1 must not make it harder.
- Gate 2 emphasis (Parker): the grid read path seam is the one to be "extra
  careful" about — done right it unlocks presenting the terminal beyond a
  classic terminal UI downstream. Find and define that seam deliberately.
- These docs live in the LIVE tree (`~/Work/terminal-delight`). The checkout at
  `~/BROWN-FAMILY-SPORTS/Software/terminal-delight` is a stale fork (forked
  ~2026-08-18; see issue #313 "collapse the checkouts") — never build or deploy
  the installed binary from it. The 2026-09-08 phantom-window incident
  (#312–#314) came from exactly that mistake.
- Process: the 4-gate software-factory workflow,
  `~/.claude/skills/software-factory/SKILL.md`. Explicit approval at every
  gate; read every doc in this folder before continuing; resume from the first
  unapproved gate.
- Recon: DONE 2026-09-08 — `research-inventory.md` is the merged inventory
  (current process model, 13 existing IPC seams, 12 ranked coupling
  hot-spots, the state a server must own, reusable assets, 11 consolidated
  Gate 2 questions); `research-maps.md` holds the raw per-subsystem maps with
  file:line refs. Inventory only — no target design; Gate 2 designs against
  it. Its housekeeping flag (a live legacy `state.toml` writer) was checked
  same-day and is dead — residue only, cleanup rides the checkout collapse.
