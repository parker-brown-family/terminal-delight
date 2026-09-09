# Status: Client-server split

- Gate 1 — Product: APPROVED 2026-09-08 — via annotated brief (13 notes,
  reports/2026-09-08-client-server-gate1.html); "approve with flags", all
  flags ingested into 01-product.md same day
  (out-of-scope section expanded 2026-09-08 on Parker's note: the two refusals now
  carry why-now, cost-to-reverse, and the constraint each puts on Gate 2)
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
- [ ] Slice 3 — GUI attach behind `TD_SESSIOND=1`; the survival harness is
      built here and the flip-gate numbers are measured here.
- [ ] Slice 4 — persistence redirect + orphan adoption.
- [ ] Slice 5 — flip the default, gated on the harness numbers Parker signs.
- [ ] Slice 6 — tie-off: the follow-up issues 03 commits to.

## Where it stands, 2026-09-09

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

Next: slice 3, the GUI attaching behind `TD_SESSIOND=1`, where the two
unmeasured bets finally get measured. Slice 1's two held items belong there
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
  recovery follows the agent session.
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
