# Seamless restart — the plan

## Why the host is the whole problem

The split gave terminals a life independent of the window, and that half works:
the session-1 client was killed and relaunched five times on 2026-09-11 while
holding twenty panes and sixteen running agents, and lost nothing each time. A
window is disposable by design, and it now behaves that way.

The host is the opposite. It holds every pseudoterminal, so replacing it ends
every terminal — which means:

- **A host-side fix cannot ship.** `encode_snapshot` and the authoritative
  `grid_hash` both run host-side. PR #386 fixes four real defects in the first
  and nothing changed when it was installed, because the process that produces
  snapshots was still the old one (#387).
- **Every host bug is permanent for the life of a session.** A session that has
  been up since 07:37 is running whatever was installed at 07:37, forever.
- **The upgrade decision is a hostage negotiation.** "Is this fix worth twenty
  conversations?" is not a question a deploy should ask.

So the work is not "make the host restart faster". It is: **a host must be able
to hand its pseudoterminals to a replacement process and exit.**

## The handover

A pseudoterminal is a file descriptor. File descriptors move between processes
over a unix socket with `SCM_RIGHTS`. That is the whole mechanism, and it is how
every zero-downtime socket-activated server has worked for thirty years.

```
old host                                  new host
   |  1. exec the new binary, pass it the handover socket
   |----------------------------------------->|
   |  2. for each pane: the master fd + the shell pid + the pane's
   |     recipe, cwd, geometry, mode and note
   |----------------------------------------->|
   |  3. the grid. NOT re-encoded — the bytes the old host would have
   |     sent as a snapshot, or better, the Term's own serialisation
   |----------------------------------------->|
   |  4. new host takes the socket path, old host stops accepting
   |<-----------------------------------------|
   |  5. old host exits. The client notices its control connection
   |     drop and reconnects to the same path.
```

Four things make it non-trivial, and each is a real decision rather than an
implementation detail:

**The reader threads.** Each pane has an alacritty event loop holding the fd and
a `Term`. The old host must stop those cleanly at a point where no bytes are
half-parsed, or the new host inherits a parser mid-escape — which is #378, in a
new and worse place. The honest boundary is the same lease the divergence guard
already takes: stop after a complete read-and-parse cycle, then hand over.

**The grid, not the snapshot.** Re-encoding through `encode_snapshot` would put
the handover behind the very code path being upgraded. The new host should
receive the grid directly — the same `Term` state, serialised — so a handover
cannot be broken by an encoder bug and an encoder fix does not need a handover
that already works.

**The client must not notice.** Its control connection drops and its per-pane
byte streams drop. It already re-attaches on divergence; it does not currently
re-attach on a dropped host. That is #369's sibling and wants the same work.

**Failure must leave the old host alive.** If the new binary cannot start, the
handover aborts and the old host keeps everything. This is the property that
makes the feature safe to use, and it means the old host does not close an fd
until the new host has acknowledged it.

### Why not the alternatives

- *Re-exec in place* (`execve` on self, keeping fds): simpler, and it cannot
  fail safe — a new binary that panics on startup has already replaced the
  process holding the terminals.
- *Never upgrade a host; start new sessions on new builds*: this is today's
  behaviour by default. It means a long-lived session is permanently on old
  code, and long-lived sessions are the entire point of the split.
- *Move the encoder client-side*: would fix #387's encoder half, not the hash
  half, and the host must hash its own grid for the guard to mean anything.

## What "seamless" is worth, tested

A feature like this is verified by a harness or not at all, because every
failure mode is invisible until it costs a session. The leg to add beside the
existing ones in `scripts/td-survival-test.sh`:

**`host-handover`** — stand up a host, spawn panes running identifiable
long-lived processes, record every shell pid and every pane's grid hash, perform
a handover to a *different build*, and require: every shell pid unchanged, every
grid hash unchanged, the client still attached, and zero bytes lost on a pane
that was printing throughout. Then run it against a deliberately broken new
binary and require the old host to survive.

The floor case matters as much: the same script with handover disabled must lose
the panes, or the leg proves nothing. That is the lesson from #381 — a survival
harness with no failing floor was how #377 shipped.

## Sequencing, and why the ownership work comes first

#382 slice 0 is first not because it is the biggest but because it makes
everything after it cheap. While a relaunch can cost panes, every verification
run is a decision about whether to risk the session; once a relaunch is free,
the harness can bounce a hundred times in a loop. The same argument applies to
#379: a repair that has to be run by hand before each restart is a step a test
cannot take.

The handover is fourth rather than first for the same reason in reverse — it is
the largest piece, and doing it on a codebase where a client relaunch still
destroys panes means debugging two lifecycles at once.

## The interim rule, until any of this lands

**A change touching `app/src/gridwire.rs` requires host and client to be
replaced together.** The client alone is worse than not deploying: an encoder
change is inert, and a `grid_hash` change makes the two ends compare different
functions and report divergence on panes that agree. Measured 2026-09-11: 11
diverging panes became 13.

This belongs in the redeploy skill, which is separately known to describe a
deploy layout that no longer exists (#320).
